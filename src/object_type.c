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

#include "object_type.h"

#ifndef REDIS_MODULE_TARGET
#pragma GCC error "object_type must be compiled as a Redis module"
#endif

#define Vector_Last(v) Vector_Size(v) - 1

void *ObjectTypeRdbLoad(RedisModuleIO *rdb) {
    // IMPORTANT: no encoding version check here, this is up to the calller
    Vector *nodes;
    Vector *indices;
    Node *node;
    uint64_t len;
    uint64_t type;
    size_t strlen;
    char *str;
    enum { S_INIT, S_BEGIN_VALUE, S_END_VALUE, S_CONTAINER, S_END } state = S_INIT;

    while (S_END != state) {
        switch (state) {
            case S_INIT:  // Initial state
                nodes = NewVector(Node *, 1);
                indices = NewVector(uint64_t, 1);
                type = RedisModule_LoadUnsigned(rdb);
                state = S_BEGIN_VALUE;
                break;
            case S_BEGIN_VALUE:
                switch (type) {
                    case N_NULL:
                        node = NULL;
                        state = S_END_VALUE;
                        break;
                    case N_BOOLEAN:
                        str = RedisModule_LoadStringBuffer(rdb, &strlen);
                        node = NewBoolNode('1' == str[0]);
                        state = S_END_VALUE;
                        break;
                    case N_INTEGER:
                        node = NewIntNode(RedisModule_LoadSigned(rdb));
                        state = S_END_VALUE;
                        break;
                    case N_NUMBER:
                        node = NewDoubleNode(RedisModule_LoadDouble(rdb));
                        state = S_END_VALUE;
                        break;
                    case N_STRING:
                        str = RedisModule_LoadStringBuffer(rdb, &strlen);
                        node = NewStringNode(str, strlen);
                        state = S_END_VALUE;
                        break;
                    case N_KEYVAL:
                        str = RedisModule_LoadStringBuffer(rdb, &strlen);
                        Vector_Push(nodes, NewKeyValNode(str, strlen, NULL));
                        Vector_Push(indices, (uint64_t)1);
                        state = S_CONTAINER;
                        break;
                    case N_DICT:
                        len = RedisModule_LoadUnsigned(rdb);
                        Vector_Push(nodes, NewDictNode(len));
                        Vector_Push(indices, len);
                        state = S_CONTAINER;
                        break;
                    case N_ARRAY:
                        len = RedisModule_LoadUnsigned(rdb);
                        Vector_Push(nodes, NewArrayNode(len));
                        Vector_Push(indices, len);
                        state = S_CONTAINER;
                        break;
                }  // switch (type)
                break;
            case S_END_VALUE:
                if (Vector_Size(nodes)) {  // in case the new node has a parent
                    Node *container;
                    Vector_Get(nodes, Vector_Last(nodes), &container);
                    switch (container->type) {  // add it
                        case N_KEYVAL:
                            container->value.kvval.val = node;
                            break;
                        case N_DICT:
                            Node_DictSetKeyVal(container, node);
                            break;
                        case N_ARRAY:
                            Node_ArrayAppend(container, node);
                        default:
                            break;
                    }
                    state = S_CONTAINER;
                } else {
                    state = S_END;
                }
                break;
            case S_CONTAINER:
                Vector_Get(indices, Vector_Last(indices), &len);
                if (len) {  // move to next child node
                    Vector_Put(indices, Vector_Last(indices), len - 1);
                    type = RedisModule_LoadUnsigned(rdb);
                    state = S_BEGIN_VALUE;
                } else {
                    Vector_Pop(indices, NULL);
                    Vector_Pop(nodes, &node);
                    state = S_END_VALUE;
                }
                break;
            case S_END:  // keeps the compiler from complaining
                break;
        }  // switch (state)
    }      //    while (S_END != state)

    Vector_Free(indices);
    Vector_Free(nodes);
    return (void *)node;
}

void _ObjectTypeSave_Begin(Node *n, void *ctx) {
    RedisModuleIO *rdb = (RedisModuleIO *)ctx;

    // type is saved as uint64, but could be compressed to 1-2 bytes.
    if (!n) {
        RedisModule_SaveUnsigned(rdb, N_NULL);
    } else {
        RedisModule_SaveUnsigned(rdb, n->type);
        switch (n->type) {
            case N_BOOLEAN:
                RedisModule_SaveStringBuffer(rdb, n->value.boolval ? "1" : "0", 1);
                break;
            case N_INTEGER:
                RedisModule_SaveSigned(rdb, n->value.intval);
                break;
            case N_NUMBER:
                RedisModule_SaveDouble(rdb, n->value.numval);
                break;
            case N_STRING:
                RedisModule_SaveStringBuffer(rdb, n->value.strval.data, n->value.strval.len);
                break;
            case N_KEYVAL:
                RedisModule_SaveStringBuffer(rdb, n->value.kvval.key, strlen(n->value.kvval.key));
                break;
            case N_DICT:
                RedisModule_SaveUnsigned(rdb, n->value.dictval.len);
                break;
            case N_ARRAY:
                RedisModule_SaveUnsigned(rdb, n->value.arrval.len);
                break;
            case N_NULL:  // keeps the compiler from complaining
                break;
        }
    }
}

void ObjectTypeRdbSave(RedisModuleIO *rdb, void *value) {
    Node *node = (Node *)value;
    NodeSerializerOpt nso = {0};

    nso.fBegin = _ObjectTypeSave_Begin;
    nso.xBegin = 0xff;  // mask for all basic types
    Node_Serializer(node, &nso, rdb);
}

void ObjectTypeFree(void *value) {
    if (value) Node_Free(value);
}

void _ObjectTypeToResp_Begin(Node *n, void *ctx) {
    RedisModuleCtx *rctx = (RedisModuleCtx *)ctx;

    if (!n) {
        RedisModule_ReplyWithNull(ctx);
    } else {
        switch (n->type) {
            case N_BOOLEAN:
                RedisModule_ReplyWithSimpleString(ctx, n->value.boolval ? "true" : "false");
                break;
            case N_INTEGER:
                RedisModule_ReplyWithLongLong(ctx, n->value.intval);
                break;
            case N_NUMBER:
                RedisModule_ReplyWithDouble(ctx, n->value.numval);
                break;
            case N_STRING:
                RedisModule_ReplyWithStringBuffer(ctx, n->value.strval.data, n->value.strval.len);
                break;
            case N_KEYVAL:
                RedisModule_ReplyWithArray(ctx, 2);
                RedisModule_ReplyWithStringBuffer(ctx, n->value.kvval.key, strlen(n->value.kvval.key));
                break;
            case N_DICT:
                RedisModule_ReplyWithArray(ctx, n->value.dictval.len + 1);
                RedisModule_ReplyWithSimpleString(ctx, "{");
                break;
            case N_ARRAY:
                RedisModule_ReplyWithArray(ctx, n->value.arrval.len + 1);
                RedisModule_ReplyWithSimpleString(ctx, "[");
                break;
            case N_NULL:  // keeps the compiler from complaining
                break;
        }
    }
}

void ObjectTypeToRespReply(RedisModuleCtx *ctx, Node *node) {
    NodeSerializerOpt nso = {0};

    nso.fBegin = _ObjectTypeToResp_Begin;
    nso.xBegin = 0xff;  // mask for all basic types
    Node_Serializer(node, &nso, ctx);
}