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

#include "object.h"

Node *__newNode(NodeType t) {
    Node *ret = malloc(sizeof(Node));
    ret->type = t;
    return ret;
}

Node *NewBoolNode(int val) {
    Node *ret = __newNode(N_BOOLEAN);
    ret->value.boolval = val != 0;
    return ret;
}

Node *NewDoubleNode(double val) {
    Node *ret = __newNode(N_NUMBER);
    ret->value.numval = val;
    return ret;
}

Node *NewIntNode(int64_t val) {
    Node *ret = __newNode(N_INTEGER);
    ret->value.intval = val;
    return ret;
}

Node *NewStringNode(const char *s, uint32_t len) {
    Node *ret = __newNode(N_STRING);
    ret->value.strval.data = strndup(s, len);
    ret->value.strval.len = len;
    return ret;
}

Node *NewCStringNode(const char *s) { return NewStringNode(s, strlen(s)); }

Node *NewKeyValNode(const char *key, uint32_t len, Node *n) {
    Node *ret = __newNode(N_KEYVAL);
    ret->value.kvval.key = strndup(key, len);
    ret->value.kvval.val = n;
    return ret;
}

Node *NewArrayNode(uint32_t cap) {
    Node *ret = __newNode(N_ARRAY);
    ret->value.arrval.cap = cap;
    ret->value.arrval.len = 0;
    ret->value.arrval.entries = calloc(cap, sizeof(Node *));
    return ret;
}

Node *NewDictNode(uint32_t cap) {
    Node *ret = __newNode(N_DICT);
    ret->value.dictval.cap = cap;
    ret->value.dictval.len = 0;
    ret->value.dictval.entries = calloc(cap, sizeof(Node *));
    return ret;
}

void __node_FreeKV(Node *n) {
    Node_Free(n->value.kvval.val);
    free((char *)n->value.kvval.key);
    free(n);
}

void __node_FreeObj(Node *n) {
    for (int i = 0; i < n->value.dictval.len; i++) {
        Node_Free(n->value.dictval.entries[i]);
    }
    if (n->value.dictval.entries) free(n->value.dictval.entries);
    free(n);
}

void __node_FreeArr(Node *n) {
    for (int i = 0; i < n->value.arrval.len; i++) {
        Node_Free(n->value.arrval.entries[i]);
    }
    free(n->value.arrval.entries);
    free(n);
}

void __node_FreeString(Node *n) {
    free((char *)n->value.strval.data);
    free(n);
}

void Node_Free(Node *n) {
    // ignore NULL nodes
    if (!n) return;

    switch (n->type) {
        case N_ARRAY:
            __node_FreeArr(n);
            break;
        case N_DICT:
            __node_FreeObj(n);
            break;
        case N_STRING:
            __node_FreeString(n);
            break;
        case N_KEYVAL:
            __node_FreeKV(n);
            break;
        default:
            free(n);
    }
}

int Node_Length(const Node *n) {
    // Length is only defined for arrays, dictionaries and strings
    if (n) {
        switch (n->type) {
            case N_ARRAY:
                return n->value.arrval.len;
                break;
            case N_DICT:
                return n->value.dictval.len;
                break;
            case N_STRING:
                return n->value.strval.len;
                break;
            default:
                break;
        }
    }

    return -1;
}

int Node_StringAppend(Node *dst, Node *src) {
    t_string *d = &dst->value.strval;
    t_string *s = &src->value.strval;

    char *newval = calloc(d->len + s->len, sizeof(char));
    strncpy(newval, d->data, d->len);
    strncpy(&newval[d->len], s->data, s->len);

    free((char *)d->data);
    d->data = newval;
    d->len += s->len;

    return OBJ_OK;
}

int Node_ArrayDelRange(Node *arr, const int index, const int count) {
    t_array *a = &arr->value.arrval;

    if (count <= 0 || !a->len) return OBJ_OK;

    int start = index < 0 ? MAX(a->len + index, 0) : MIN(index, a->len - 1);
    int stop = MIN(start + count, a->len);  // stop is exclusive

    // free range
    for (int i = start; i < stop; i++) Node_Free(a->entries[i]);

    // move whatever remains on the left side
    if (stop < a->len)
        memmove(&a->entries[start], &a->entries[stop], (a->len - stop) * sizeof(Node *));

    // adjust length
    a->len -= stop - start;

    return OBJ_OK;
}

