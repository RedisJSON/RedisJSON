#include <sys/param.h>
#include <string.h>
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

Node *NewNumberNode(double val) {
  Node *ret = __newNode(N_NUMBER);
  ret->value.numval = val;
  return ret;
}

Node *NewNumberNodeInt(int64_t val) {
  Node *ret = __newNode(N_NUMBER);
  ret->value.numval = (double)val;
  return ret;
}

Node *NewStringNode(const char *s, u_int32_t len) {
  Node *ret = __newNode(N_STRING);
  ret->value.strval.data = s;
  ret->value.strval.len = len;
  return ret;
}

Node *NewArrayNode(u_int32_t len, u_int32_t cap) {
  Node *ret = __newNode(N_ARRAY);
  ret->value.arrval.cap = cap;
  ret->value.arrval.len = 0;
  ret->value.arrval.nodes = calloc(cap, sizeof(Node *));
  return ret;
}
Node *NewObjectNode(u_int32_t cap) {
  Node *ret = __newNode(N_OBJECT);
  ret->value.object.cap = cap;
  ret->value.object.len = 0;
  ret->value.object.entries = calloc(cap, sizeof(Node *));
  return ret;
}

void __node_FreeKV(Node *n) {
  Node_Free(n->value.kv.val);
  free((char *)n->value.kv.key);
  free(n);
}

void __node_FreeObj(Node *n) {

  for (int i = 0; i < n->value.object.len; i++) {
    Node_Free(n->value.object.entries[i]);
  }
  free(n);
}

void __node_FreeArr(Node *n) {
  for (int i = 0; i < n->value.arrval.len; i++) {
    Node_Free(n->value.arrval.nodes[i]);
  }
  free(n);
}
void __node_FreeString(Node *n) {
  free((char *)n->value.strval.data);
  free(n);
}

void Node_Free(Node *n) {
  switch (n->type) {
  case N_ARRAY:
    __node_FreeArr(n);
    break;
  case N_OBJECT:
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

int Node_ArrayAppend(Node *arr, Node *n) {

  t_array *a = &arr->value.arrval;
  if (a->len >= a->cap) {
    a->cap = MIN(a->cap * 2, 1024 * 1024);
    a->nodes = realloc(a->nodes, a->cap * sizeof(Node *));
  }
  a->nodes[a->len++] = n;
  return OBJ_OK;
}

int Node_ArraySet(Node *arr, int index, Node *n) {
  t_array *a = &arr->value.arrval;

  // invalid index!
  if (index < 0 || index >= a->len) {
    return OBJ_ERR;
  }
  a->nodes[index] = n;

  return OBJ_OK;
}

int Node_ArrayItem(Node *arr, int index, Node **n) {
  t_array *a = &arr->value.arrval;

  // invalid index!
  if (index < 0 || index >= a->len) {
    *n = NULL;
    return OBJ_ERR;
  }
  *n = a->nodes[index];
  return OBJ_OK;
}

Node *__obj_find(t_object *o, const char *key, int *idx) {

  for (int i = 0; i < o->len; i++) {
    if (!strcmp(key, o->entries[i]->value.kv.key)) {

      if (idx)
        *idx = i;

      return o->entries[i];
    }
  }

  return NULL;
}
int Node_ObjSet(Node *obj, const char *key, Node *n) {
  t_object *o = &obj->value.object;

  if (key == NULL)
    return OBJ_ERR;

  int idx;
  Node *kv = __obj_find(o, key, &idx);
  // first find a replacement possiblity
  if (kv) {
    if (kv->value.kv.val) {
      Node_Free(kv->value.kv.val);
    }
    kv->value.kv.val = n;
    return OBJ_OK;
  }

  // append another entry
  if (o->len >= o->cap) {
    o->cap = MIN(o->cap * 2, 1024 * 1024);
    o->entries = realloc(o->entries, o->cap * sizeof(t_keyval));
  }

  kv = __newNode(N_KEYVAL);
  kv->value.kv.key = strdup(key);
  kv->value.kv.val = n;

  return OBJ_OK;
}

int Node_ObjDel(Node *obj, const char *key) {
  if (key == NULL)
    return OBJ_ERR;

  t_object *o = &obj->value.object;

  int idx = -1;
  t_keyval *kv = __obj_find(o, key, &idx);

  // tried to delete a non existing node
  if (!kv)
    return OBJ_ERR;

  // let's delete the node's memory
  if (kv->val) {
    Node_Free(kv->val);
  }
  free((char *)kv->key);

  // replace the deleted entry and the top entry to avoid holes
  if (idx < o->len - 1) {
    o->entries[idx] = o->entries[o->len - 1];
  }
  o->len--;

  return OBJ_OK;
}

int Node_ObjGet(Node *obj, const char *key, Node **val) {
  if (key == NULL)
    return OBJ_ERR;

  t_object *o = &obj->value.object;

  int idx = -1;
  t_keyval *kv = __obj_find(o, key, &idx);

  // not found!
  if (!kv)
    return OBJ_ERR;

  *val = kv->val;
  return OBJ_OK;
}

void __objTraverse(Node *n, NodeVisitor f, void *ctx) {
  t_object *o = &n->value.object;

  f(n, ctx);
  for (int i = 0; i < o->len; i++) {
    Node_Traverse(o->entries[i], f, ctx);
  }
}
void __arrTraverse(Node *n, NodeVisitor f, void *ctx) {
  t_array *a = &n->value.arrval;
  f(n, ctx);

  for (int i = 0; i < a->len; i++) {
    Node_Traverse(a->nodes[i], f, ctx);
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
  case N_OBJECT:
    __objTraverse(n, f, ctx);
    break;
  // for all other types - just call the callback
  default:
    f(n, ctx);
  }

}