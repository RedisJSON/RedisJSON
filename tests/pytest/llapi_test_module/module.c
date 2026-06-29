/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

/*
 * Test consumer module for the RedisJSON shared C API (LLAPI).
 *
 * This module fetches the RedisJSON shared API via RedisModule_GetSharedAPI and
 * exposes each RedisJSONAPI function as a thin `LLAPI.*` command, so the Python
 * flow tests can exercise the C API end-to-end against a live Redis.
 *
 * It binds the API *by name* ("RedisJSON_V<n>") and never depends on RedisJSON
 * internals, so the same tests can run against any module that exports the same
 * shared API (e.g. JsonHDT in the OSS test suite, MOD-14113).
 *
 * It must be loaded *after* the JSON module under test, e.g.:
 *   redis-server --loadmodule rejson.so --loadmodule llapi_test.so
 */

#define REDISMODULE_MAIN
#include "redismodule.h"
#include "rejson_api.h"

#include <string.h>
#include <stdlib.h>

static const RedisJSONAPI *japi = NULL;
static int japi_ver = 0;

/* Map a JSONType to a stable lowercase name used in replies. */
static const char *json_type_name(JSONType t) {
    switch (t) {
        case JSONType_String: return "string";
        case JSONType_Int:    return "int";
        case JSONType_Double: return "double";
        case JSONType_Bool:   return "bool";
        case JSONType_Object: return "object";
        case JSONType_Array:  return "array";
        case JSONType_Null:   return "null";
        default:              return "unknown";
    }
}

/* Map a JSONArrayType to a stable name used in replies. */
static const char *json_array_type_name(JSONArrayType t) {
    switch (t) {
        case JSONArrayType_Heterogeneous: return "heterogeneous";
        case JSONArrayType_I8:   return "i8";
        case JSONArrayType_U8:   return "u8";
        case JSONArrayType_I16:  return "i16";
        case JSONArrayType_U16:  return "u16";
        case JSONArrayType_F16:  return "f16";
        case JSONArrayType_BF16: return "bf16";
        case JSONArrayType_I32:  return "i32";
        case JSONArrayType_U32:  return "u32";
        case JSONArrayType_F32:  return "f32";
        case JSONArrayType_I64:  return "i64";
        case JSONArrayType_U64:  return "u64";
        case JSONArrayType_F64:  return "f64";
        default:                 return "unknown";
    }
}

/* Open the key named by argv[1] and run get(path=argv[2]).
 * On failure, replies with an error and returns NULL (caller should return). */
static JSONResultsIterator open_and_get(RedisModuleCtx *ctx, RedisModuleString **argv) {
    RedisJSON json = japi->openKey(ctx, argv[1]);
    if (!json) {
        RedisModule_ReplyWithError(ctx, "ERR key does not exist or is not JSON");
        return NULL;
    }
    const char *path = RedisModule_StringPtrLen(argv[2], NULL);
    JSONResultsIterator it = japi->get(json, path);
    if (!it) {
        RedisModule_ReplyWithError(ctx, "ERR path could not be evaluated");
        return NULL;
    }
    return it;
}

/* LLAPI.VERSION -> the bound shared-API version (integer). */
static int VersionCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    REDISMODULE_NOT_USED(argv);
    if (argc != 1) return RedisModule_WrongArity(ctx);
    RedisModule_ReplyWithLongLong(ctx, japi_ver);
    return REDISMODULE_OK;
}

