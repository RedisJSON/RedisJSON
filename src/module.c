/*
* ReJSON - a JSON data type for Redis
* Copyright (C) 2016 Redis Labs
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

#ifndef REDIS_MODULE_TARGET
#pragma GCC error "ReJSON must be compiled as a Redis module"
#endif

#include <logging.h>
#include <sds.h>
#include <string.h>
#include <util.h>
#include "config.h"
#include "json_object.h"
#include "json_path.h"
#include "object.h"
#include "object_type.h"
#include "redismodule.h"

#define JSONTYPE_ENCODING_VERSION 0
#define JSONTYPE_NAME "OBJECT-RL"
#define RLMODULE_NAME "ReJSON"
#define RLMODULE_DESC "JSON data type for Redis"

#define RM_LOGLEVEL_WARNING "warning"

#define OBJECT_ROOT_PATH "."

#define REJSON_ERROR_PARSE_PATH "ERR error parsing path"
#define REJSON_ERROR_EMPTY_STRING "ERR the empty string is not a valid JSON value"
#define REJSON_ERROR_JSONOBJECT_ERROR "ERR unspecified json_object error (probably OOM)"
#define REJSON_ERROR_SERIALIZE "ERR object serialization to JSON failed"
#define REJSON_ERROR_NEW_NOT_ROOT "ERR new objects must be created at the root"
#define REJSON_ERROR_PATH_NANTYPE "ERR wrong type of path value - expected a number but found %s"
#define REJSON_ERROR_PATH_WRONGTYPE "ERR wrong type of path value - expected %s but found %s"
#define REJSON_ERROR_PATH_NONTERMINAL_KEY "ERR missing key at non-terminal path level"
#define REJSON_ERROR_INDEX_INVALID "ERR array index must be an integer"
#define REJSON_ERROR_INDEX_OUTOFRANGE "ERR index out of range"
#define REJSON_ERROR_VALUE_NAN "ERR value is not a number type"
#define REJSON_ERROR_DICT_SET "ERR could not set key in dictionary"
#define REJSON_ERROR_ARRAY_SET "ERR could not set item in array"
#define REJSON_ERROR_ARRAY_GET "ERR could not get item from array"
#define REJSON_ERROR_DICT_DEL "ERR could not delete from dictionary"
#define REJSON_ERROR_ARRAY_DEL "ERR could not delete from array"
#define REJSON_ERROR_INSERT "ERR could not insert into array"
#define REJSON_ERROR_INSERT_SUBARRY "ERR could not prepare the insert operation"

// == Helpers ==
#define NODEVALUE_AS_DOUBLE(n) (N_INTEGER == n->type ? (double)n->value.intval : n->value.numval)
#define NODETYPE(n) (n ? n->type : N_NULL)

/* Returns the string representation of a the node's type. */
static inline char *NodeTypeStr(const NodeType nt) {
    static char *types[] = {"null", "boolean", "integer", "number", "string", "object", "array"};
    switch (nt) {
        case N_NULL:
            return types[0];
        case N_BOOLEAN:
            return types[1];
        case N_INTEGER:
            return types[2];
        case N_NUMBER:
            return types[3];
        case N_STRING:
            return types[4];
        case N_DICT:
            return types[5];
        case N_ARRAY:
            return types[6];
    }
    return NULL;  // this is never reached
}

/* Check if a search path is the root search path. */
static inline int SearchPath_IsRootPath(const SearchPath *sp) {
    return (1 == sp->len && NT_ROOT == sp->nodes[0].type);
}

/* Stores everything about a resolved path. */
typedef struct {
    const char *spath;  // the path's string
    size_t spathlen;    // the path's string length
    Node *n;            // the referenced node
    Node *p;            // its parent
    SearchPath sp;      // the search path
    PathError err;      // set in case of path error
    int errlevel;       // indicates the level of the error in the path
} JSONPathNode_t;

/* Call this to free the struct's contents. */
void JSONPathNode_Free(JSONPathNode_t *jpn) { SearchPath_Free(&jpn->sp); }

/* Sets n to the target node by path.
 * p is n's parent, errors are set into err and level is the error's depth
 * Returns PARSE_OK if parsing successful
*/
int NodeFromJSONPath(Node *root, const RedisModuleString *path, JSONPathNode_t *jpn) {
    // initialize everything
    jpn->n = NULL;
    jpn->p = NULL;
    jpn->err = E_OK;
    jpn->errlevel = -1;

    // path must be valid from the root or it's an error
    jpn->sp = NewSearchPath(0);
    jpn->spath = RedisModule_StringPtrLen(path, &jpn->spathlen);
    if (PARSE_ERR == ParseJSONPath(jpn->spath, jpn->spathlen, &jpn->sp)) {
        SearchPath_Free(&jpn->sp);
        return PARSE_ERR;
    }

    // if there are any errors return them
    if (!SearchPath_IsRootPath(&jpn->sp)) {
        jpn->err = SearchPath_FindEx(&jpn->sp, root, &jpn->n, &jpn->p, &jpn->errlevel);
    } else {
        // deal with edge case of setting root's parent
        jpn->n = root;
    }

    return PARSE_OK;
}

