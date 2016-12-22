/*
* Copyright (C) 2016 Redis Labs
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

#ifndef __PATH_H__
#define __PATH_H__

#include <string.h>
#include <sys/param.h>
#include "object.h"

#ifdef REDIS_MODULE_TARGET
#include <alloc.h>
#endif

/* The type of a path node */
typedef enum {
    NT_ROOT,
    NT_KEY,
    NT_INDEX,
} PathNodeType;

/* Error codes returned from path lookups */
typedef enum {
    // OK
    E_OK,
    // dict key does not exist
    E_NOKEY,

    // array index is out of range
    E_NOINDEX,

    // the path predicate does not match the node type
    E_BADTYPE,
} PathError;

/* A single lookup node in a lookup path. A lookup path is just a list of nodes */
typedef struct {
    PathNodeType type;
    union {
        int index;
        const char *key;
    } value;
} PathNode;

/** Evaluate a single path node against an object node */
Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err);

/**
* A search path parsed from JSON or other formats, representing
* a lookup path in the object tree
*/
typedef struct {
    PathNode *nodes;
    size_t len;
    size_t cap;
} SearchPath;

/* Create a new search path. cap can be 0 if you don't know it */
SearchPath NewSearchPath(size_t cap);

/* Append an array index selection node to the path */
void SearchPath_AppendIndex(SearchPath *p, int idx);

/* Append a string key lookup node to the search path */
void SearchPath_AppendKey(SearchPath *p, const char *key, const size_t len);

/* Appends a root node to the search path (makes sense only as the first append)  */
void SearchPath_AppendRoot(SearchPath *p);

/* Free a search path and all its nodes */
void SearchPath_Free(SearchPath *p);

// Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err);

/**
* Find a node in an object tree based on a parsed path.
* An error code is returned, and if a node matches the path, its value
* is put into n's pointer. This can be NULL if the lookup matches a NULL node.
*/
PathError SearchPath_Find(SearchPath *path, Node *root, Node **n);

/**
* Like SearchPath_Find, but sets p to the parent container of n. In case of E_NOKEY, E_NOINDEX,
* and E_INFINDEX returns the path level of the error in errnode.
*/
PathError SearchPath_FindEx(SearchPath *path, Node *root, Node **n, Node **p, int *errnode);

#endif