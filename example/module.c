#include "../redismodule.h"
#include "../rmutil/util.h"
#include "../rmutil/strings.h"

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
    long long x,y;
       
    
    // If we got SUM - return the sum of 2 consecutive arguments
    if (RMUtil_ParseArgsAfter("SUM", argv, argc, "ll", &x, &y) == REDISMODULE_OK) {
        RedisModule_ReplyWithLongLong(ctx, x+y);
        return REDISMODULE_OK;
    }
    // If we got PROD - return the product of 2 consecutive arguments
    if (RMUtil_ParseArgsAfter("PROD", argv, argc, "ll", &x, &y) == REDISMODULE_OK) {
        RedisModule_ReplyWithLongLong(ctx, x*y);
        return REDISMODULE_OK;
    }
    
    // something is fishy...
    RedisModule_ReplyWithError(ctx, "Invalid arguments");
    
    return REDISMODULE_OK;
}


int RedisModule_OnLoad(RedisModuleCtx *ctx) {
    
  if (RedisModule_Init(ctx,"example",1, REDISMODULE_APIVER_1) == REDISMODULE_ERR) {
    return REDISMODULE_ERR;  
  } 


   if (RedisModule_CreateCommand(ctx, "example.parse", ParseCommand, "readonly", 1,1,1)  
   == REDISMODULE_ERR) {
        return REDISMODULE_ERR;
   }


    return REDISMODULE_OK;
}