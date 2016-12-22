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

#include "path.h"

Node *__pathNode_eval(PathNode *pn, Node *n, PathError *err) {
    *err = E_OK;
    if (!n) {
        goto badtype;
    }

    if (n->type == N_ARRAY) {
        Node *rn = NULL;
        if (NT_INDEX == pn->type) {
            int index = pn->value.index;
            // translate negative values
            if (index < 0) index = n->value.arrval.len + index;            
            int rc = Node_ArrayItem(n, index, &rn);
            if (rc != OBJ_OK) {
                *err = E_NOINDEX;
            }
        } else {
            goto badtype;
        }
        return rn;
    }

    if (n->type == N_DICT) {
        if (pn->type != NT_KEY) {
            goto badtype;
        }
        Node *rn = NULL;
        int rc = Node_DictGet(n, pn->value.key, &rn);
        if (rc != OBJ_OK) {
            *err = E_NOKEY;
        }
        return rn;
    }

badtype:
    *err = E_BADTYPE;
    return NULL;
}

PathError SearchPath_Find(SearchPath *path, Node *root, Node **n) {
    Node *current = root;
    PathError ret;
    for (int i = 0; i < path->len; i++) {
        current = __pathNode_eval(&path->nodes[i], current, &ret);
        if (ret != E_OK) {
            *n = NULL;
            return ret;
        }
    }
    *n = current;
    return E_OK;
}

PathError SearchPath_FindEx(SearchPath *path, Node *root, Node **n, Node **p, int *errnode) {
    Node *current = root;
    Node *prev = NULL;
    Node *next;
    PathError ret;

    for (int i = 0; i < path->len; i++) {
        prev = current;
        current = __pathNode_eval(&path->nodes[i], current, &ret);
        if (ret != E_OK) {
            *errnode = i;
            *p = prev;
            *n = NULL;
            return ret;
        }
    }
    *p = prev;
    *n = current;
    return E_OK;
}

SearchPath NewSearchPath(size_t cap) { return (SearchPath){calloc(cap, sizeof(PathNode)), 0, cap}; }

void __searchPath_append(SearchPath *p, PathNode pn) {
    if (p->len >= p->cap) {
        p->cap = p->cap ? MIN(p->cap * 2, 1024) : 1;
        p->nodes = realloc(p->nodes, p->cap * sizeof(PathNode));
    }

    p->nodes[p->len++] = pn;
}

void SearchPath_AppendIndex(SearchPath *p, int idx) {
    PathNode pn;
    pn.type = NT_INDEX;
    pn.value.index = idx;
    __searchPath_append(p, pn);
}

void SearchPath_AppendKey(SearchPath *p, const char *key, const size_t len) {
    PathNode pn;
    pn.type = NT_KEY;
    pn.value.key = strndup(key, len);
    __searchPath_append(p, pn);
}

void SearchPath_AppendRoot(SearchPath *p) {
    PathNode pn;
    pn.type = NT_ROOT;
    __searchPath_append(p, pn);
}

void SearchPath_Free(SearchPath *p) {
    if (p->nodes) {
        for (int i = 0; i < p->len; i++) {
            if (p->nodes[i].type == NT_KEY) {
                free((char *)p->nodes[i].value.key);
            }
        }
    }

    free(p->nodes);
}