/* LLAPI.OPEN_GET key path -> array of the JSON string of every matched node. */
static int OpenGetCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_LEN);
    long n = 0;
    RedisJSON node;
    while ((node = japi->next(it)) != NULL) {
        RedisModuleString *s = NULL;
        if (japi->getJSON(node, ctx, &s) == REDISMODULE_OK) {
            RedisModule_ReplyWithString(ctx, s);
            RedisModule_FreeString(ctx, s);
        } else {
            RedisModule_ReplyWithError(ctx, "ERR getJSON failed");
        }
        n++;
    }
    RedisModule_ReplySetArrayLength(ctx, n);
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.ITER_JSON key path -> getJSONFromIter (whole result set as one JSON string). */
static int IterJsonCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisModuleString *s = NULL;
    if (japi->getJSONFromIter(it, ctx, &s) == REDISMODULE_OK) {
        RedisModule_ReplyWithString(ctx, s);
        RedisModule_FreeString(ctx, s);
    } else {
        RedisModule_ReplyWithError(ctx, "ERR getJSONFromIter failed");
    }
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.ITER_LEN key path -> number of matched nodes (iterator len). */
static int IterLenCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;
    RedisModule_ReplyWithLongLong(ctx, (long long)japi->len(it));
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.RESET key path -> [count_first_pass, count_after_reset]. Proves resetIter. */
static int ResetCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    long long first = 0, second = 0;
    while (japi->next(it) != NULL) first++;
    japi->resetIter(it);
    while (japi->next(it) != NULL) second++;

    RedisModule_ReplyWithArray(ctx, 2);
    RedisModule_ReplyWithLongLong(ctx, first);
    RedisModule_ReplyWithLongLong(ctx, second);
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.OPENFROMSTR keyname path -> number of matched nodes (uses openKeyFromStr). */
static int OpenFromStrCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    const char *keyname = RedisModule_StringPtrLen(argv[1], NULL);
    RedisJSON json = japi->openKeyFromStr(ctx, keyname);
    if (!json) {
        RedisModule_ReplyWithError(ctx, "ERR key does not exist or is not JSON");
        return REDISMODULE_OK;
    }
    const char *path = RedisModule_StringPtrLen(argv[2], NULL);
    JSONResultsIterator it = japi->get(json, path);
    if (!it) {
        RedisModule_ReplyWithError(ctx, "ERR path could not be evaluated");
        return REDISMODULE_OK;
    }
    RedisModule_ReplyWithLongLong(ctx, (long long)japi->len(it));
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.OPENFLAGS key path -> number of matched nodes (uses openKeyWithFlags, read). */
static int OpenFlagsCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    RedisJSON json = japi->openKeyWithFlags(ctx, argv[1], REDISMODULE_READ);
    if (!json) {
        RedisModule_ReplyWithError(ctx, "ERR key does not exist or is not JSON");
        return REDISMODULE_OK;
    }
    const char *path = RedisModule_StringPtrLen(argv[2], NULL);
    JSONResultsIterator it = japi->get(json, path);
    if (!it) {
        RedisModule_ReplyWithError(ctx, "ERR path could not be evaluated");
        return REDISMODULE_OK;
    }
    RedisModule_ReplyWithLongLong(ctx, (long long)japi->len(it));
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.TYPE key path -> JSONType name of the first matched node. */
static int TypeCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    if (!node) {
        RedisModule_ReplyWithError(ctx, "ERR no value at path");
    } else {
        RedisModule_ReplyWithSimpleString(ctx, json_type_name(japi->getType(node)));
    }
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.SCALAR key path -> the scalar value of the first node via the typed getters. */
static int ScalarCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    if (!node) {
        RedisModule_ReplyWithError(ctx, "ERR no value at path");
        japi->freeIter(it);
        return REDISMODULE_OK;
    }

    switch (japi->getType(node)) {
        case JSONType_Int: {
            long long v = 0;
            if (japi->getInt(node, &v) == REDISMODULE_OK)
                RedisModule_ReplyWithLongLong(ctx, v);
            else
                RedisModule_ReplyWithError(ctx, "ERR getInt failed");
            break;
        }
        case JSONType_Double: {
            double d = 0;
            if (japi->getDouble(node, &d) == REDISMODULE_OK)
                RedisModule_ReplyWithDouble(ctx, d);
            else
                RedisModule_ReplyWithError(ctx, "ERR getDouble failed");
            break;
        }
        case JSONType_Bool: {
            int b = 0;
            if (japi->getBoolean(node, &b) == REDISMODULE_OK)
                RedisModule_ReplyWithLongLong(ctx, b);
            else
                RedisModule_ReplyWithError(ctx, "ERR getBoolean failed");
            break;
        }
        case JSONType_String: {
            const char *str = NULL;
            size_t len = 0;
            if (japi->getString(node, &str, &len) == REDISMODULE_OK)
                RedisModule_ReplyWithStringBuffer(ctx, str, len);
            else
                RedisModule_ReplyWithError(ctx, "ERR getString failed");
            break;
        }
        default: {
            /* null / object / array: fall back to the JSON representation. */
            RedisModuleString *s = NULL;
            if (japi->getJSON(node, ctx, &s) == REDISMODULE_OK) {
                RedisModule_ReplyWithString(ctx, s);
                RedisModule_FreeString(ctx, s);
            } else {
                RedisModule_ReplyWithError(ctx, "ERR getJSON failed");
            }
            break;
        }
    }
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.GETLEN key path -> getLen of the first node (string/array/object length). */
static int GetLenCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    size_t len = 0;
    if (node && japi->getLen(node, &len) == REDISMODULE_OK)
        RedisModule_ReplyWithLongLong(ctx, (long long)len);
    else
        RedisModule_ReplyWithError(ctx, "ERR getLen failed (not a string/array/object)");
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.GETJSON key path -> getJSON of the first matched node. */
static int GetJsonCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    RedisModuleString *s = NULL;
    if (node && japi->getJSON(node, ctx, &s) == REDISMODULE_OK) {
        RedisModule_ReplyWithString(ctx, s);
        RedisModule_FreeString(ctx, s);
    } else {
        RedisModule_ReplyWithError(ctx, "ERR getJSON failed");
    }
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.GETAT key path index -> getJSON of the array element at `index`.
 * Exercises allocJson / getAt / freeJson. */
