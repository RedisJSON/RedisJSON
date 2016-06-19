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
    PathNode *path;
    size_t len;
} LookupPath;


Node *Node_Find(Node *root, LookupPath *path, PathError *err);

#endif