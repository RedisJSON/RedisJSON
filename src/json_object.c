/*
 * Copyright (C) 2016-2017 Redis Labs
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

#include "json_object.h"

/* === JSONObjectCtx === */
void resetJSONObjectCtx(JSONObjectCtx *ctx);

/* === Parser === */
static inline void _pushNode(_JsonParserContext *ctx, Node *n) {
    ctx->nodes[ctx->nlen] = n;
    ctx->nlen++;
}

static inline Node *_popNode(_JsonParserContext *ctx) {
    ctx->nlen--;
    return ctx->nodes[ctx->nlen];
}

/* Decalre it. */
static int _AllowedEscapes[];
static int _IsAllowedWhitespace(unsigned c);

inline static int errorCallback(jsonsl_t jsn, jsonsl_error_t err, struct jsonsl_state_st *state,
                                char *errat) {
    _JsonParserContext *jpctx = (_JsonParserContext *)jsn->data;

    jpctx->err = err;
    jpctx->errpos = state->pos_cur;
    jsonsl_stop(jsn);
    return 0;
}

inline static void pushCallback(jsonsl_t jsn, jsonsl_action_t action, struct jsonsl_state_st *state,
                                const jsonsl_char_t *at) {
    _JsonParserContext *jpctx = (_JsonParserContext *)jsn->data;
    Node *n = NULL;
    // only objects (dictionaries) and lists (arrays) create a container on push
    switch (state->type) {
        case JSONSL_T_OBJECT:
            n = NewDictNode(1);
            _pushNode(jpctx, n);
            break;
        case JSONSL_T_LIST:
            n = NewArrayNode(1);
            _pushNode(jpctx, n);
            break;
        default:
            break;
    }
}

inline static void popCallback(jsonsl_t jsn, jsonsl_action_t action, struct jsonsl_state_st *state,
                               const jsonsl_char_t *at) {
    _JsonParserContext *jpctx = (_JsonParserContext *)jsn->data;
    const char *pos = jsn->base + state->pos_begin;  // element starting position
    size_t len = state->pos_cur - state->pos_begin;  // element length

    // popping string and key values means addingg them to the node stack
    if (JSONSL_T_STRING == state->type || JSONSL_T_HKEY == state->type) {
        char *buffer = NULL;  // a temporary buffer for unescaped strings

        // ignore the quote marks
        pos++;
        len--;

        // deal with escapes
        if (state->nescapes) {
            jsonsl_error_t err;
            size_t newlen;

            buffer = RedisModule_Calloc(len, sizeof(char));
            newlen = jsonsl_util_unescape(pos, buffer, len, _AllowedEscapes, &err);
            if (!newlen) {
                RedisModule_Free(buffer);
                errorCallback(jsn, err, state, NULL);
                return;
            }

            pos = buffer;
            len = newlen;
        }

        // push it
        Node *n;
        if (JSONSL_T_STRING == state->type)
            n = NewStringNode(pos, len);
        else
            n = NewKeyValNode(pos, len, NULL);  // NULL is a placeholder for now
        _pushNode(jpctx, n);

        if (buffer) RedisModule_Free(buffer);
    }

    // popped special values are also added to the node stack
    if (JSONSL_T_SPECIAL == state->type) {
        if (state->special_flags & JSONSL_SPECIALf_NUMERIC) {
            if (state->special_flags & (JSONSL_SPECIALf_FLOAT | JSONSL_SPECIALf_EXPONENT)) {
                // convert to double
                double value;
                char *eptr;

                errno = 0;
                value = strtod(pos, &eptr);
                // in lieu of "ERR value is not a double or out of range"
                if ((errno == ERANGE && (value == HUGE_VAL || value == -HUGE_VAL)) ||
                    (errno != 0 && value == 0) || isnan(value) || (eptr != pos + len)) {
                    errorCallback(jsn, JSONSL_ERROR_INVALID_NUMBER, state, NULL);
                    return;
                }
                _pushNode(jpctx, NewDoubleNode(value));
            } else {
                // convert long long (int64_t)
                long long value;
                char *eptr;

                errno = 0;
                value = strtoll(pos, &eptr, 10);
                // in lieu of "ERR value is not an integer or out of range"
                if ((errno == ERANGE && (value == LLONG_MAX || value == LLONG_MIN)) ||
                    (errno != 0 && value == 0) || (eptr != pos + len)) {
                    errorCallback(jsn, JSONSL_ERROR_INVALID_NUMBER, state, NULL);
                    return;
                }

                _pushNode(jpctx, NewIntNode((int64_t)value));
            }
        } else if (state->special_flags & JSONSL_SPECIALf_BOOLEAN) {
            _pushNode(jpctx, NewBoolNode(state->special_flags & JSONSL_SPECIALf_TRUE));
        } else if (state->special_flags & JSONSL_SPECIALf_NULL) {
            _pushNode(jpctx, NULL);
        }
    }

    // anything that pops needs to be set in its parent, except the root element and keys
    if (jpctx->nlen > 1 && state->type != JSONSL_T_HKEY) {
        NodeType p = jpctx->nodes[jpctx->nlen - 2]->type;
        Node *n = NULL;
        switch (p) {
            case N_DICT:
                n = _popNode(jpctx);
                Node_DictSetKeyVal(jpctx->nodes[jpctx->nlen - 1], n);
                break;
            case N_ARRAY:
                n = _popNode(jpctx);
                Node_ArrayAppend(jpctx->nodes[jpctx->nlen - 1], n);
                break;
            case N_KEYVAL:
                n = _popNode(jpctx);
                jpctx->nodes[jpctx->nlen - 1]->value.kvval.val = n;
                n = _popNode(jpctx);
                Node_DictSetKeyVal(jpctx->nodes[jpctx->nlen - 1], n);
                break;
            default:
                break;
        }
    }
}