void ReplyWithPathTypeError(RedisModuleCtx *ctx, NodeType expected, NodeType actual) {
    sds err = sdscatfmt(sdsempty(), REJSON_ERROR_PATH_WRONGTYPE, NodeTypeStr(expected),
                        NodeTypeStr(actual));
    RedisModule_ReplyWithError(ctx, err);
    sdsfree(err);
}

/* Generic path error reply handler */
void ReplyWithPathError(RedisModuleCtx *ctx, const JSONPathNode_t *jpn) {
    // TODO: report actual position in path & literal token
    PathNode *epn = &jpn->sp.nodes[jpn->errlevel];
    sds err = sdsnew("ERR ");
    switch (jpn->err) {
        case E_OK:
            err = sdscat(err, "ERR nothing wrong with path");
            break;
        case E_BADTYPE:
            if (NT_KEY == epn->type) {
                err = sdscatfmt(err, "ERR invalid index '[\"%s\"]' at level %i in path",
                                epn->value.key, jpn->errlevel);
            } else {
                err = sdscatfmt(err, "ERR invalid key '[%i]' at level %i in path", epn->value.index,
                                jpn->errlevel);
            }
            break;
        case E_NOINDEX:
            err = sdscatfmt(err, "ERR index '[%i]' out of range at level %i in path",
                            epn->value.index, jpn->errlevel);
            break;
        case E_NOKEY:
            err = sdscatfmt(err, "ERR key '%s' does not exist at level %i in path", epn->value.key,
                            jpn->errlevel);
            break;
        default:
            err = sdscatfmt(err, "ERR unknown path error at level %i in path", jpn->errlevel);
            break;
    }  // switch (err)
    RedisModule_ReplyWithError(ctx, err);
    sdsfree(err);
}

// == JSONType type methods ==
static RedisModuleType *JSONType;

void *JSONTypeRdbLoad(RedisModuleIO *rdb, int encver) {
    if (encver < 0 || encver > JSONTYPE_ENCODING_VERSION) {
        RedisModule_LogIOError(
            rdb, RM_LOGLEVEL_WARNING,
            "Can't load JSON from RDB due to unknown encoding version %d, expecting %d at most",
            encver, JSONTYPE_ENCODING_VERSION);
        return NULL;
    }
    return ObjectTypeRdbLoad(rdb);
}

void JSONTypeAofRewrite(RedisModuleIO *aof, RedisModuleString *key, void *value) {
    // two approaches:
    // 1. For small documents it makes more sense to serialze the entire document in one go
    // 2. Large documents need to be broken to smaller pieces in order to stay within 0.5GB, but
    // we'll need some meta data to make sane-sized chunks so this gets lower priority atm
    Node *n = (Node *)value;

    // serialize it
    JSONSerializeOpt jsopt = {.indentstr = "", .newlinestr = "", .spacestr = ""};
    sds json = sdsnewlen("\"", 1);
    SerializeNodeToJSON(n, &jsopt, &json);
    json = sdscatlen(json, "\"", 1);
    RedisModule_EmitAOF(aof, "JSON.SET", "scb", key, OBJECT_ROOT_PATH, json, sdslen(json));
    sdsfree(json);
}

// == Module JSON commands ==

/**
* JSON.RESP <key>
* Return the JSON in `key` in RESP.
*
* This command uses the following mapping from JSON to RESP:
* - JSON Null is mapped to the RESP Null Bulk String
* - JSON `false` and `true` values are mapped to the respective RESP Simple Strings
* - JSON Numbers are mapped to RESP Integers or RESP Bulk Strings, depending on type
* - JSON Strings are mapped to RESP Bulk Strings
* - JSON Arrays are represented as RESP Arrays in which first element is the simple string `[`
*   followed by the array's elements
* - JSON Objects are represented as RESP Arrays in which first element is the simple string `{`.
    Each successive entry represents a key-value pair as a two-entries array of bulk strings.
*
* Reply: Array, specifically the JSON's RESP form.
*/
int JSONResp_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if ((argc != 2)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty (reply with null) or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    } else if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        {
            RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
            return REDISMODULE_ERR;
        }
    }

    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    ObjectTypeToRespReply(ctx, objRoot);
    return REDISMODULE_OK;
}

/**
 * JSON.MEMORY <key>
 * Reply: Integer, specifically the memory usage of the key
*/
int JSONMemory_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if ((argc != 2)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty (reply with null) or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    } else if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    RedisModule_ReplyWithLongLong(ctx, ObjectTypeMemoryUsage(objRoot));
    return REDISMODULE_OK;
}

