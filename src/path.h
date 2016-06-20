#ifndef __PATH_H__
#define __PATH_H__
#include "object.h"

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
void SearchPath_AppendKey(SearchPath *p, const char *key);

/* Free a search path and all its nodes */
void SearchPath_Free(SearchPath *p);

Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err);

/**
* Find a node in an object tree based on a parsed path.
* An error code is returned, and if a node matches the path, its value
* is put into n's pointer. This can be NULL if the lookup matches a NULL node.
*/
PathError SearchPath_Find(SearchPath *path, Node *root, Node **n);

#endif