#include <jsonsl.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include "all-tests.h"

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

static void check_path(const char *path)
{
    jsonsl_error_t err;
    size_t ii;
    jsonsl_jpr_t jpr;

    fprintf(stderr, "=== Testing %s ===\n", path);

    jpr = jsonsl_jpr_new(path, &err);
    if (jpr == NULL) {
        fprintf(stderr, "Couldn't create new JPR with path '%s': %s\n",
                path, jsonsl_strerror(err));
        abort();
    }
    printf("%lu components\n", jpr->ncomponents);

    for (ii = 0; ii < jpr->ncomponents; ii++) {
        struct jsonsl_jpr_component_st *comp = jpr->components + ii;
        printf("[%lu]: ", ii);
        if (comp->ptype == JSONSL_PATH_ROOT) {
            printf("Root: /\n");
        } else if (comp->ptype == JSONSL_PATH_NUMERIC) {
            printf("\tNumeric: %lu\n", comp->idx);
        } else if (comp->ptype == JSONSL_PATH_WILDCARD) {
            printf("\tWildcard: %c\n", JSONSL_PATH_WILDCARD_CHAR);
        } else {
            printf("\tString: %s\n", comp->pstr);
        }
    }
    printf("Destroying..\n\n");
    jsonsl_jpr_destroy(jpr);
}

static void check_bad_path(const char *bad_path)
{
    jsonsl_error_t err;
    jsonsl_jpr_t jpr;
    fprintf(stderr, "=== Checking bad path %s ===\n", bad_path);
    jpr = jsonsl_jpr_new(bad_path, &err);
    if (jpr != NULL) {
        fprintf(stderr, "Expected %s to fail validation\n", bad_path);
        abort();
    }
}

static void check_match(const char *path,
                        jsonsl_type_t type,
                        unsigned int level,
                        void *spec,
                        jsonsl_jpr_match_t expected)
{
    char *key;
    size_t nkey;
    jsonsl_jpr_t jpr;
    jsonsl_jpr_match_t matchres;
    fprintf(stderr, "=== Match jpr=%-15s parent(type=%s,level=%d)",
           path, jsonsl_strtype(type), level);

    if (type == JSONSL_T_LIST) {
        key = NULL;
        nkey = (size_t)spec;
        fprintf(stderr, " idx=%lu", nkey);
    } else {
        key = (char*)spec;
        nkey = strlen(spec);
        fprintf(stderr, " key=%-10s", key);
    }
    fprintf(stderr, " Exp: %s ===\n", jsonsl_strmatchtype(expected));

    jpr = jsonsl_jpr_new(path, NULL);
    assert(jpr);

    matchres = jsonsl_jpr_match(jpr, type, level, key, nkey);
    if (matchres != expected) {
        fprintf(stderr, "Expected %s, got %s\n", jsonsl_strmatchtype(expected),
                jsonsl_strmatchtype(matchres));
        abort();
    }
}

struct lexer_global_st {
    const char *hkey;
    size_t nhkey;
};

static void push_callback(jsonsl_t jsn,
                          jsonsl_action_t action,
                          struct jsonsl_state_st *state,
                          const jsonsl_char_t *at)
{
    struct lexer_global_st *global = (struct lexer_global_st*)jsn->data;
    jsonsl_jpr_match_t matchres;
    jsonsl_jpr_t matchjpr;
    if (state->type == JSONSL_T_HKEY) {
        return;
    }
    matchjpr = jsonsl_jpr_match_state(jsn, state,
                                      global->hkey,
                                      global->nhkey,
                                      &matchres);
    printf("Got match result: %d\n", matchres);
}

static void pop_callback(jsonsl_t jsn,
                         jsonsl_action_t action,
                         struct jsonsl_state_st *state,
                         const jsonsl_char_t *at)
{
    struct lexer_global_st *global = (struct lexer_global_st*)jsn->data;
    if (state->type == JSONSL_T_HKEY) {
        global->hkey = at - (state->pos_cur - state->pos_begin);
        global->hkey++;
        global->nhkey = (state->pos_cur - state->pos_begin)-1;
        printf("Got key..");
        fwrite(global->hkey, 1,  global->nhkey, stdout);
        printf("\n");
    }
}

static int error_callback(jsonsl_t jsn,
                          jsonsl_error_t error,
                          struct jsonsl_state_st *state,
                          jsonsl_char_t *at)
{
    fprintf(stderr, "Got error %s at pos %lu. Remaining: %s\n",
            jsonsl_strerror(error), jsn->pos, at);
    abort();
    return 0;
}


static void lexjpr(void)
{
    struct lexer_global_st global;
    jsonsl_t jsn;
    jsonsl_jpr_t jpr;
    jpr = jsonsl_jpr_new("/foo/^/1", NULL);
    assert(jpr);
    jsn = jsonsl_new(24);
    assert(jsn);
    jsonsl_jpr_match_state_init(jsn, &jpr, 1);
    jsn->error_callback = error_callback;
    jsn->action_callback_POP = pop_callback;
    jsn->action_callback_PUSH = push_callback;
    jsonsl_enable_all_callbacks(jsn);
    jsn->data = &global;
    jsonsl_feed(jsn, SampleJSON, sizeof(SampleJSON)-1);
}

JSONSL_TEST_JPR_FUNC
{
    printf("%s\n", SampleJSON);
    if (getenv("JSONSL_QUIET_TESTS")) {
        freopen("/dev/null", "w", stdout);
    }
    /* This should match only the root object */
    check_path("/");

    /* This should match { "foo" : <whatever> } */
    check_path("/foo");

    /* This should match { "foo" : { "another prop": <whatever> } } */
    check_path("/foo/another%20prop");

    /* this should match { "foo" : { "another prop": { "baz" : <whatever> } } } */
    check_path("/foo/another%20prop/baz");

    /* This should match { "foo" : { "anArray" : [ <whatever>, <not matched> ] } } */
    check_path("/foo/anArray/0");

    /* This should match any of the following:
     * {
     *  "hello" : {
     *      "cruel" : {
     *          "world" : {
     *              ....
     *          }
     *      },
     *      "kind" : {
     *          "world" : {
     *              ....
     *         }
     *     }
     *  }
     * }
     */
    check_path("/hello/^/world");

    check_bad_path("rootless/uri");
    check_bad_path("/doubly-escaped//uri");
    check_bad_path("/%GG");
    check_bad_path("/incomplete%f/hex");

    check_match("/", JSONSL_T_OBJECT, 0, "some_key", JSONSL_MATCH_COMPLETE);
    check_match("/", JSONSL_T_OBJECT, 1, "some_key", JSONSL_MATCH_NOMATCH);
    check_match("/^", JSONSL_T_OBJECT, 1, "some_key", JSONSL_MATCH_COMPLETE);
    check_match("/foo/bar/baz", JSONSL_T_OBJECT, 2, "bar", JSONSL_MATCH_POSSIBLE);
    check_match("/foo/bar/^/grrrrrr", JSONSL_T_OBJECT, 3, "anything", JSONSL_MATCH_POSSIBLE);
    check_match("/foo/bar/something/grrr", JSONSL_T_OBJECT, 3, "anything", JSONSL_MATCH_NOMATCH);

    lexjpr();
    return 0;
}