/* Enlarge the capacity of an array to hold at least its current length + addlen. */
void __node_ArrayMakeRoomFor(Node *arr, uint32_t addlen) {
    t_array *a = &arr->value.arrval;
    uint32_t newcap = a->len + addlen;

    // Nothing to do if enough capacity is already available
    if (a->cap >= newcap) return;

    /* Find a reasonable next capacity.
    * For small numbers we grow to the next power of 2:
    * http://graphics.stanford.edu/~seander/bithacks.html#RoundUpPowerOf2
    */
    uint32_t nextcap = newcap;
    nextcap--;
    nextcap |= nextcap >> 1;
    nextcap |= nextcap >> 2;
    nextcap |= nextcap >> 4;
    nextcap |= nextcap >> 8;
    nextcap |= nextcap >> 16;
    nextcap++;

    // For larger capacities, e.g. 1MB, we chunk it.
    const uint32_t CHUNK_SIZE = 1 << 20;
    if (nextcap > CHUNK_SIZE) {
        nextcap = ((newcap / CHUNK_SIZE) + 1) * CHUNK_SIZE;
    }

    a->cap = nextcap;
    a->entries = realloc(a->entries, a->cap * sizeof(Node *));
}

int Node_ArrayInsert(Node *arr, int index, Node *sub) {
    t_array *a = &arr->value.arrval;
    t_array *s = &sub->value.arrval;

    if (index < 0) index = (int)a->len + index;     // translate negative index value
    if (index < 0) index = 0;                       // not in range always start at the beginning
    if (index > (int)a->len) index = (int)a->len;   // or appended at the end

    __node_ArrayMakeRoomFor(arr, s->len);
    if (index < (int) a->len) {                     //  shift contents to the right
        memmove(&a->entries[index + s->len], &a->entries[index], (a->len - index) * sizeof(Node *));
    }

    // copy the references
    memcpy(&a->entries[index], s->entries, s->len * sizeof(Node *));
    a->len += s->len;

    // destroy all traces
    s->len = 0;
    Node_Free(sub);

    return OBJ_OK;
}

int Node_ArrayAppend(Node *arr, Node *n) {
    t_array *a = &arr->value.arrval;
    __node_ArrayMakeRoomFor(arr, 1);
    a->entries[a->len++] = n;

    return OBJ_OK;
}

int Node_ArrayPrepend(Node *arr, Node *n) {
    Node *sub = NewArrayNode(1);
    Node_ArrayAppend(sub, n);
    return Node_ArrayInsert(arr, 0, sub);
}

int Node_ArraySet(Node *arr, int index, Node *n) {
    t_array *a = &arr->value.arrval;

    // invalid index!
    if (index < 0 || index >= a->len) {
        return OBJ_ERR;
    }
    a->entries[index] = n;

    return OBJ_OK;
}

int Node_ArrayItem(Node *arr, int index, Node **n) {
    t_array *a = &arr->value.arrval;

    // invalid index!
    if (index < 0 || index >= a->len) {
        *n = NULL;
        return OBJ_ERR;
    }
    *n = a->entries[index];
    return OBJ_OK;
}

int Node_ArrayIndex(Node *arr, Node *n, int start, int stop) {
    t_array *a = &arr->value.arrval;

    // Break early for empty arrays or non scalar nodes
    if (!a->len || !NODE_IS_SCALAR(n)) {
        return -1;
    }

    // convert negative indices
    if (start < 0) start = a->len + start;
    if (stop < 0) stop = a->len + stop;

    // check and adjust for out of range indices
    if (start < 0) start = 0;                               // start at the beginning
    if (start >= (int) a->len) start = MAX(0, a->len - 1);  // but don't overdo it
    if (stop >= (int) a->len) stop = 0;                     // get including the end
    if (stop == 0) stop = a->len;                           // stop after the end
    if (stop < start) stop = start;                         // don't search at all

    // search for the value
    for (int i = start; i < stop; i++) {
        if (!n && !a->entries[i]) return i;             // both are nulls
        if (!n || !a->entries[i]) continue;             // just one null
        if (a->entries[i]->type != n->type) continue;   // types not the same

        // Check equality per scalar type
        switch (n->type) {
            case N_STRING:
                if ((n->value.strval.len == a->entries[i]->value.strval.len) &&
                    !strncmp(n->value.strval.data, a->entries[i]->value.strval.data,
                             n->value.strval.len)) {
                    return i;
                }
                break;
            case N_NUMBER:
                if (n->value.numval == a->entries[i]->value.numval) return i;
                break;
            case N_INTEGER:
                if (n->value.intval == a->entries[i]->value.intval) return i;
                break;
            case N_BOOLEAN:
                if (n->value.boolval == a->entries[i]->value.boolval) return i;
                break;
            default:
                break;
        }  // switch (n->type)
    }      // for
    
    return -1;  // unfound
}