/**
 * JSON.TYPE <key> <path>
 * Reports the type of JSON value at `path`.
 * If the key or path do not exist, null is returned.
 * Reply: Simple string, specifically the type.
*/
int JSONType_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 3) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    }
    if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // make the type-specifc reply, or deal with path errors
    if (E_OK == jpn.err) {
        RedisModule_ReplyWithSimpleString(ctx, NodeTypeStr(NODETYPE(jpn.n)));
    } else if (E_NOINDEX == jpn.err || E_NOKEY == jpn.err) {
        // reply with null if there are **any** non-existing elements along the path
        RedisModule_ReplyWithNull(ctx);
    } else {  // report the path error
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.ARRLEN <key> <path>
 * JSON.OBJLEN <key> <path>
 * JSON.STRLEN <key> <path>
 * Report the length of the JSON value at `path` in `key`.
 *
 * If the `key` does not exist, null is returned.
 *
 * Reply: Integer, specifically the length of the value.
*/
int JSONLen_GenericCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 3) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // the actual command
    const char *cmd = RedisModule_StringPtrLen(argv[0], NULL);

    // key must be empty or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    }
    if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // determine the type of target value based on command name
    NodeType expected, actual = NODETYPE(jpn.n);
    if (!strcasecmp("json.arrlen", cmd))
        expected = N_ARRAY;
    else if (!strcasecmp("json.objlen", cmd))
        expected = N_DICT;
    else  // must be json.strlen
        expected = N_STRING;

    // reply with the length per type, or with an error if the wrong type is encountered
    if (actual == expected) {
        RedisModule_ReplyWithLongLong(ctx, Node_Length(jpn.n));
    } else {
        ReplyWithPathTypeError(ctx, expected, actual);
        goto error;
    }

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.OBJKEYS <key> <path>
 * Return the keys in the object that's referenced by `path`.
 *
 * If the object is empty, or either key or path do not exist then null is returned.
 *
 * Reply: Array, specifically the key names as bulk strings.
*/
int JSONObjKeys_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 3) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    }
    if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_NOINDEX == jpn.err || E_NOKEY == jpn.err) {
        // reply with null if there are **any** non-existing elements along the path
        RedisModule_ReplyWithNull(ctx);
        goto ok;
    } else if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // reply with the object's keys if it is a dictionary, error otherwise
    if (N_DICT == NODETYPE(jpn.n)) {
        int len = Node_Length(jpn.n);
        RedisModule_ReplyWithArray(ctx, len);
        for (int i = 0; i < len; i++) {
            // TODO: need an iterator for keys in dict
            const char *k = jpn.n->value.dictval.entries[i]->value.kvval.key;
            RedisModule_ReplyWithStringBuffer(ctx, k, strlen(k));
        }
    } else {
        ReplyWithPathTypeError(ctx, N_DICT, NODETYPE(jpn.n));
        goto error;
    }

ok:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.SET <key> <path> <json>
 * Sets the JSON value at `path` in `key`
 *
 * For new keys the `path` must be the root. For existing keys, when the entire `path` exists, the
 * value that it contains is replaced with the `json` value. A key (with its respective value) is
 * added to a JSON Object only if it is the last child in the `path`.
 *
 * Reply: Simple string, OK.
*/
int JSONSet_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 4) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY != type && RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // JSON must be valid
    size_t jsonlen;
    const char *json = RedisModule_StringPtrLen(argv[3], &jsonlen);
    if (!jsonlen) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_EMPTY_STRING);
        return REDISMODULE_ERR;
    }

    // Create object from json
    Object *jo = NULL;
    char *jerr = NULL;
    if (JSONOBJECT_OK != CreateNodeFromJSON(json, jsonlen, &jo, &jerr)) {
        if (jerr) {
            RedisModule_ReplyWithError(ctx, jerr);
            free(jerr);
        } else {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
        }
        return REDISMODULE_ERR;
    }

    // validate path against the existing object root, and pretend that the new object is the root
    // if the key is empty
    JSONPathNode_t jpn;
    Object *objRoot =
        (REDISMODULE_KEYTYPE_EMPTY == type ? jo : RedisModule_ModuleTypeGetValue(key));
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }
    int isRootPath = SearchPath_IsRootPath(&jpn.sp);

    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        // new keys must be created at the root
        if (E_OK != jpn.err || !isRootPath) {
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_NEW_NOT_ROOT);
            goto error;
        }
        RedisModule_ModuleTypeSetValue(key, JSONType, jo);
    } else {
        // deal with path errors
        switch (jpn.err) {
            case E_OK:
                // this means we're good to go so set the value according the parent container
                if (isRootPath) {
                    // replacing the root is easy
                    RedisModule_DeleteKey(key);
                    RedisModule_ModuleTypeSetValue(key, JSONType, jo);
                } else if (N_DICT == NODETYPE(jpn.p)) {
                    if (OBJ_OK != Node_DictSet(jpn.p, jpn.sp.nodes[jpn.sp.len - 1].value.key, jo)) {
                        RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_DICT_SET);
                        RedisModule_ReplyWithError(ctx, REJSON_ERROR_DICT_SET);
                        goto error;
                    }
                } else {  // must be an array
                    int index = jpn.sp.nodes[jpn.sp.len - 1].value.index;
                    if (index < 0) index = Node_Length(jpn.p) + index;
                    if (OBJ_OK != Node_ArraySet(jpn.p, index, jo)) {
                        RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_ARRAY_SET);
                        RedisModule_ReplyWithError(ctx, REJSON_ERROR_ARRAY_SET);
                        goto error;
                    }
                    // unlike DictSet, ArraySet does not free so we need to call it explicitly
                    Node_Free(jpn.n);
                }
                break;
            case E_NOKEY:
                // only allow inserting at terminal
                if (jpn.errlevel != jpn.sp.len - 1) {
                    RedisModule_ReplyWithError(ctx, REJSON_ERROR_PATH_NONTERMINAL_KEY);
                    goto error;
                }
                if (OBJ_OK != Node_DictSet(jpn.p, jpn.sp.nodes[jpn.sp.len - 1].value.key, jo)) {
                    RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_DICT_SET);
                    RedisModule_ReplyWithError(ctx, REJSON_ERROR_DICT_SET);
                    goto error;
                }
                break;
            case E_NOINDEX:
            case E_BADTYPE:
            default:
                ReplyWithPathError(ctx, &jpn);
                goto error;
        }  // switch (err)
    }

    JSONPathNode_Free(&jpn);
    RedisModule_ReplyWithSimpleString(ctx, "OK");

    RedisModule_ReplicateVerbatim(ctx);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    if (jo) Node_Free(jo);
    return REDISMODULE_ERR;
}

