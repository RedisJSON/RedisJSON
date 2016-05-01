#ifndef __UTIL_H__
#define __UTIL_H__

#include <redismodule.h>

// make sure the response is not NULL or an error, and if it is sends the error to the client and exit the current function
#define  REDIS_ASSERT_NOERROR(r) \
    if (r == NULL) { \
        return RedisModule_ReplyWithError(ctx,"ERR reply is NULL"); \
    } else if (RedisModule_CallReplyType(r) == REDISMODULE_REPLY_ERROR) { \
        RedisModule_ReplyWithCallReply(ctx,r); \
        return REDISMODULE_ERR; \
    }


/* RedisModule utilities. */

/* Return the offset of an arg if it exists in the arg list, or 0 if it's not there */
int RMUtil_ArgExists(const char *arg, RedisModuleString **argv, int argc, int offset);

// NOT IMPLEMENTED YET
int RMUtil_ParseArgs(RedisModuleString **argv, int argc, const char *fmt, ...);

// A single key/value entry in a redis info map
typedef struct {
    const char *key;
    const char *val;
} RMUtilInfoEntry;

// Representation of INFO command response, as a list of k/v pairs
typedef struct {
    RMUtilInfoEntry *entries;
    int numEntries;
} RMUtilInfo;

/*
* Get redis INFO result and parse it as RMUtilInfo.
* Returns NULL if something goes wrong.
* The resulting object needs to be freed with RMUtilRedisInfo_Free
*/
RMUtilInfo *RMUtil_GetRedisInfo(RedisModuleCtx *ctx);

/*
* Free an RMUtilInfo object and its entries
*/
void RMUtilRedisInfo_Free(RMUtilInfo *info);

/**
* Get an integer value from an info object. Returns 1 if the value was found and 
* is an integer, 0 otherwise. the value is placed in 'val'
*/
int RMUtilInfo_GetInt(RMUtilInfo *info, const char *key, long long *val);

/*
* Get a string value from an info object. The value is placed in str.
* Returns 1 if the key was found, 0 if not 
*/
int RMUtilInfo_GetString(RMUtilInfo *info, const char *key, const char **str);

/*
* Get a double value from an info object. Returns 1 if the value was found and is 
* a correctly formatted double, 0 otherwise. the value is placed in 'd'
*/
int RMUtilInfo_GetDouble(RMUtilInfo *info, const char *key, double *d);


#endif