Node *__obj_find(t_dict *o, const char *key, int *idx) {
    for (int i = 0; i < o->len; i++) {
        if (!strcmp(key, o->entries[i]->value.kvval.key)) {
            if (idx) *idx = i;

            return o->entries[i];
        }
    }

    return NULL;
}

#define __obj_insert(o, n)                                             \
    if (o->len >= o->cap) {                                            \
        o->cap = o->cap ? MIN(o->cap * 2, 1024 * 1024) : 1;            \
        o->entries = realloc(o->entries, o->cap * sizeof(t_keyval *)); \
    }                                                                  \
    o->entries[o->len++] = n;

int Node_DictSet(Node *obj, const char *key, Node *n) {
    t_dict *o = &obj->value.dictval;

    if (key == NULL) return OBJ_ERR;

    int idx;
    Node *kv = __obj_find(o, key, &idx);
    // first find a replacement possiblity
    if (kv) {
        if (kv->value.kvval.val) {
            Node_Free(kv->value.kvval.val);
        }
        kv->value.kvval.val = n;
        return OBJ_OK;
    }

    // append another entry
    __obj_insert(o, NewKeyValNode(key, strlen(key), n));

    return OBJ_OK;
}

int Node_DictSetKeyVal(Node *obj, Node *kv) {
    t_dict *o = &obj->value.dictval;

    if (kv->value.kvval.key == NULL) return OBJ_ERR;

    int idx;
    Node *_kv = __obj_find(o, kv->value.kvval.key, &idx);
    // first find a replacement possiblity
    if (_kv) {
        o->entries[idx] = kv;
        Node_Free(_kv);
        return OBJ_OK;
    }

    // append another entry
    __obj_insert(o, kv);

    return OBJ_OK;
}

int Node_DictDel(Node *obj, const char *key) {
    if (key == NULL) return OBJ_ERR;

    t_dict *o = &obj->value.dictval;

    int idx = -1;
    Node *kv = __obj_find(o, key, &idx);

    // tried to delete a non existing node
    if (!kv) return OBJ_ERR;

    // let's delete the node's memory
    if (kv->value.kvval.val) {
        Node_Free(kv->value.kvval.val);
    }
    free((char *)kv->value.kvval.key);

    // replace the deleted entry and the top entry to avoid holes
    if (idx < o->len - 1) {
        o->entries[idx] = o->entries[o->len - 1];
    }
    o->len--;

    return OBJ_OK;
}

int Node_DictGet(Node *obj, const char *key, Node **val) {
    if (key == NULL) return OBJ_ERR;

    t_dict *o = &obj->value.dictval;

    int idx = -1;
    Node *kv = __obj_find(o, key, &idx);

    // not found!
    if (!kv) return OBJ_ERR;

    *val = kv->value.kvval.val;
    return OBJ_OK;
}

void __objTraverse(Node *n, NodeVisitor f, void *ctx) {
    t_dict *o = &n->value.dictval;

    f(n, ctx);
    for (int i = 0; i < o->len; i++) {
        Node_Traverse(o->entries[i], f, ctx);
    }
}
void __arrTraverse(Node *n, NodeVisitor f, void *ctx) {
    t_array *a = &n->value.arrval;
    f(n, ctx);

    for (int i = 0; i < a->len; i++) {
        Node_Traverse(a->entries[i], f, ctx);
    }
}

void Node_Traverse(Node *n, NodeVisitor f, void *ctx) {
    // for null node - just call the callback
    if (!n) {
        f(n, ctx);
        return;
    }
    switch (n->type) {
        case N_ARRAY:
            __arrTraverse(n, f, ctx);
            break;
        case N_DICT:
            __objTraverse(n, f, ctx);
            break;
        // for all other types - just call the callback
        default:
            f(n, ctx);
    }
}

#define __node_indent(depth)          \
    for (int i = 0; i < depth; i++) { \
        printf("  ");                 \
    }

