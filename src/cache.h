/*
 * ReJSON - a JSON data type for Redis
 * Copyright (C) 2017 Redis Labs
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

/**
 * LRU Entry, per path. Stored under keys
 */
#include "rejson.h"
#include <rmutil/sds.h>
typedef struct LruPathEntry {
    // Prev/Next in the LRU itself
    struct LruPathEntry *lru_prev;
    struct LruPathEntry *lru_next;

    // When deleting keys, know which keys map to which entries
    struct LruPathEntry *key_next;

    // When deleting an entry, remove it from the head of the key list, if
    // a head.
    JSONType_t *parent;
    sds path;
    sds value;
} LruPathEntry;

typedef struct {
    LruPathEntry *newest;
    LruPathEntry *oldest;

    // Number of total entries within the LRU
    size_t numEntries;

    // Number of bytes within the LRU
    size_t numBytes;

    // Maximum number of allowable entries within the LRU (is this needed?)
    size_t maxEntries;

    // Maximum number of bytes allowed in the LRU
    size_t maxBytes;

    // Minimum entry size. Entries smaller than this are not used because it's usually
    // cheaper to construct the response on the fly
    size_t minSize;
} LruCache;

#define LRUCACHE_DEFAULT_MINSIZE 0
#define LRUCACHE_DEFAULT_MAXBYTE (1 << 20)
#define LRUCACHE_DEFAULT_MAXENT 20000

extern LruCache jsonLruCache_g;
#define REJSON_LRUCACHE_GLOBAL (&jsonLruCache_g)

/**
 * Get the value from the LRU cache. This renews the entry within the LRU
 */
const sds LruCache_GetValue(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen);

/**
 * Set the value for a given path. It is assumed that the value for the current
 * path does not yet exist. This will insert
 */
void LruCache_AddValue(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen,
                       const char *value, size_t valueLen);

// Clear all cache items for a given path
void LruCache_ClearValues(LruCache *cache, JSONType_t *json, const char *path, size_t pathLen);

// Clears all values for a given key
void LruCache_ClearKey(LruCache *cache, JSONType_t *json);