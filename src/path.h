#ifndef __PATH_H__
#define __PATH_H__
#include "object.h"

typedef enum {
    NT_ROOT,
    NT_KEY,
    NT_INDEX,
} PathNodeType;

typedef enum {
    E_OK,
    E_NOKEY,
    E_NOINDEX,
    E_BADTYPE,
} PathError;

typedef struct {
    PathNodeType type;
    union {
        int index;
        const char *key;
    } value;
} PathNode; 

Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err);

typedef struct {
    PathNode *nodes;
    size_t len;
    size_t cap;
} SearchPath;

SearchPath NewSearchPath(size_t cap);

void SearchPath_AppendIndex(SearchPath *p, int idx);
void SearchPath_AppendKey(SearchPath *p, const char *key);
void SearchPath_Free(SearchPath *p);

Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err);

PathError SearchPath_Find(SearchPath *path, Node *root, Node **n);

#endif