/** Pretty print a JSON-like (but not compatible!) version of a node */
void Node_Print(Node *n, int depth) {
    if (n == NULL) {
        printf("null");
        return;
    }
    switch (n->type) {
        case N_NULL:    // stop the compiler from complaining
            break;
        case N_ARRAY: {
            printf("[\n");
            for (int i = 0; i < n->value.arrval.len; i++) {
                __node_indent(depth + 1);
                Node_Print(n->value.arrval.entries[i], depth + 1);
                if (i < n->value.arrval.len - 1) printf(",");
                printf("\n");
            }
            __node_indent(depth);
            printf("]");
        } break;

        case N_DICT: {
            printf("{\n");
            for (int i = 0; i < n->value.dictval.len; i++) {
                __node_indent(depth + 1);
                Node_Print(n->value.dictval.entries[i], depth + 1);
                if (i < n->value.dictval.len - 1) printf(",");
                printf("\n");
            }
            __node_indent(depth);
            printf("}");
        } break;
        case N_BOOLEAN:
            printf("%s", n->value.boolval ? "true" : "false");
            break;
        case N_NUMBER:
            printf("%f", n->value.numval);
            break;
        case N_INTEGER:
            printf("%ld", n->value.intval);
            break;
        case N_KEYVAL: {
            printf("\"%s\": ", n->value.kvval.key);
            Node_Print(n->value.kvval.val, depth);
        } break;
        case N_STRING:
            printf("\"%.*s\"", n->value.strval.len, n->value.strval.data);
    }
}

// serializer stack
typedef struct {
    int level;  // current level
    int pos;    // 0-based level
    Vector *nodes;
    Vector *indices;

} NodeSerializerStack;

// serializer stack push
static inline void _serializerPush(NodeSerializerStack *s, const Node *n) {
    s->level++;
    Vector_Push(s->nodes, n);
    Vector_Push(s->indices, 0);
}

// serializer stack push
static inline void _serializerPop(NodeSerializerStack *s) {
    s->level--;
    Vector_Pop(s->nodes, NULL);
    Vector_Pop(s->indices, NULL);
}

#define _maskenabled(n, x) ((int)(n ? n->type : N_NULL) & x)

// serialzer states
typedef enum {
    S_INIT,
    S_BEGIN_VALUE,
    S_CONT_VALUE,
    S_END_VALUE,
    S_CONTAINER,
    S_END
} NodeSerializerState;

void Node_Serializer(const Node *n, const NodeSerializerOpt *o, void *ctx) {
    Node *curr_node;
    int curr_len;
    int curr_index;
    Node **curr_entries;
    NodeSerializerStack stack = {0};
    NodeSerializerState state = S_INIT;

    // ===
    while (S_END != state) {
        switch (state) {
            case S_INIT:  // initial state
                stack.nodes = NewVector(Node *, 1);
                stack.indices = NewVector(int, 1);
                _serializerPush(&stack, n);
                state = S_BEGIN_VALUE;
                break;
            case S_BEGIN_VALUE:  // begining of a new value
                Vector_Get(stack.nodes, stack.level - 1, &curr_node);
                if (_maskenabled(curr_node, o->xBegin)) o->fBegin(curr_node, ctx);
                // NULL nodes have no type so they need special care
                state = curr_node ? S_CONT_VALUE : S_END_VALUE;
                break;
            case S_CONT_VALUE:  // container values
                if (N_DICT == curr_node->type) {
                    curr_len = curr_node->value.dictval.len;
                    curr_entries = curr_node->value.dictval.entries;
                    state = S_CONTAINER;
                } else if (N_ARRAY == curr_node->type) {
                    curr_len = curr_node->value.arrval.len;
                    curr_entries = curr_node->value.arrval.entries;
                    state = S_CONTAINER;
                } else if (N_KEYVAL == curr_node->type) {
                    curr_len = 1;
                    curr_entries = &curr_node->value.kvval.val;
                    state = S_CONTAINER;
                } else {
                    state = S_END_VALUE;  // must be non-container
                }
                break;
            case S_CONTAINER:  // go over container's contents
                Vector_Get(stack.indices, stack.level - 1, &curr_index);
                if (curr_index < curr_len) {
                    if (curr_index && _maskenabled(curr_node, o->xDelim)) o->fDelim(ctx);
                    Vector_Put(stack.indices, stack.level - 1, curr_index + 1);
                    _serializerPush(&stack, curr_entries[curr_index]);
                    state = S_BEGIN_VALUE;
                } else {
                    state = S_END_VALUE;
                }
                break;
            case S_END_VALUE:  // finished with the current value
                if (_maskenabled(curr_node, o->xEnd)) o->fEnd(curr_node, ctx);
                _serializerPop(&stack);
                if (stack.level) {  // if the value belongs to a container, go back to the container
                    Vector_Get(stack.nodes, stack.level - 1, &curr_node);
                    state = S_CONT_VALUE;
                } else {
                    state = S_END;  // otherwise we're done serializing
                }
                break;
            case S_END:  // keeps the compiler from compaining
                break;
        }  // switch(state)
    }
    Vector_Free(stack.nodes);
    Vector_Free(stack.indices);
}