/**
 * JSON.GET <key> [INDENT indentation-string] [NEWLINE newline-string] [SPACE space-string]
 *                [path ...]
 * Return the value at `path` in JSON serialized form.
 *
 * This command accepts multiple `path`s, and defaults to the value's root when none are given.
 *
 * The following subcommands change the reply's and are all set to the empty string by default:
 *   - `INDENT` sets the indentation string for nested levels
 *   - `NEWLINE` sets the string that's printed at the end of each line
 *   - `SPACE` sets the string that's put between a key and a value
 *
 * Reply: Bulk String, specifically the JSON serialization.
 * The reply's structure depends on the on the number of paths. A single path results in the value
 * being itself is returned, whereas multiple paths are returned as a JSON object in which each path
 * is a key.
*/
int JSONGet_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if ((argc < 2)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty (reply with null) or an object type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithNull(ctx);
        return REDISMODULE_OK;
    } else if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // check for optional arguments
    int pathpos = 2;
    JSONSerializeOpt jsopt = {0};
    if (pathpos < argc) {
        RMUtil_ParseArgsAfter("indent", argv, argc, "c", &jsopt.indentstr);
        if (jsopt.indentstr) {
            pathpos += 2;
        } else {
            jsopt.indentstr = "";
        }
    }
    if (pathpos < argc) {
        RMUtil_ParseArgsAfter("newline", argv, argc, "c", &jsopt.newlinestr);
        if (jsopt.newlinestr) {
            pathpos += 2;
        } else {
            jsopt.newlinestr = "";
        }
    }
    if (pathpos < argc) {
        RMUtil_ParseArgsAfter("space", argv, argc, "c", &jsopt.spacestr);
        if (jsopt.spacestr) {
            pathpos += 2;
        } else {
            jsopt.spacestr = "";
        }
    }

    // initialize the reply
    sds json = sdsempty();

    // validate paths, if none provided default to root
    int npaths = argc - pathpos;
    int jpnslen = 0;
    JSONPathNode_t jpns[MAX(npaths, 1)];  // if no paths then the root
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (!npaths) {  // default to root
        NodeFromJSONPath(objRoot, RedisModule_CreateString(ctx, OBJECT_ROOT_PATH, 1), &jpns[0]);
        jpnslen = 1;
    } else {
        while (jpnslen < npaths) {
            // validate path correctness
            if (PARSE_OK != NodeFromJSONPath(objRoot, argv[pathpos + jpnslen], &jpns[jpnslen])) {
                RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
                goto error;
            }

            // deal with path errors
            if (E_OK != jpns[jpnslen].err) {
                ReplyWithPathError(ctx, &jpns[jpnslen]);
                goto error;
            }

            jpnslen++;
        }  // while (jpnslen < npaths)
    }

    // return the single path's JSON value, or wrap all paths-values as an object
    if (1 == jpnslen) {
        SerializeNodeToJSON(jpns[0].n, &jsopt, &json);
    } else {
        Node *objReply = NewDictNode(jpnslen);
        for (int i = 0; i < jpnslen; i++) {
            Node_DictSet(objReply, jpns[i].spath, jpns[i].n);
        }
        SerializeNodeToJSON(objReply, &jsopt, &json);

        // avoid removing the actual data by resetting the reply dict
        // TODO: need a non-freeing Del
        for (int i = 0; i < objReply->value.dictval.len; i++) {
            objReply->value.dictval.entries[i]->value.kvval.val = NULL;
        }
        Node_Free(objReply);
    }

    // check whether serialization had succeeded
    if (!sdslen(json)) {
        RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_SERIALIZE);
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_SERIALIZE);
        goto error;
    }

    RedisModule_ReplyWithStringBuffer(ctx, json, sdslen(json));

    for (int i = 0; i < jpnslen; i++) {
        JSONPathNode_Free(&jpns[i]);
    }
    sdsfree(json);
    return REDISMODULE_OK;

error:
    for (int i = 0; i < jpnslen; i++) {
        JSONPathNode_Free(&jpns[i]);
    }
    sdsfree(json);
    return REDISMODULE_ERR;
}

