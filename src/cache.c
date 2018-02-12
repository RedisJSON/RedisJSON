#include "json_type.h"
#include "cache.h"
#include <assert.h>

// Extern
LruCache jsonLruCache_g = {.maxEntries = LRUCACHE_DEFAULT_MAXENT,
                           .maxBytes = LRUCACHE_DEFAULT_MAXBYTE,
                           .minSize = LRUCACHE_DEFAULT_MINSIZE};

static void pluckEntry(LruCache *cache, LruPathEntry *entry) {
    LruPathEntry *prev = entry->lru_prev, *next = entry->lru_next;
    assert(entry->lru_prev != entry);
    assert(entry->lru_next != entry);
    if (next) {
        next->lru_prev = prev;
    }
    if (prev) {
        prev->lru_next = next;
    }

    if (entry == cache->newest) {
        cache->newest = prev;
    }
    if (entry == cache->oldest) {
        cache->oldest = next;
    }

    entry->lru_next = entry->lru_prev = NULL;
}

static void touchEntry(LruCache *cache, LruPathEntry *entry) {
    pluckEntry(cache, entry);
    if (cache->newest) {
        cache->newest->lru_prev = entry;
        entry->lru_next = cache->newest;
    }
    cache->newest = entry;
    if (cache->oldest == NULL) {
        cache->oldest = entry;
    }
}

// Don't free the purged entry, this entry will be reused
#define PURGE_NOFREE 0x01

// Don't change the item's key list
#define PURGE_NOKEYCHECK 0x02

// static size_t countKeyEntries(const LruPathEntry *ent) {
//     size_t n = 0;
//     for (; ent; ent = ent->key_next, n++) {
//     }
//     return n;
// }

static LruPathEntry *purgeEntry(LruCache *cache, LruPathEntry *entry, int options) {
    pluckEntry(cache, entry);
    cache->numEntries--;
    cache->numBytes -= sdslen(entry->value);

    // Clear from the JSON's list
    int found = 0;
    LruPathEntry *prev = NULL;
    for (LruPathEntry *cur = entry->parent->lruEntries; cur; cur = cur->key_next) {
        if (cur == entry) {
            found = 1;
            break;
        } else {
            prev = cur;
        }
    }

    assert(found);
    if (prev) {
        prev->key_next = entry->key_next;
    } else {
        entry->parent->lruEntries = entry->key_next;
    }

    if (!(options & PURGE_NOFREE)) {
        sdsfree(entry->path);
        sdsfree(entry->value);
        free(entry);
        entry = NULL;
    }
    return entry;
}

const sds LruCache_GetValue(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen) {
    LruPathEntry *ent = NULL;
    // printf("Root: %p\n", json->lruEntries);
    for (LruPathEntry *root = json->lruEntries; root; root = root->key_next) {
        if (sdslen(root->path) != pathLen) {
            // printf("len mismatch!\n");
            continue;
        } else if (strncmp(root->path, path, pathLen)) {
            // printf("str mismatch!\n");
            continue;
        } else {
            ent = root;
            break;
        }
    }
    if (!ent) {
        // printf("Match not found!\n");
        return NULL;
    }

    // Place LRU entry at the head of the list
    touchEntry(cache, ent);
    return ent->value;
}

void LruCache_AddValue(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen,
                       const char *value, size_t valueLen) {
    if (valueLen < cache->minSize) {
        return;
    }

    LruPathEntry *newEnt;
    if (cache->numEntries >= cache->maxEntries || cache->numBytes >= cache->maxBytes) {
        newEnt = purgeEntry(cache, cache->oldest, PURGE_NOFREE);
        newEnt->value = sdscpylen(newEnt->value, value, valueLen);
        newEnt->path = sdscpylen(newEnt->path, path, pathLen);
    } else {
        newEnt = calloc(1, sizeof(*newEnt));
        newEnt->path = sdsnewlen(path, pathLen);
        newEnt->value = sdsnewlen(value, valueLen);
    }

    // Set the entries own fields
    newEnt->key_next = json->lruEntries;
    newEnt->parent = json;
    json->lruEntries = newEnt;

    touchEntry(cache, newEnt);
    cache->numEntries++;
    cache->numBytes += valueLen;
}

static int shouldClearPath(const sds curPath, const char *path, size_t pathLen) {
    if (pathLen == 0) {
        // Clearing the root. Remove all
        return 1;
    }
    size_t curLen = sdslen(curPath);
    if (curLen == 0) {
        // Root path contains all other child paths
        return 1;
    }

    if (curLen > pathLen) {
        // Check if current node is a child of the search path
        return !strncmp(path, curPath, pathLen);
    } else {
        return !strncmp(path, curPath, curLen);
    }
}

void LruCache_ClearValues(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen) {
    // Remove all paths which are affected by this entry..
    LruPathEntry *ent = json->lruEntries;
    while (ent) {
        if (!shouldClearPath(ent->path, path, pathLen)) {
            // Not included in current path
            ent = ent->key_next;
            continue;
        }
        // Otherwise, purge the entry
        LruPathEntry *next = ent->key_next;
        purgeEntry(cache, ent, 0);
        ent = next;
    }
}

void LruCache_ClearKey(LruCache *cache, JSONType_t *json) {
    LruCache_ClearValues(cache, json, NULL, 0);
}