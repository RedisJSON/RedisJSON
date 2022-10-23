
#define REDISMODULE_MAIN

#include <string.h>
#include <math.h>   // fabs
#include <float.h>  // DBL_EPSILON

#include "rejson_api.h"

// REJSON APIs
static struct RJ_API {
  RedisJSONAPI *japi;
  int version;
} RJ_API = { NULL, 0 };

#define STRING(...)          #__VA_ARGS__
#define STRINGIFY(...) STRING(__VA_ARGS__)

#define ASSERT(expr)                                                        \
  if (!(expr)) {                                                            \
    RedisModule_ReplyWithError(ctx, "Assertion " STRING(expr) " Failed\n"); \
    return REDISMODULE_ERR;                                                 \
  }

#define MOD_PREFIX "RJ_LLAPI"
#define CMD_NAME(f)                            \
  ({                                           \
    char s[32] = MOD_PREFIX ".";               \
    strcat(s, STRING(f) + sizeof(MOD_PREFIX)); \
    s;                                         \
  })
#define RegisterCmd(f)                                                                  \
  if (RedisModule_CreateCommand(ctx, CMD_NAME(f), f, "", 0, 0, 0) == REDISMODULE_ERR) { \
    return REDISMODULE_ERR;                                                             \
  }

#define TESTFUNC(f)                                                          \
  do {                                                                       \
    RedisModuleCallReply *r = RedisModule_Call(ctx, CMD_NAME(f), "");        \
    RedisModule_ReplyWithCallReply(ctx, r);                                  \
    PASSED_TESTS += RedisModule_CallReplyType(r) != REDISMODULE_REPLY_ERROR; \
  } while (0);

#define TEST_PREFIX (MOD_PREFIX "_test")
#define TEST_NAME (__FUNCTION__ + sizeof(TEST_PREFIX))
#define TEST_SUCCESS                           \
  do {                                         \
    char s[32] = "Test ";                      \
    strcat(s, TEST_NAME);                      \
    strcat(s, ": PASSED");                     \
    RedisModule_ReplyWithSimpleString(ctx, s); \
    return REDISMODULE_OK;                     \
  } while (0);

void ModuleChangeHandler(
  struct RedisModuleCtx *ctx,
  RedisModuleEvent e,
  uint64_t sub,
  RedisModuleModuleChange *ei
);

int GetJSONAPIs(
  RedisModuleCtx *ctx,
  int subscribeToModuleChange
) {
  if ((RJ_API.japi = RedisModule_GetSharedAPI(ctx, "RedisJSON_V2")) != NULL) {
    RJ_API.version = 2;
    return REDISMODULE_OK;
  }
  
  if ((RJ_API.japi = RedisModule_GetSharedAPI(ctx, "RedisJSON_V1")) != NULL) {
    RJ_API.version = 1;
    return REDISMODULE_OK;
  }

  if (subscribeToModuleChange) {
    RedisModule_SubscribeToServerEvent(ctx, RedisModuleEvent_ModuleChange, (RedisModuleEventCallback) ModuleChangeHandler);
  }
  return REDISMODULE_ERR;
}

void ModuleChangeHandler(
  struct RedisModuleCtx *ctx,
  RedisModuleEvent e,
  uint64_t sub,
  RedisModuleModuleChange *ei
) {
  if (sub != REDISMODULE_SUBEVENT_MODULE_LOADED || // If the subscribed event is not a module load,
      RJ_API.japi != NULL ||                       // or JSON is already loaded,
      strcmp(ei->module_name, "ReJSON") != 0       // or the loading module is not JSON:
  ) { return; }                                    // ignore.

  // If RedisJSON module is loaded after this module, we need to get the API exported by RedisJSON.
  if (GetJSONAPIs(ctx, 0) != REDISMODULE_OK) {
    RedisModule_Log(ctx, "error", "Detected RedisJSON: failed to acquire ReJSON API");
  }
}


int RJ_llapi_test_open_key(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  if (argc != 1) {
    RedisModule_WrongArity(ctx);
    return REDISMODULE_ERR;
  }

  RedisModuleString *keyname = RedisModule_CreateString(ctx, TEST_NAME, strlen(TEST_NAME));
  RedisModuleCallReply *r = RedisModule_Call(ctx, "JSON.SET", "scc", keyname, "$", "0");
  ASSERT(RedisModule_CallReplyType(r) != REDISMODULE_REPLY_ERROR);

  RedisModuleKey *rmk = RedisModule_OpenKey(ctx, keyname, 0);
  ASSERT(RJ_API.japi->isJSON(rmk) != 0);
  RedisModule_CloseKey(rmk);
  ASSERT(RJ_API.japi->openKey(ctx, keyname) != NULL);

  RedisModule_Call(ctx, "SET", "sc", keyname, "0");
  rmk = RedisModule_OpenKey(ctx, keyname, 0);
  ASSERT(RJ_API.japi->isJSON(rmk) == 0);
  RedisModule_CloseKey(rmk);
  ASSERT(RJ_API.japi->openKey(ctx, keyname) == NULL);

  RedisModule_FreeString(ctx, keyname);
  TEST_SUCCESS;
}