/**
 * JSON.MGET <path> <key> [<key> ...]
 * Returns the values at `path` from multiple `key`s. Non-existing keys and non-existing paths are
 * reported as null.
 * Reply: Array of Bulk Strings, specifically the JSON serialization of the value at each key's
 * path.
*/
int JSONMGet_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if ((argc < 2)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    if (RedisModule_IsKeysPositionRequest(ctx)) {
        for (int i = 2; i < argc - 2; i++) RedisModule_KeyAtPos(ctx, i);
        return REDISMODULE_OK;
    }
    RedisModule_AutoMemory(ctx);

    // validate search path
    size_t spathlen;
    const char *spath = RedisModule_StringPtrLen(argv[1], &spathlen);
    JSONPathNode_t jpn;
    jpn.sp = NewSearchPath(0);
    if (PARSE_ERR == ParseJSONPath(spath, spathlen, &jpn.sp)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        goto error;
    }

    // iterate keys
    RedisModule_ReplyWithArray(ctx, argc - 2);
    int isRootPath = SearchPath_IsRootPath(&jpn.sp);
    JSONSerializeOpt jsopt = {0};
    for (int i = 2; i < argc; i++) {
        RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[i], REDISMODULE_READ);

        // key must an object type, empties and others return null like MGET
        int type = RedisModule_KeyType(key);
        if (REDISMODULE_KEYTYPE_EMPTY == type) goto null;
        if (RedisModule_ModuleTypeGetType(key) != JSONType) goto null;

        // follow the path to the target node in the key
        Node *objRoot = RedisModule_ModuleTypeGetValue(key);
        if (isRootPath) {
            jpn.err = E_OK;
            jpn.n = objRoot;
        } else {
            jpn.err = SearchPath_FindEx(&jpn.sp, objRoot, &jpn.n, &jpn.p, &jpn.errlevel);
        }

        // deal with path errors by returning null
        if (E_OK != jpn.err) goto null;

        // serialize it
        sds json = sdsempty();
        SerializeNodeToJSON(jpn.n, &jsopt, &json);

        // check whether serialization had succeeded
        if (!sdslen(json)) {
            sdsfree(json);
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_SERIALIZE);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_SERIALIZE);
            goto error;
        }

        // add the serialization of object for that key's path
        RedisModule_ReplyWithStringBuffer(ctx, json, sdslen(json));
        sdsfree(json);
        continue;

    null:  // reply with null for keys that the path mismatches
        RedisModule_ReplyWithNull(ctx);
    }

    SearchPath_Free(&jpn.sp);
    return REDISMODULE_OK;

error:
    SearchPath_Free(&jpn.sp);
    return REDISMODULE_ERR;
}

/**
 * JSON.DEL <key> <path>
 * Delete the value at `path`.
 *
 * Non-existing keys as well as non-existing paths are ignored. Deleting an object's root is
 * equivalent to deleting the key from Redis.
 *
 * Reply: Integer], specifically the number of paths deleted (0 or 1).
*/
int JSONDel_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 3) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key must be empty or a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type) {
        RedisModule_ReplyWithLongLong(ctx, 0);
        return REDISMODULE_OK;
    } else if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_NOINDEX == jpn.err || E_NOKEY == jpn.err) {
        // reply with 0 if there are **any** non-existing elements along the path
        RedisModule_ReplyWithLongLong(ctx, 0);
        goto ok;
    } else if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // if it is the root then delete the key, otherwise delete the target from parent container
    if (SearchPath_IsRootPath(&jpn.sp)) {
        RedisModule_DeleteKey(key);
    } else if (N_DICT == NODETYPE(jpn.p)) {  // delete from a dict
        const char *dictkey = jpn.sp.nodes[jpn.sp.len - 1].value.key;
        if (OBJ_OK != Node_DictDel(jpn.p, dictkey)) {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_DICT_DEL);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_DICT_DEL);
            goto error;
        }
    } else {  // container must be an array
        int index = jpn.sp.nodes[jpn.sp.len - 1].value.index;
        if (OBJ_OK != Node_ArrayDelRange(jpn.p, index, 1)) {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_ARRAY_DEL);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_ARRAY_DEL);
            goto error;
        }
    }  // if (N_DICT)

    RedisModule_ReplyWithLongLong(ctx, (long long)argc - 2);

ok:
    JSONPathNode_Free(&jpn);
    RedisModule_ReplicateVerbatim(ctx);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.NUMINCRBY <key> <path> <value>
 * JSON.NUMMULTBY <key> <path> <value>
 * Increments/multiplies the value stored under `path` by `value`.
 * `path` must exist path and must be a number value.
 * Reply: String, specifically the resulting JSON number value