static int GetAtCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 4) return RedisModule_WrongArity(ctx);
    long long index = 0;
    if (RedisModule_StringToLongLong(argv[3], &index) != REDISMODULE_OK || index < 0) {
        RedisModule_ReplyWithError(ctx, "ERR invalid index");
        return REDISMODULE_OK;
    }
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    if (!node) {
        RedisModule_ReplyWithError(ctx, "ERR no value at path");
        japi->freeIter(it);
        return REDISMODULE_OK;
    }

    RedisJSONPtr buf = japi->allocJson();
    if (japi->getAt(node, (size_t)index, buf) == REDISMODULE_OK) {
        RedisJSON elem = *buf;
        RedisModuleString *s = NULL;
        if (japi->getJSON(elem, ctx, &s) == REDISMODULE_OK) {
            RedisModule_ReplyWithString(ctx, s);
            RedisModule_FreeString(ctx, s);
        } else {
            RedisModule_ReplyWithError(ctx, "ERR getJSON failed");
        }
    } else {
        RedisModule_ReplyWithError(ctx, "ERR getAt failed (not an array or index out of range)");
    }
    japi->freeJson(buf);
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.GETARRAY key path -> [array_type_name, length]. Exercises getArray. */
static int GetArrayCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    if (!node) {
        RedisModule_ReplyWithError(ctx, "ERR no value at path");
        japi->freeIter(it);
        return REDISMODULE_OK;
    }

    size_t len = 0;
    JSONArrayType atype = JSONArrayType_Heterogeneous;
    const void *arr = japi->getArray(node, &len, &atype);
    if (!arr && len == 0 && japi->getType(node) != JSONType_Array) {
        RedisModule_ReplyWithError(ctx, "ERR not an array");
    } else {
        RedisModule_ReplyWithArray(ctx, 2);
        RedisModule_ReplyWithSimpleString(ctx, json_array_type_name(atype));
        RedisModule_ReplyWithLongLong(ctx, (long long)len);
    }
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.KEYVALUES key path -> flat array [key, valueJSON, key, valueJSON, ...]
 * for the first node, which must be an object. */
static int KeyValuesCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 3) return RedisModule_WrongArity(ctx);
    JSONResultsIterator it = open_and_get(ctx, argv);
    if (!it) return REDISMODULE_OK;

    RedisJSON node = japi->next(it);
    if (!node || japi->getType(node) != JSONType_Object) {
        RedisModule_ReplyWithError(ctx, "ERR not an object");
        japi->freeIter(it);
        return REDISMODULE_OK;
    }

    JSONKeyValuesIterator kv = japi->getKeyValues(node);
    if (!kv) {
        RedisModule_ReplyWithError(ctx, "ERR getKeyValues failed");
        japi->freeIter(it);
        return REDISMODULE_OK;
    }

    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_LEN);
    long n = 0;
    RedisModuleString *key = NULL;
    RedisJSONPtr buf = japi->allocJson();
    while (japi->nextKeyValue(kv, &key, buf) == REDISMODULE_OK) {
        RedisModule_ReplyWithString(ctx, key);
        RedisModule_FreeString(ctx, key);
        key = NULL;
        RedisJSON val = *buf;
        RedisModuleString *s = NULL;
        if (japi->getJSON(val, ctx, &s) == REDISMODULE_OK) {
            RedisModule_ReplyWithString(ctx, s);
            RedisModule_FreeString(ctx, s);
        } else {
            RedisModule_ReplyWithError(ctx, "ERR getJSON failed");
        }
        n += 2;
    }
    RedisModule_ReplySetArrayLength(ctx, n);
    japi->freeJson(buf);
    japi->freeKeyValuesIter(kv);
    japi->freeIter(it);
    return REDISMODULE_OK;
}

