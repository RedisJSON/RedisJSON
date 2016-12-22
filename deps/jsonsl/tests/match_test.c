#include <jsonsl.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <stdarg.h>

#define _JSTR(e) \
    "\"" #e "\""

const char SampleJSON[] =
        "{"
            _JSTR(foo) ": {"
                _JSTR(bar) ": ["
                    _JSTR(element0) ","
                    _JSTR(element1)
                    "],"
               _JSTR(inner object) ": {" \
                   _JSTR(baz) ":" _JSTR(qux)
               "}"
           "}"
        "}";

typedef struct {
    jsonsl_jpr_t jpr;
    jsonsl_jpr_match_t match_status;
    const char *buf;
    const char *key;
    size_t nkey;

    unsigned match_type;
    size_t match_begin;
    size_t match_end;
    unsigned match_level;
} match_context;

static void
push_callback(jsonsl_t jsn, jsonsl_action_t action,
              struct jsonsl_state_st *state, const jsonsl_char_t *at)
{
    match_context *ctx = jsn->data;
    jsonsl_jpr_match_t matchres;
    assert(ctx->match_status != JSONSL_MATCH_COMPLETE);

    if (state->type == JSONSL_T_HKEY) {
        ctx->key = ctx->buf + state->pos_begin + 1;
        return;
    }

    matchres = jsonsl_path_match(
            ctx->jpr, jsonsl_last_state(jsn, state), state, ctx->key, ctx->nkey);

    if (matchres == JSONSL_MATCH_NOMATCH) {
        state->ignore_callback = 1;
        return;
    } else if (matchres == JSONSL_MATCH_TYPE_MISMATCH) {
        ctx->match_status = matchres;
        jsonsl_stop(jsn);
        return;
    } else if (matchres == JSONSL_MATCH_COMPLETE) {
        jsn->max_callback_level = state->level + 1;
    } else {
        /* POSSIBLE */
    }

    ctx->match_status = matchres;
    ctx->match_level = state->level;
}

static void
pop_callback(jsonsl_t jsn,
             jsonsl_action_t action, struct jsonsl_state_st *state,
             const jsonsl_char_t *at)
{
    match_context *ctx = jsn->data;

    if (state->type == JSONSL_T_HKEY) {
        ctx->key = ctx->buf + state->pos_begin + 1;
        ctx->nkey = state->pos_cur - state->pos_begin - 1;
        return;
    }

    if (ctx->match_status == JSONSL_MATCH_COMPLETE) {
        ctx->match_end = state->pos_cur;
        jsonsl_stop(jsn);
    }
}

static int
error_callback(jsonsl_t jsn, jsonsl_error_t error,
               struct jsonsl_state_st *state, jsonsl_char_t *at)
{
    fprintf(stderr, "Got error %s at pos %lu. Remaining: %s\n",
            jsonsl_strerror(error), jsn->pos, at);
    abort();
    return 0;
}

static void
do_match(jsonsl_jpr_match_t exp_status, unsigned exp_type, int comptype, ...)
{
    struct jsonsl_jpr_component_st comps[64];
    struct jsonsl_jpr_st jprst;
    va_list ap;
    size_t ncomps = 1;
    jsonsl_t jsn = jsonsl_new(512);
    match_context mctx = { 0 };

    memset(comps, 0, sizeof comps);
    memset(&jprst, 0, sizeof jprst);


    comps[0].ptype = JSONSL_PATH_ROOT;

    va_start(ap, comptype);

    while (comptype != JSONSL_PATH_INVALID) {
        if (comptype == JSONSL_PATH_STRING) {
            const char *s = va_arg(ap, const char *);
            comps[ncomps].pstr = (char *)s;
            comps[ncomps].len = strlen(s);
            comps[ncomps].ptype = JSONSL_PATH_STRING;
        } else {
            comps[ncomps].idx = va_arg(ap, int);
            comps[ncomps].ptype = JSONSL_PATH_NUMERIC;
            comps[ncomps].is_arridx = 1;
        }
        ncomps++;
        comptype = va_arg(ap, int);
    }

    va_end(ap);

    jprst.components = comps;
    jprst.ncomponents = ncomps;
    jprst.match_type = exp_type;

    jsonsl_enable_all_callbacks(jsn);
    jsn->action_callback_POP = pop_callback;
    jsn->action_callback_PUSH = push_callback;
    jsn->error_callback = error_callback;
    jsn->data = &mctx;

    mctx.buf = SampleJSON;
    mctx.jpr = &jprst;
    mctx.match_status = JSONSL_MATCH_NOMATCH;

    jsonsl_feed(jsn, SampleJSON, strlen(SampleJSON));
    assert(mctx.match_status == exp_status);
    jsonsl_destroy(jsn);
}

int main(int argc, char **argv)
{
    /* Match is OK */
    do_match(JSONSL_MATCH_COMPLETE, JSONSL_T_LIST,
             JSONSL_PATH_STRING, "foo",
             JSONSL_PATH_STRING, "bar",
             JSONSL_PATH_INVALID);

    /* Match is actually a list! */
    do_match(JSONSL_MATCH_TYPE_MISMATCH, JSONSL_T_STRING,
             JSONSL_PATH_STRING, "foo",
             JSONSL_PATH_STRING, "bar",
             JSONSL_PATH_INVALID);

    /* Bad intermediate path (array index for dict parent) */
    do_match(JSONSL_MATCH_TYPE_MISMATCH, JSONSL_T_STRING,
             JSONSL_PATH_STRING, "foo",
             JSONSL_PATH_NUMERIC, 29,
             JSONSL_PATH_INVALID);

    /* Bad intermediate path (string key for array parent) */
    do_match(JSONSL_MATCH_TYPE_MISMATCH, JSONSL_T_STRING,
             JSONSL_PATH_STRING, "foo",
             JSONSL_PATH_STRING, "bar",
             JSONSL_PATH_STRING, "baz",
             JSONSL_PATH_INVALID);

    /* Ok intermediate path matching (but index not found) */
    do_match(JSONSL_MATCH_POSSIBLE, JSONSL_T_STRING,
             JSONSL_PATH_STRING, "foo",
             JSONSL_PATH_STRING, "bar",
             JSONSL_PATH_NUMERIC, 99,
             JSONSL_PATH_INVALID);
    return 0;
}