int CreateNodeFromJSON(JSONObjectCtx *ctx, const char *buf, size_t len, Node **node, char **err) {
    size_t _off = 0, _len = len;
    char *_buf = (char *)buf;
    int is_scalar = 0;

    // munch any leading whitespaces
    while (_off < _len && _IsAllowedWhitespace(_buf[_off])) _off++;

    /* Embed scalars in a list (also avoids JSONSL_ERROR_STRING_OUTSIDE_CONTAINER).
     * Copying is necc. evil to avoid messing w/ non-standard string implementations (e.g. sds), but
     * forgivable because most scalars are supposed to be short-ish.
     */
    if ((is_scalar = ('{' != _buf[_off]) && ('[' != _buf[_off]) && _off < _len)) {
        _len = _len - _off + 2;
        _buf = RedisModule_Calloc(_len, sizeof(char));
        _buf[0] = '[';
        _buf[_len - 1] = ']';
        memcpy(&_buf[1], &buf[_off], len - _off);
    }

    /* Reset all and feed the lexer. */
    resetJSONObjectCtx(ctx);
    jsonsl_feed(ctx->parser, _buf, _len);

    /* Check for lexer errors. */
    sds serr = sdsempty();
    if (JSONSL_ERROR_SUCCESS != ctx->pctx->err) {
        serr = sdscatprintf(serr, "ERR JSON lexer error %s at position %zd",
                            jsonsl_strerror(ctx->pctx->err), ctx->pctx->errpos + 1);
        goto error;
    }

    /* Verify that parsing had ended at level 0. */
    if (ctx->parser->level) {
        serr = sdscatprintf(serr, "ERR JSON value incomplete - %u containers unterminated",
                            ctx->parser->level);
        goto error;
    }

    /* Verify that an element. */
    if (!ctx->parser->stack[0].nelem) {
        serr = sdscatprintf(serr, "ERR JSON value not found");
        goto error;
    }

    /* Finalize. */
    if (is_scalar) {
        // extract the scalar and discard the wrapper array
        Node_ArrayItem(ctx->pctx->nodes[0], 0, node);
        Node_ArraySet(ctx->pctx->nodes[0], 0, NULL);
        Node_Free(_popNode(ctx->pctx));
        RedisModule_Free(_buf);
    } else {
        *node = _popNode(ctx->pctx);
    }

    sdsfree(serr);

    return JSONOBJECT_OK;

error:
    // set error string, if one has been passed
    if (err) {
        *err = rmstrndup(serr, strlen(serr));
    }

    // free any nodes that are in the stack
    while (ctx->pctx->nlen) Node_Free(_popNode(ctx->pctx));

    // if this is a scalar, we need to release the temporary buffer
    if (is_scalar) RedisModule_Free(_buf);

    sdsfree(serr);

    return JSONOBJECT_ERROR;
}

