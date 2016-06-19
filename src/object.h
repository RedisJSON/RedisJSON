#ifndef __OBJECT_H__
#define __OBJECT_H__

#include <stdlib.h>

typedef enum {
    N_STRING,
    N_NUMBER,
    N_BOOLEAN,
    N_OBJECT,
    N_ARRAY,
    N_NULL
} NodeType;

struct t_node;

typedef struct {
    char *data;
    size_t len;
} t_string;

typedef struct {
    t_node **nodes;
    size_t len;
    size_t cap;
} t_array;

typedef struct {
    char *key;
    t_node *val;
} t_objectEntry;

typedef struct {
    t_objectEntry **entries;
    size_t len;
    size_t cap;
} t_object;

typedef struct t_node {
    union {
        int boolval;
        double numval;
        t_string strval;
        t_array arrval;
        t_object object;
    } value;
    NodeType type;
} Node;

typedef struct {
    Node *root;
} Object;

Node *NewBoolNode(int val);
Node *NewNumberNode(double val);
Node *NewNumberNodeInt(int64_t val);
Node *NewStringNode(const char *s, size_t len);
Node *NewArrayNode(size_t len, size_t cap);
Node *NewObjectNode(size_t cap);

void Node_Free(Node *n);

int Node_ArrayAppend(Node *n);
int Node_ArraySet(Node *arr, int index, Node *n);
int Node_ObjSet(Node *obj, const char *key, Node *n);
int Node_ObjDel(Node *objm, const char *key);


typedef void (*NodeVisitor)(Node *, void *);

void Node_Traverse(Node *n, NodeVisitor f, void *ctx);

#endif