int RJ_llapi_test_iterator(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  if (argc != 1) {
    RedisModule_WrongArity(ctx);
    return REDISMODULE_ERR;
  }

#define VALS 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
  long long vals[] = { VALS };
  const char json[] = "[" STRINGIFY(VALS) "]";
  RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", json);

  JSONResultsIterator ji = RJ_API.japi->get(RJ_API.japi->openKeyFromStr(ctx, TEST_NAME), "$..*");
  ASSERT(ji != NULL);
  if (RJ_API.version >= 2) {
    RedisModuleString *str;
    RJ_API.japi->getJSONFromIter(ji, ctx, &str);
    ASSERT(strcmp(RedisModule_StringPtrLen(str, NULL), json) == 0);
    RedisModule_FreeString(ctx, str);
  }

  size_t len = RJ_API.japi->len(ji); ASSERT(len == sizeof(vals)/sizeof(*vals));
  RedisJSON js; long long num;
  for (int i = 0; i < len; ++i) {
    js = RJ_API.japi->next(ji); ASSERT(js != NULL);
    RJ_API.japi->getInt(js, &num); ASSERT(num == vals[i]);
  }
  ASSERT(RJ_API.japi->next(ji) == NULL);
  if (RJ_API.version >= 2) {
    RJ_API.japi->resetIter(ji);
    for (int i = 0; i < len; ++i) {
      js = RJ_API.japi->next(ji); ASSERT(js != NULL);
      RJ_API.japi->getInt(js, &num); ASSERT(num == vals[i]);
    }
    ASSERT(RJ_API.japi->next(ji) == NULL);
  }

  RJ_API.japi->freeIter(ji);
  TEST_SUCCESS;
}

int RJ_llapi_test_get_type(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  if (argc != 1) {
    RedisModule_WrongArity(ctx);
    return REDISMODULE_ERR;
  }

  RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", "[\"\", 0, 0.0, false, {}, [], null]");
  RedisJSON js = RJ_API.japi->openKeyFromStr(ctx, TEST_NAME);

  size_t len; RJ_API.japi->getLen(js, &len); ASSERT(len == JSONType__EOF);
  for (int i = 0; i < len; ++i) {
    ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, i)) == (JSONType) i);
  }
  TEST_SUCCESS;
}

int RJ_llapi_test_get_value(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  if (argc != 1) {
    RedisModule_WrongArity(ctx);
    return REDISMODULE_ERR;
  }

  RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]");
  RedisJSON js = RJ_API.japi->openKeyFromStr(ctx, TEST_NAME);

  const char *s; size_t len;
  RJ_API.japi->getString(RJ_API.japi->getAt(js, JSONType_String), &s, &len);
  ASSERT(strncmp(s, "a", len) == 0);

  long long ll;
  RJ_API.japi->getInt(RJ_API.japi->getAt(js, JSONType_Int), &ll);
  ASSERT(ll == 1);

  double dbl;
  RJ_API.japi->getDouble(RJ_API.japi->getAt(js, JSONType_Double), &dbl);
  ASSERT(fabs(dbl - 0.1) < DBL_EPSILON);

  int b;
  RJ_API.japi->getBoolean(RJ_API.japi->getAt(js, JSONType_Bool), &b);
  ASSERT(b);

  len = 0;
  RJ_API.japi->getLen(RJ_API.japi->getAt(js, JSONType_Object), &len);
  ASSERT(len == 1);

  len = 0;
  RJ_API.japi->getLen(RJ_API.japi->getAt(js, JSONType_Array), &len);
  ASSERT(len == 1);

  TEST_SUCCESS;
}

int RJ_llapi_test_all(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  RedisModule_Call(ctx, "FLUSHALL", "");
  const int NUM_TESTS = 4;
  int PASSED_TESTS = 0;
  RedisModule_ReplyWithArray(ctx, 2);

  RedisModule_ReplyWithArray(ctx, NUM_TESTS);
  TESTFUNC(RJ_llapi_test_open_key);
  TESTFUNC(RJ_llapi_test_iterator);
  TESTFUNC(RJ_llapi_test_get_type);
  TESTFUNC(RJ_llapi_test_get_value);

  ASSERT(PASSED_TESTS == NUM_TESTS);
  TEST_SUCCESS;
}

int RedisModule_OnLoad(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  if (RedisModule_Init(ctx, MOD_PREFIX, 1, REDISMODULE_APIVER_1) == REDISMODULE_ERR) {
    return REDISMODULE_ERR;
  }
  GetJSONAPIs(ctx, 1);

  RegisterCmd(RJ_llapi_test_open_key);
  RegisterCmd(RJ_llapi_test_iterator);
  RegisterCmd(RJ_llapi_test_get_type);
  RegisterCmd(RJ_llapi_test_get_value);

  RegisterCmd(RJ_llapi_test_all);

  return REDISMODULE_OK;
}