/* LLAPI.ISJSON key -> 1 if the key holds JSON, else 0. */
static int IsJsonCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 2) return RedisModule_WrongArity(ctx);
    RedisModuleKey *key = RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ);
    int res = japi->isJSON(key);
    if (key) RedisModule_CloseKey(key);
    RedisModule_ReplyWithLongLong(ctx, res);
    return REDISMODULE_OK;
}

/* LLAPI.PATHPARSE path -> [isSingle, hasDefinedOrder], or an error if the path
 * fails to parse / is not supported (e.g. a projection). */
static int PathParseCmd(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    if (argc != 2) return RedisModule_WrongArity(ctx);
    const char *path = RedisModule_StringPtrLen(argv[1], NULL);
    RedisModuleString *err = NULL;
    JSONPath p = japi->pathParse(path, ctx, &err);
    if (!p) {
        if (err) {
            RedisModule_ReplyWithError(ctx, RedisModule_StringPtrLen(err, NULL));
            RedisModule_FreeString(ctx, err);
        } else {
            RedisModule_ReplyWithError(ctx, "ERR pathParse failed");
        }
        return REDISMODULE_OK;
    }
    RedisModule_ReplyWithArray(ctx, 2);
    RedisModule_ReplyWithLongLong(ctx, japi->pathIsSingle(p));
    RedisModule_ReplyWithLongLong(ctx, japi->pathHasDefinedOrder(p));
    japi->pathFree(p);
    return REDISMODULE_OK;
}

/* Resolve the highest available shared-API version, "RedisJSON_V<n>" .. V1. */
static int fetch_japi(RedisModuleCtx *ctx) {
    char name[32];
    for (int v = RedisJSONAPI_LATEST_API_VER; v >= 1; v--) {
        snprintf(name, sizeof(name), "RedisJSON_V%d", v);
        const RedisJSONAPI *api = RedisModule_GetSharedAPI(ctx, name);
        if (api) {
            japi = api;
            japi_ver = v;
            return REDISMODULE_OK;
        }
    }
    return REDISMODULE_ERR;
}

#define REGISTER(name, fn) \
    if (RedisModule_CreateCommand(ctx, name, fn, "readonly", 0, 0, 0) != REDISMODULE_OK) \
        return REDISMODULE_ERR;

int RedisModule_OnLoad(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
    REDISMODULE_NOT_USED(argv);
    REDISMODULE_NOT_USED(argc);

    if (RedisModule_Init(ctx, "llapi_test", 1, REDISMODULE_APIVER_1) == REDISMODULE_ERR)
        return REDISMODULE_ERR;

    if (fetch_japi(ctx) != REDISMODULE_OK) {
        RedisModule_Log(ctx, "warning",
            "llapi_test: RedisJSON shared API not found; load a JSON module first");
        return REDISMODULE_ERR;
    }
    RedisModule_Log(ctx, "notice", "llapi_test: bound RedisJSON shared API V%d", japi_ver);

    REGISTER("LLAPI.VERSION", VersionCmd);
    REGISTER("LLAPI.OPEN_GET", OpenGetCmd);
    REGISTER("LLAPI.ITER_JSON", IterJsonCmd);
    REGISTER("LLAPI.ITER_LEN", IterLenCmd);
    REGISTER("LLAPI.RESET", ResetCmd);
    REGISTER("LLAPI.OPENFROMSTR", OpenFromStrCmd);
    REGISTER("LLAPI.OPENFLAGS", OpenFlagsCmd);
    REGISTER("LLAPI.TYPE", TypeCmd);
    REGISTER("LLAPI.SCALAR", ScalarCmd);
    REGISTER("LLAPI.GETLEN", GetLenCmd);
    REGISTER("LLAPI.GETJSON", GetJsonCmd);
    REGISTER("LLAPI.GETAT", GetAtCmd);
    REGISTER("LLAPI.GETARRAY", GetArrayCmd);
    REGISTER("LLAPI.KEYVALUES", KeyValuesCmd);
    REGISTER("LLAPI.ISJSON", IsJsonCmd);
    REGISTER("LLAPI.PATHPARSE", PathParseCmd);

    return REDISMODULE_OK;
}