*/
int JSONNum_GenericCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if ((argc < 4)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    const char *cmd = RedisModule_StringPtrLen(argv[0], NULL);
    double oval, bval, rz;  // original value, by value and the result
    Object *joval = NULL;   // the by value as a JSON object

    // key must be an object type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // verify that the target value is a number
    if (N_INTEGER != NODETYPE(jpn.n) && N_NUMBER != NODETYPE(jpn.n)) {
        sds err = sdscatfmt(sdsempty(), REJSON_ERROR_PATH_NANTYPE, NodeTypeStr(NODETYPE(jpn.n)));
        RedisModule_ReplyWithError(ctx, err);
        sdsfree(err);
        goto error;
    }
    oval = NODEVALUE_AS_DOUBLE(jpn.n);

    // we use the json parser to convert the bval arg into a value to catch all of JSON's syntices
    size_t vallen;
    const char *val = RedisModule_StringPtrLen(argv[3], &vallen);
    char *jerr = NULL;
    if (JSONOBJECT_OK != CreateNodeFromJSON(val, vallen, &joval, &jerr)) {
        if (jerr) {
            RedisModule_ReplyWithError(ctx, jerr);
            free(jerr);
        } else {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
        }
        goto error;
    }

    // the by value must be a number
    if (N_INTEGER != NODETYPE(joval) && N_NUMBER != NODETYPE(joval)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_VALUE_NAN);
        goto error;
    }
    bval = NODEVALUE_AS_DOUBLE(joval);

    // perform the operation
    if (!strcasecmp("json.numincrby", cmd)) {
        rz = oval + bval;
    } else {
        rz = oval * bval;
    }

    // make an object out of the result per its type
    Node *orz;
    // the result is an integer only if both values were
    if (N_INTEGER == NODETYPE(jpn.n) && N_INTEGER == NODETYPE(joval))
        orz = NewIntNode((int)rz);
    else
        orz = NewDoubleNode(rz);

    // replace the original value with the result depending on the parent container's type
    if (SearchPath_IsRootPath(&jpn.sp)) {
        RedisModule_DeleteKey(key);
        RedisModule_ModuleTypeSetValue(key, JSONType, orz);
    } else if (N_DICT == NODETYPE(jpn.p)) {
        if (OBJ_OK != Node_DictSet(jpn.p, jpn.sp.nodes[jpn.sp.len - 1].value.key, orz)) {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_DICT_SET);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_DICT_SET);
            goto error;
        }
    } else {  // container must be an array
        int index = jpn.sp.nodes[jpn.sp.len - 1].value.index;
        if (index < 0) index = Node_Length(jpn.p) + index;
        if (OBJ_OK != Node_ArraySet(jpn.p, index, orz)) {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_ARRAY_SET);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_ARRAY_SET);
            goto error;
        }
        // unlike DictSet, ArraySet does not free so we need to call it explicitly
        Node_Free(jpn.n);
    }
    jpn.n = orz;

    // reply with the serialization of the new value
    JSONSerializeOpt jsopt = {0};
    sds json = sdsempty();
    SerializeNodeToJSON(jpn.n, &jsopt, &json);
    RedisModule_ReplyWithStringBuffer(ctx, json, sdslen(json));
    sdsfree(json);

    Node_Free(joval);
    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    Node_Free(joval);
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.STRAPPEND <key> <path> <json-string>
 * Append the `json-string` value(s) the string at `path`.
 * Reply: Integer, specifically the string's new length.
*/
int JSONStrAppend_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 4) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key can't be empty and must be a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type || RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // the target must be a string
    if (N_STRING != NODETYPE(jpn.n)) {
        ReplyWithPathTypeError(ctx, N_STRING, NODETYPE(jpn.n));
        goto error;
    }

    // JSON must be valid
    size_t jsonlen;
    const char *json = RedisModule_StringPtrLen(argv[3], &jsonlen);
    if (!jsonlen) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_EMPTY_STRING);
        goto error;
    }

    // make an object from the JSON value
    Object *jo = NULL;
    char *jerr = NULL;
    if (JSONOBJECT_OK != CreateNodeFromJSON(json, jsonlen, &jo, &jerr)) {
        if (jerr) {
            RedisModule_ReplyWithError(ctx, jerr);
            free(jerr);
        } else {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
        }
        goto error;
    }

    // the value must be a string
    if (N_STRING != NODETYPE(jo)) {
        sds err = sdscatfmt(sdsempty(), "ERR wrong type of value - expected %s but found %s",
                            NodeTypeStr(N_STRING), NodeTypeStr(NODETYPE(jpn.n)));
        RedisModule_ReplyWithError(ctx, err);
        sdsfree(err);
    }

    // actually concatenate the strings
    Node_StringAppend(jpn.n, jo);
    RedisModule_ReplyWithLongLong(ctx, (long long)Node_Length(jpn.n));

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.ARRINSERT <key> <path> <index> <json> [<json> ...]
 * Insert the `json` value(s) into the array at `path` before the `index` (shifts to the right).
 *
 * The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative
 * index values are interpreted as starting from the end.
 *
 * Reply: Integer, specifically the array's new size