/* === JSON serializer === */

typedef struct {
    sds buf;         // serialization buffer
    int depth;       // current tree depth
    int indent;      // indentation string length
    int noescape;    // Don't \u-escape non-printable characters (whose escape is not required)
    sds indentstr;   // indentaion string
    sds newlinestr;  // newline string
    sds spacestr;    // space string
    sds delimstr;    // delimiter string
} _JSONBuilderContext;

#define _JSONSerialize_Indent(b) \
    if (b->indent)               \
        for (int i = 0; i < b->depth; i++) b->buf = sdscatsds(b->buf, b->indentstr);

static const char twoCharEscape[256] = {0,
                                        ['"'] = '"',
                                        ['\\'] = '\\',
                                        ['/'] = '/',
                                        ['\b'] = 'b',
                                        ['\f'] = 'f',
                                        ['\n'] = 'n',
                                        ['\r'] = 'r',
                                        ['\t'] = 't'};

inline static void _JSONSerialize_StringValue(Node *n, void *ctx) {
    _JSONBuilderContext *b = (_JSONBuilderContext *)ctx;
    size_t len = n->value.strval.len;
    const char *p = n->value.strval.data;

    // Pointer to the beginning of the last 'simple' string. This allows to
    // forego adding char-by-char for longer spans of non-special strings
    const char *simpleBegin = NULL;
#define FLUSH_SIMPLE()                                            \
    if (simpleBegin != NULL) {                                    \
        b->buf = sdscatlen(b->buf, simpleBegin, p - simpleBegin); \
        simpleBegin = NULL;                                       \
    }

    b->buf = sdsMakeRoomFor(b->buf, len + 2);  // we'll need at least as much room as the original
    b->buf = sdscatlen(b->buf, "\"", 1);
    while (len--) {
        char escChr = 0;
        if ((escChr = twoCharEscape[(uint8_t)*p])) {
            FLUSH_SIMPLE();
            char bufTmp[2] = {'\\', escChr};
            b->buf = sdscatlen(b->buf, bufTmp, 2);
        } else {
            if (b->noescape || ((unsigned char)*p > 31 && isprint(*p))) {
                if (!simpleBegin) {
                    simpleBegin = p;
                }
            } else {
                FLUSH_SIMPLE();
                b->buf = sdscatprintf(b->buf, "\\u%04x", (unsigned char)*p);
            }
        }
        p++;
    }

    FLUSH_SIMPLE();
    b->buf = sdscatlen(b->buf, "\"", 1);
}

inline static void _JSONSerialize_BeginValue(Node *n, void *ctx) {
    _JSONBuilderContext *b = (_JSONBuilderContext *)ctx;

    if (!n) {  // NULL nodes are literal nulls
        b->buf = sdscatlen(b->buf, "null", 4);
    } else {
        switch (n->type) {
            case N_BOOLEAN:
                if (n->value.boolval) {
                    b->buf = sdscatlen(b->buf, "true", 4);
                } else {
                    b->buf = sdscatlen(b->buf, "false", 5);
                }
                break;
            case N_INTEGER:
                b->buf = sdscatfmt(b->buf, "%I", n->value.intval);
                break;
            case N_NUMBER:
                b->buf = sdscatprintf(b->buf, "%.17g", n->value.numval);
                break;
            case N_STRING:
                _JSONSerialize_StringValue(n, b);
                break;
            case N_KEYVAL:
                b->buf = sdscatfmt(b->buf, "\"%s\":%s", n->value.kvval.key, b->spacestr);
                break;
            case N_DICT:
                b->buf = sdscatlen(b->buf, "{", 1);
                b->depth++;
                if (n->value.dictval.len) {
                    b->buf = sdscatsds(b->buf, b->newlinestr);
                    _JSONSerialize_Indent(b);
                }
                break;
            case N_ARRAY:
                b->buf = sdscatlen(b->buf, "[", 1);
                b->depth++;
                if (n->value.arrval.len) {
                    b->buf = sdscatsds(b->buf, b->newlinestr);
                    _JSONSerialize_Indent(b);
                }
                break;
            case N_NULL:  // keeps the compiler from complaining
                break;
        }  // switch(n->type)
    }
}

