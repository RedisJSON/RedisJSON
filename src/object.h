#ifndef __OBJECT_H__
#define __OBJECT_H__

#include <stdlib.h>

#define OBJ_OK 0
#define OBJ_ERR 1

typedef enum {
    N_STRING,
    N_NUMBER,
    N_BOOLEAN,
    N_OBJECT,
    N_ARRAY,
    N_KEYVAL,
} NodeType;

struct t_node;

typedef struct {
    const char *data;
    u_int32_t len;
} t_string;

typedef struct {
    struct t_node **nodes;
    u_int32_t len;
    u_int32_t cap;
} t_array;

typedef struct {
    const char *key;
    struct t_node *val;
} t_keyval;

typedef struct {
    struct t_node **entries;
    u_int32_t len;
    u_int32_t cap;
} t_object;

typedef struct t_node {
    union {
        int boolval;
        double numval;
        int64_t intval;
        t_string strval;
        t_array arrval;
        t_object object;
        t_keyval kv;
    } value;
    NodeType type;
} Node;

typedef struct {
    Node *root;
} Object;

Node *NewBoolNode(int val);
Node *NewNumberNode(double val);
Node *NewNumberNodeInt(int64_t val);
Node *NewStringNode(const char *s, u_int32_t len);
Node *NewArrayNode(u_int32_t len, u_int32_t cap);
Node *NewObjectNode(u_int32_t cap);

void Node_Free(Node *n);

void Node_Print(Node *n, int depth);
int Node_ArrayAppend(Node *arr, Node *n);
int Node_ArraySet(Node *arr, int index, Node *n);
int Node_ArrayItem(Node *arr, int index, Node **n);

int Node_ObjSet(Node *obj, const char *key, Node *n);
int Node_ObjDel(Node *objm, const char *key);
int Node_ObjGet(Node *obj, const char *key, Node **val);


typedef void (*NodeVisitor)(Node *, void *);
void __objTraverse(Node *n, NodeVisitor f, void *ctx);
void __arrTraverse(Node *n, NodeVisitor f, void *ctx);

void Node_Traverse(Node *n, NodeVisitor f, void *ctx);

#endif