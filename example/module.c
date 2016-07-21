#include "../redismodule.h"
#include "../rmutil/util.h"
#include "../rmutil/strings.h"
#include "../rmutil/test_util.h"

/* EXAMPLE.PARSE [SUM <x> <y>] | [PROD <x> <y>]
*  Demonstrates the automatic arg parsing utility.
*  If the command receives "SUM <x> <y>" it returns their sum
*  If it receives "PROD <x> <y>" it returns their product
*/
int ParseCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {

  // we must have at least 4 args
  if (argc < 4) {
    return RedisModule_WrongArity(ctx);
  }

  // init auto memory for created strings
  RedisModule_AutoMemory(ctx);
  long long x, y;

  // If we got SUM - return the sum of 2 consecutive arguments
  if (RMUtil_ParseArgsAfter("SUM", argv, argc, "ll", &x, &y) ==
      REDISMODULE_OK) {
    RedisModule_ReplyWithLongLong(ctx, x + y);
    return REDISMODULE_OK;
  }

  // If we got PROD - return the product of 2 consecutive arguments
  if (RMUtil_ParseArgsAfter("PROD", argv, argc, "ll", &x, &y) ==
      REDISMODULE_OK) {
    RedisModule_ReplyWithLongLong(ctx, x * y);
    return REDISMODULE_OK;
  }

  // something is fishy...
  RedisModule_ReplyWithError(ctx, "Invalid arguments");

  return REDISMODULE_ERR;
}

/*
* example.HGETSET <key> <element> <value>
* Atomically set a value in a HASH key to <value> and return its value before
* the HSET.
*
* Basically atomic HGET + HSET
*/
int HGetSetCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {

  // we need EXACTLY 4 arguments
  if (argc != 4) {
    return RedisModule_WrongArity(ctx);
  }
  RedisModule_AutoMemory(ctx);

  // open the key and make sure it's indeed a HASH and not empty
  RedisModuleKey *key =
      RedisModule_OpenKey(ctx, argv[1], REDISMODULE_READ | REDISMODULE_WRITE);
  if (RedisModule_KeyType(key) != REDISMODULE_KEYTYPE_HASH &&
      RedisModule_KeyType(key) != REDISMODULE_KEYTYPE_EMPTY) {
    return RedisModule_ReplyWithError(ctx, REDISMODULE_ERRORMSG_WRONGTYPE);
  }

  // get the current value of the hash element
  RedisModuleCallReply *rep =
      RedisModule_Call(ctx, "HGET", "ss", argv[1], argv[2]);
  RMUTIL_ASSERT_NOERROR(rep);

  // set the new value of the element
  RedisModuleCallReply *srep =
      RedisModule_Call(ctx, "HSET", "sss", argv[1], argv[2], argv[3]);
  RMUTIL_ASSERT_NOERROR(srep);

  // if the value was null before - we just return null
  if (RedisModule_CallReplyType(rep) == REDISMODULE_REPLY_NULL) {
    RedisModule_ReplyWithNull(ctx);
    return REDISMODULE_OK;
  }

  // forward the HGET reply to the client
  RedisModule_ReplyWithCallReply(ctx, rep);
  return REDISMODULE_OK;
}

// Test the the PARSE command
int testParse(RedisModuleCtx *ctx) {

  RedisModuleCallReply *r =
      RedisModule_Call(ctx, "example.parse", "ccc", "SUM", "5", "2");
  RMUtil_Assert(RedisModule_CallReplyType(r) == REDISMODULE_REPLY_INTEGER);
  RMUtil_AssertReplyEquals(r, "7");

  r = RedisModule_Call(ctx, "example.parse", "ccc", "PROD", "5", "2");
  RMUtil_Assert(RedisModule_CallReplyType(r) == REDISMODULE_REPLY_INTEGER);
  RMUtil_AssertReplyEquals(r, "10");
  return 0;
}

// test the HGETSET command
int testHgetSet(RedisModuleCtx *ctx) {
  RedisModuleCallReply *r =
      RedisModule_Call(ctx, "example.hgetset", "ccc", "foo", "bar", "baz");
  RMUtil_Assert(RedisModule_CallReplyType(r) != REDISMODULE_REPLY_ERROR);

  r = RedisModule_Call(ctx, "example.hgetset", "ccc", "foo", "bar", "bag");
  RMUtil_Assert(RedisModule_CallReplyType(r) == REDISMODULE_REPLY_STRING);
  RMUtil_AssertReplyEquals(r, "baz");
  r = RedisModule_Call(ctx, "example.hgetset", "ccc", "foo", "bar", "bang");
  RMUtil_AssertReplyEquals(r, "bag");
  return 0;
}

// Unit test entry point for the module
int TestModule(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
  RedisModule_AutoMemory(ctx);

  RMUtil_Test(testParse);
  RMUtil_Test(testHgetSet);

  RedisModule_ReplyWithSimpleString(ctx, "PASS");
  return REDISMODULE_OK;
}

int RedisModule_OnLoad(RedisModuleCtx *ctx) {

  // Register the module itself
  if (RedisModule_Init(ctx, "example", 1, REDISMODULE_APIVER_1) ==
      REDISMODULE_ERR) {
    return REDISMODULE_ERR;
  }

  // register example.parse - the default registration syntax
  if (RedisModule_CreateCommand(ctx, "example.parse", ParseCommand, "readonly",
                                1, 1, 1) == REDISMODULE_ERR) {
    return REDISMODULE_ERR;
  }

  // register example.hgetset - using the shortened utility registration macro
  RMUtil_RegisterWriteCmd(ctx, "example.hgetset", HGetSetCommand);

  // register the unit test
  RMUtil_RegisterWriteCmd(ctx, "example.test", TestModule);

  return REDISMODULE_OK;
}
