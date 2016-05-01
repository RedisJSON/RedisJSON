#include <stdlib.h>
#include <errno.h>
#include <math.h>
#include <ctype.h>
#include <sys/time.h>
#include <limits.h>
#include <string.h>
#include "util.h"

/**
Check if an argument exists in an argument list (argv,argc), starting at offset.
@return 0 if it doesn't exist, otherwise the offset it exists in
*/
int RMUtil_ArgExists(const char *arg, RedisModuleString **argv, int argc, int offset) {

    for (; offset < argc; offset++) {
        size_t l;
        const char *carg = RedisModule_StringPtrLen(argv[offset], &l);
        if (carg != NULL && strncasecmp(carg, arg, l) == 0) {
            return offset;
        }
    }
    return 0;
}

RMUtilInfo *RMUtil_GetRedisInfo(RedisModuleCtx *ctx) {
    
    RedisModuleCallReply *r = RedisModule_Call(ctx, "INFO", "c", "all");
    if (r == NULL || RedisModule_CallReplyType(r) == REDISMODULE_REPLY_ERROR) {
        return NULL;
    }
    
    
    int cap = 100; // rough estimate of info lines
    RMUtilInfo *info = malloc(sizeof(RMUtilInfo));
    info->entries = calloc(cap, sizeof(RMUtilInfoEntry));
    
    int i = 0;
    char *text = (char *)RedisModule_StringPtrLen(RedisModule_CreateStringFromCallReply(r), NULL);
    char *line = text;
    while (line) {
        char *line = strsep(&text, "\r\n");
        if (line == NULL)  break;
        
        if (!(*line >= 'a' && *line <= 'z')) { //skip non entry lines 
            continue;
        }
        
        char *key = strsep(&line, ":");
        info->entries[i].key = key;
        info->entries[i].val = line;
        printf("Got info '%s' = '%s'\n", key, line);
        i++;
        if (i >= cap) {
            cap *=2;
            info->entries = realloc(info->entries, cap*sizeof(RMUtilInfoEntry));
        }
    } 
    info->numEntries = i;
    
    return info;
    
}
void RMUtilRedisInfo_Free(RMUtilInfo *info) {
    
    free(info->entries);
    free(info);
    
}

int RMUtilInfo_GetInt(RMUtilInfo *info, const char *key, long long *val) {
    
     const char *p = NULL;
     if (!RMUtilInfo_GetString(info, key, &p)) {
         return 0;
     }
     
     *val = strtoll(p, NULL, 10);
     if ((errno == ERANGE && (*val == LONG_MAX || *val == LONG_MIN)) ||
        (errno != 0 && *val == 0)) {
       *val = -1;
       return 0;
    }
     
    
    return 1;
}


int RMUtilInfo_GetString(RMUtilInfo *info, const char *key, const char **str) {
    int i;
    for (i = 0; i < info->numEntries; i++) {
        if (!strcmp(key, info->entries[i].key)) {
            *str = info->entries[i].val;
            return 1;
        }
    }
    return 0;
}

int RMUtilInfo_GetDouble(RMUtilInfo *info, const char *key, double *d) {
     const char *p = NULL;
     if (!RMUtilInfo_GetString(info, key, &p)) {
         printf("not found %s\n", key);
         return 0;
     }
     
     *d = strtod(p, NULL);
     printf("p: %s, d: %f\n",p,*d);
     if ((errno == ERANGE && (*d == HUGE_VAL || *d == -HUGE_VAL)) ||
        (errno != 0 && *d == 0)) {
       return 0;
    }
     
    
    return 1;
}
