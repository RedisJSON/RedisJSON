#include "path.h"


Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err) {
    *err = E_OK;
    if (!n) {
       goto badtype;
    }

    if (n->type == N_ARRAY) {
        if (pn->type != NT_INDEX) {
            goto badtype;
        }
        Node *rn = NULL;
        int rc = Node_ArrayItem(n, pn->value.index, &rn);
        if (rc != OBJ_OK) {
            *err = E_NOINDEX;
        }
        return rn;
    }
    if (n->type == N_OBJECT) {
        if (pn->type != NT_KEY) {
            goto badtype;
        }
        Node *rn = NULL;
        int rc = Node_ObjGet(n, pn->value.key, &rn);
        if (rc != OBJ_OK) {
            *err = E_NOKEY;
        }
        return rn;
    }
    
badtype:
    *err = E_BADTYPE;
    return NULL;
}

PathError Node_Find(Node *root, LookupPath *path, Node **n) {
    Node *current = root;
    PathError ret;
    for (int i = 0; i < path->len; i++) {
       current = __pathNode_eval(&path->path[i], current, &ret);
       if (ret != E_OK) {
           return ret;
       } 
    }
    *n = current;
    return E_OK;
}
