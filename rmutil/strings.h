#ifndef __RMUTIL_STRINGS_H__
#define __RMUTIL_STRINGS_H__

#include <redismodule.h>

/*
* Create a new RedisModuleString object from a printf-style format and arguments.
* Note that RedisModuleString objects CANNOT be used as formatting arguments.
*/
RedisModuleString *RMUtils_CreateFormattedString(RedisModuleCtx *ctx, const char *fmt, ...);

/* Return 1 if the two strings are equal. Case *sensitive* */
int RMUtils_StringEquals(RedisModuleString *s1, RedisModuleString *s2);

/* Converts a redis string to lowercase in place without reallocating anything */
void RMUtils_StringToLower(RedisModuleString *s);

/* Converts a redis string to uppercase in place without reallocating anything */
void RMUtils_StringToUpper(RedisModuleString *s);


#endif