*/
int JSONArrInsert_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc < 5) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key can't be empty and must be a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type || RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // the target must be an array
    if (N_ARRAY != NODETYPE(jpn.n)) {
        ReplyWithPathTypeError(ctx, N_ARRAY, NODETYPE(jpn.n));
        goto error;
    }

    // get the index
    long long index;
    if (REDISMODULE_OK != RedisModule_StringToLongLong(argv[3], &index)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_INVALID);
        goto error;
    }

    // convert negative values
    if (index < 0) index = Node_Length(jpn.n) + index;

    // check for out of range
    if (index < 0 || index > Node_Length(jpn.n)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_OUTOFRANGE);
        goto error;
    }

    // make an array from the JSON values
    Node *sub = NewArrayNode(argc - 4);
    for (int i = 4; i < argc; i++) {
        // JSON must be valid
        size_t jsonlen;
        const char *json = RedisModule_StringPtrLen(argv[i], &jsonlen);
        if (!jsonlen) {
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_EMPTY_STRING);
            Node_Free(sub);
            goto error;
        }

        // create object from json
        Object *jo = NULL;
        char *jerr = NULL;
        if (JSONOBJECT_OK != CreateNodeFromJSON(json, jsonlen, &jo, &jerr)) {
            Node_Free(sub);
            if (jerr) {
                RedisModule_ReplyWithError(ctx, jerr);
                free(jerr);
            } else {
                RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
                RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
            }
            goto error;
        }

        // append it to the sub array
        if (OBJ_OK != Node_ArrayAppend(sub, jo)) {
            Node_Free(jo);
            Node_Free(sub);
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_INSERT_SUBARRY);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_INSERT_SUBARRY);
            goto error;
        }
    }

    // insert the sub array to the target array
    if (OBJ_OK != Node_ArrayInsert(jpn.n, index, sub)) {
        Node_Free(sub);
        RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_INSERT);
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INSERT);
        goto error;
    }

    RedisModule_ReplyWithLongLong(ctx, Node_Length(jpn.n));

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/* JSON.ARRAPPEND <key> <path> <json> [<json> ...]
 * Append the `json` value(s) into the array at `path` after the last element in it.
 * Reply: Integer, specifically the array's new size
*/
int JSONArrAppend_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc < 4) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key can't be empty and must be a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type || RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // the target must be an array
    if (N_ARRAY != NODETYPE(jpn.n)) {
        ReplyWithPathTypeError(ctx, N_ARRAY, NODETYPE(jpn.n));
        goto error;
    }

    // make an array from the JSON values
    Node *sub = NewArrayNode(argc - 3);
    for (int i = 3; i < argc; i++) {
        // JSON must be valid
        size_t jsonlen;
        const char *json = RedisModule_StringPtrLen(argv[i], &jsonlen);
        if (!jsonlen) {
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_EMPTY_STRING);
            Node_Free(sub);
            goto error;
        }

        // create object from json
        Object *jo = NULL;
        char *jerr = NULL;
        if (JSONOBJECT_OK != CreateNodeFromJSON(json, jsonlen, &jo, &jerr)) {
            Node_Free(sub);
            if (jerr) {
                RedisModule_ReplyWithError(ctx, jerr);
                free(jerr);
            } else {
                RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
                RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
            }
            goto error;
        }

        // append it to the sub array
        if (OBJ_OK != Node_ArrayAppend(sub, jo)) {
            Node_Free(jo);
            Node_Free(sub);
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_INSERT_SUBARRY);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_INSERT_SUBARRY);
            goto error;
        }
    }

    // insert the sub array to the target array
    if (OBJ_OK != Node_ArrayInsert(jpn.n, Node_Length(jpn.n), sub)) {
        Node_Free(sub);
        RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_INSERT);
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INSERT);
        goto error;
    }

    RedisModule_ReplyWithLongLong(ctx, Node_Length(jpn.n));

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
 * JSON.ARRINDEX <key> <path> <scalar> [start] [stop]
 * Search for the first occurance of a scalar JSON value in an array.
 *
 * The optional inclusive `start` (default 0) and exclusive `stop` (default 0, meaning that the last
 * element is included) specify a slice of the array to search.
 *
 * Note: out of range errors are treated by rounding the index to the array's start and end. An
 * inverse index range (e.g, from 1 to 0) will return unfound.
 *
 * Reply: Integer, specifically the position of the scalar value in the array or -1 if unfound.
*/
int JSONArrIndex_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if ((argc < 4) || (argc > 6)) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key can't be empty and must be a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type || RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // verify that the target's type is an array
    if (N_ARRAY != NODETYPE(jpn.n)) {
        ReplyWithPathTypeError(ctx, N_ARRAY, NODETYPE(jpn.n));
        goto error;
    }

    // the JSON value to search for must be valid
    size_t jsonlen;
    const char *json = RedisModule_StringPtrLen(argv[3], &jsonlen);
    if (!jsonlen) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_EMPTY_STRING);
        goto error;
    }

    // create an object from json
    Object *jo = NULL;
    char *jerr = NULL;
    if (JSONOBJECT_OK != CreateNodeFromJSON(json, jsonlen, &jo, &jerr)) {
        if (jerr) {
            RedisModule_ReplyWithError(ctx, jerr);
            free(jerr);
        } else {
            RM_LOG_WARNING(ctx, "%s", REJSON_ERROR_JSONOBJECT_ERROR);
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_JSONOBJECT_ERROR);
        }
        goto error;
    }

    // get start (inclusive) & stop (exlusive) indices
    long long start = 0, stop = 0;
    if (argc > 4) {
        if (REDISMODULE_OK != RedisModule_StringToLongLong(argv[4], &start)) {
            RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_INVALID);
            goto error;
        }
        if (argc > 5) {
            if (REDISMODULE_OK != RedisModule_StringToLongLong(argv[5], &stop)) {
                RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_INVALID);
                goto error;
            }
        }
    }

    RedisModule_ReplyWithLongLong(ctx, Node_ArrayIndex(jpn.n, jo, (int)start, (int)stop));

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