inline static void _JSONSerialize_EndValue(Node *n, void *ctx) {
    _JSONBuilderContext *b = (_JSONBuilderContext *)ctx;
    if (n) {
        switch (n->type) {
            case N_DICT:
                if (n->value.dictval.len) {
                    b->buf = sdscatsds(b->buf, b->newlinestr);
                }
                b->depth--;
                _JSONSerialize_Indent(b);
                b->buf = sdscatlen(b->buf, "}", 1);
                break;
            case N_ARRAY:
                if (n->value.arrval.len) {
                    b->buf = sdscatsds(b->buf, b->newlinestr);
                }
                b->depth--;
                _JSONSerialize_Indent(b);
                b->buf = sdscatlen(b->buf, "]", 1);
                break;
            default:  // keeps the compiler from complaining
                break;
        }
    }
}

inline static void _JSONSerialize_ContainerDelimiter(void *ctx) {
    _JSONBuilderContext *b = (_JSONBuilderContext *)ctx;
    b->buf = sdscat(b->buf, b->delimstr);
    _JSONSerialize_Indent(b);
}

void SerializeNodeToJSON(const Node *node, const JSONSerializeOpt *opt, sds *json) {

    // set up the builder
    _JSONBuilderContext *b = RedisModule_Calloc(1, sizeof(_JSONBuilderContext));
    b->indentstr = opt->indentstr ? sdsnew(opt->indentstr) : sdsempty();
    b->newlinestr = opt->newlinestr ? sdsnew(opt->newlinestr) : sdsempty();
    b->spacestr = opt->spacestr ? sdsnew(opt->spacestr) : sdsempty();
    b->indent = sdslen(b->indentstr);
    b->delimstr = sdsnewlen(",", 1);
    b->delimstr = sdscat(b->delimstr, b->newlinestr);
    b->noescape = opt->noescape;

    NodeSerializerOpt nso = {.fBegin = _JSONSerialize_BeginValue,
                             .xBegin = 0xffff,
                             .fEnd = _JSONSerialize_EndValue,
                             .xEnd = (N_DICT | N_ARRAY),
                             .fDelim = _JSONSerialize_ContainerDelimiter,
                             .xDelim = (N_DICT | N_ARRAY)};

    // the real work
    b->buf = *json;
    Node_Serializer(node, &nso, b);
    *json = b->buf;

    sdsfree(b->indentstr);
    sdsfree(b->newlinestr);
    sdsfree(b->spacestr);
    sdsfree(b->delimstr);
    RedisModule_Free(b);
}

/* JSONObjectContext */
JSONObjectCtx *NewJSONObjectCtx(int levels) {
    JSONObjectCtx *ret = RedisModule_Calloc(1, sizeof(JSONObjectCtx));

    // Parser setup
    if (0 >= levels || JSONSL_MAX_LEVELS < levels) {
        // default to maximium if given a negative, 0 or a value greater than maximum
        ret->levels = JSONSL_MAX_LEVELS;
    } else {
        ret->levels = levels;
    }
    ret->parser = jsonsl_new(ret->levels);
    ret->parser->error_callback = errorCallback;
    ret->parser->action_callback_POP = popCallback;
    ret->parser->action_callback_PUSH = pushCallback;
    jsonsl_enable_all_callbacks(ret->parser);

    // Parser context setup
    ret->pctx = RedisModule_Calloc(1, sizeof(_JsonParserContext));
    ret->pctx->nodes = RedisModule_Calloc(ret->levels, sizeof(Node *));
    ret->parser->data = ret->pctx;

    return ret;
}

void resetJSONObjectCtx(JSONObjectCtx *ctx) {
    _JsonParserContext *jpctx = (_JsonParserContext *)ctx->pctx;
    jpctx->err = JSONSL_ERROR_SUCCESS;
    jpctx->errpos = 0;
    jpctx->nlen = 0;
    ctx->parser->stack[0].nelem = 0;
    jsonsl_reset(ctx->parser);
}

void FreeJSONObjectCtx(JSONObjectCtx *ctx) {
    if (ctx) {
        RedisModule_Free(ctx->pctx->nodes);
        RedisModule_Free(ctx->pctx);
        jsonsl_destroy(ctx->parser);
        RedisModule_Free(ctx);
    }
}

// clang-format off
// from jsonsl.c

/**
 * This table contains entries for the allowed whitespace as per RFC 4627
 */
static int _AllowedWhitespace[0x100] = {
    /* 0x00 */ 0,0,0,0,0,0,0,0,0,                                               /* 0x08 */
    /* 0x09 */ 1 /* <TAB> */,                                                   /* 0x09 */
    /* 0x0a */ 1 /* <LF> */,                                                    /* 0x0a */
    /* 0x0b */ 0,0,                                                             /* 0x0c */
    /* 0x0d */ 1 /* <CR> */,                                                    /* 0x0d */
    /* 0x0e */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,                             /* 0x1f */
    /* 0x20 */ 1 /* <SP> */,                                                    /* 0x20 */
    /* 0x21 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0x40 */
    /* 0x41 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0x60 */
    /* 0x61 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0x80 */
    /* 0x81 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0xa0 */
    /* 0xa1 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0xc0 */
    /* 0xc1 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0xe0 */
    /* 0xe1 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0      /* 0xfe */
};

// adapted for use with jsonsl_util_unescape_ex
/**
 * Allowable two-character 'common' escapes:
 */
static int _AllowedEscapes[0x80] = {
        /* 0x00 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0x1f */
        /* 0x20 */ 0,0,                                                             /* 0x21 */
        /* 0x22 */ 1 /* <"> */,                                                     /* 0x22 */
        /* 0x23 */ 0,0,0,0,0,0,0,0,0,0,0,0,                                         /* 0x2e */
        /* 0x2f */ 1 /* </> */,                                                     /* 0x2f */
        /* 0x30 */ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, /* 0x4f */
        /* 0x50 */ 0,0,0,0,0,0,0,0,0,0,0,0,                                         /* 0x5b */
        /* 0x5c */ 1 /* <\> */,                                                     /* 0x5c */
        /* 0x5d */ 0,0,0,0,0,                                                       /* 0x61 */
        /* 0x62 */ 1 /* <b> */,                                                     /* 0x62 */
        /* 0x63 */ 0,0,0,                                                           /* 0x65 */
        /* 0x66 */ 1 /* <f> */,                                                     /* 0x66 */
        /* 0x67 */ 0,0,0,0,0,0,0,                                                   /* 0x6d */
        /* 0x6e */ 1 /* <n> */,                                                     /* 0x6e */
        /* 0x6f */ 0,0,0,                                                           /* 0x71 */
        /* 0x72 */ 1 /* <r> */,                                                     /* 0x72 */
        /* 0x73 */ 0,                                                               /* 0x73 */
        /* 0x74 */ 1 /* <t> */,                                                     /* 0x74 */
        /* 0x75 */ 1 /* <u> */,                                                     /* 0x75 */
        /* 0x76 */ 0,0,0,0,0,                                                       /* 0x80 */
};

static int _IsAllowedWhitespace(unsigned c) { return c == ' ' || _AllowedWhitespace[c & 0xff]; }

// clang-format on