/**
* JSON.ARRTRIM <key> <path> <start> <stop>
* Trim an array so that it contains only the specified inclusive range of elements.
*
* This command is extremely forgiving and using it with out of range indexes will not produce an
* error. If `start` is larger than the array's size or `start` > `stop`, the result will be an empty
* array. If `start` is < 0 then it will be treated as 0. If end is larger than the end of the array,
* it will be treated like the last element in it.
*
* Reply: Integer, specifically the array's new size.
*/
int JSONArrTrim_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    // check args
    if (argc != 5) {
        RedisModule_WrongArity(ctx);
        return REDISMODULE_ERR;
    }
    RedisModule_AutoMemory(ctx);

    // key can't be empty and must be a JSON type
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int type = RedisModule_KeyType(key);
    if (REDISMODULE_KEYTYPE_EMPTY == type || RedisModule_ModuleTypeGetType(key) != JSONType) {
        RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
        return REDISMODULE_ERR;
    }

    // validate path
    JSONPathNode_t jpn;
    Object *objRoot = RedisModule_ModuleTypeGetValue(key);
    if (PARSE_OK != NodeFromJSONPath(objRoot, argv[2], &jpn)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_PARSE_PATH);
        return REDISMODULE_ERR;
    }

    // deal with path errors
    if (E_OK != jpn.err) {
        ReplyWithPathError(ctx, &jpn);
        goto error;
    }

    // verify that the target's type is an array
    if (N_ARRAY != NODETYPE(jpn.n)) {
        ReplyWithPathTypeError(ctx, N_ARRAY, NODETYPE(jpn.n));
        goto error;
    }

    // get start & stop
    long long start, stop, left, right;
    long long len = (long long)Node_Length(jpn.n);
    if (REDISMODULE_OK != RedisModule_StringToLongLong(argv[3], &start)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_INVALID);
        goto error;
    }
    if (REDISMODULE_OK != RedisModule_StringToLongLong(argv[4], &stop)) {
        RedisModule_ReplyWithError(ctx, REJSON_ERROR_INDEX_INVALID);
        goto error;
    }

    // convert negative indexes
    if (start < 0) start = len + start;
    if (stop < 0) stop = len + stop;

    if (start < 0) start = 0;            // start at the beginning
    if (start > stop || start >= len) {  // empty the array
        left = len;
        right = 0;
    } else {  // set the boundries
        left = start;
        if (stop >= len) stop = len - 1;
        right = len - stop - 1;
    }

    // trim the array
    Node_ArrayDelRange(jpn.n, 0, left);
    Node_ArrayDelRange(jpn.n, -right, right);

    RedisModule_ReplyWithLongLong(ctx, (long long)Node_Length(jpn.n));

    JSONPathNode_Free(&jpn);
    return REDISMODULE_OK;

error:
    JSONPathNode_Free(&jpn);
    return REDISMODULE_ERR;
}

int RedisModule_OnLoad(RedisModuleCtx *ctx) __attribute__((visibility("default")));
int RedisModule_OnLoad(RedisModuleCtx *ctx) {
    // Register the module
    if (RedisModule_Init(ctx, RLMODULE_NAME, 1, REDISMODULE_APIVER_1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    // Register the JSON data type
    RedisModuleTypeMethods tm = {.version = REDISMODULE_TYPE_METHOD_VERSION,
                                 .rdb_load = JSONTypeRdbLoad,
                                 .rdb_save = ObjectTypeRdbSave,
                                 .aof_rewrite = JSONTypeAofRewrite,
                                 .free = ObjectTypeFree};
    JSONType = RedisModule_CreateDataType(ctx, JSONTYPE_NAME, JSONTYPE_ENCODING_VERSION, &tm);

    /* Module commands. */
    /* Generic JSON type commands. */
    if (RedisModule_CreateCommand(ctx, "json.resp", JSONResp_RedisCommand, "readonly", 1, 1, 1) ==
        REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.memory", JSONMemory_RedisCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.type", JSONType_RedisCommand, "readonly", 1, 1, 1) ==
        REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.set", JSONSet_RedisCommand, "write deny-oom", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.get", JSONGet_RedisCommand, "readonly", 1, 1, 1) ==
        REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.mget", JSONMGet_RedisCommand, "readonly getkeys-api",
                                  1, 1, 1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.del", JSONDel_RedisCommand, "write", 1, 1, 1) ==
        REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.forget", JSONDel_RedisCommand, "write", 1, 1, 1) ==
        REDISMODULE_ERR)
        return REDISMODULE_ERR;

    /* JSON number commands. */
    if (RedisModule_CreateCommand(ctx, "json.numincrby", JSONNum_GenericCommand, "write", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.nummultby", JSONNum_GenericCommand, "write", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    /* JSON string commands. */
    if (RedisModule_CreateCommand(ctx, "json.strlen", JSONLen_GenericCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.strappend", JSONStrAppend_RedisCommand,
                                  "write deny-oom", 1, 1, 1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    /* JSON array commands matey. */
    if (RedisModule_CreateCommand(ctx, "json.arrlen", JSONLen_GenericCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.arrinsert", JSONArrInsert_RedisCommand,
                                  "write deny-oom", 1, 1, 1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.arrappend", JSONArrAppend_RedisCommand,
                                  "write deny-oom", 1, 1, 1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.arrindex", JSONArrIndex_RedisCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.arrtrim", JSONArrTrim_RedisCommand, "write", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    /* JSON object commands. */
    if (RedisModule_CreateCommand(ctx, "json.objlen", JSONLen_GenericCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (RedisModule_CreateCommand(ctx, "json.objkeys", JSONObjKeys_RedisCommand, "readonly", 1, 1,
                                  1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    RM_LOG_WARNING(ctx, "%s - v%d.%d.%d [encver %d] is standing by.", RLMODULE_DESC,
                   PROJECT_VERSION_MAJOR, PROJECT_VERSION_MINOR, PROJECT_VERSION_PATCH,
                   JSONTYPE_ENCODING_VERSION);

    return REDISMODULE_OK;
}
