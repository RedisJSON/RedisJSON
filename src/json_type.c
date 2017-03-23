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

#include "json_type.h"

void *JSONTypeRdbLoad(RedisModuleIO *rdb, int encver) {
    if (encver < 0 || encver > JSONTYPE_ENCODING_VERSION) {
        RedisModule_LogIOError(
            rdb, RM_LOGLEVEL_WARNING,
            "Can't load JSON from RDB due to unknown encoding version %d, expecting %d at most",
            encver, JSONTYPE_ENCODING_VERSION);
        return NULL;
    }

    JSONType_t *jt = calloc(1, sizeof(JSONType_t));
    jt->root = ObjectTypeRdbLoad(rdb);
    return jt;
}

void JSONTypeRdbSave(RedisModuleIO *rdb, void *value) {
    JSONType_t *jt = (JSONType_t *)value;
    ObjectTypeRdbSave(rdb, jt->root);    
}

void JSONTypeAofRewrite(RedisModuleIO *aof, RedisModuleString *key, void *value) {
    // two approaches:
    // 1. For small documents it makes more sense to serialze the entire document in one go
    // 2. Large documents need to be broken to smaller pieces in order to stay within 0.5GB, but
    // we'll need some meta data to make sane-sized chunks so this gets lower priority atm
    JSONType_t *jt = (JSONType_t *)value;

    // serialize it
    JSONSerializeOpt jsopt = {.indentstr = "", .newlinestr = "", .spacestr = ""};
    sds json = sdsnewlen("\"", 1);
    SerializeNodeToJSON(jt->root, &jsopt, &json);
    json = sdscatlen(json, "\"", 1);
    RedisModule_EmitAOF(aof, "JSON.SET", "scb", key, OBJECT_ROOT_PATH, json, sdslen(json));
    sdsfree(json);
}

void JSONTypeFree(void *value) {
    JSONType_t *jt = (JSONType_t *)value;
    if (jt) {
        Node_Free(jt->root);
        free(jt);
    }
}

size_t JSONTypeMemoryUsage(const void *value) {
    const JSONType_t *jt = (JSONType_t *)value;
    size_t memory = sizeof(JSONType_t);

    memory += ObjectTypeMemoryUsage(jt->root);
    return memory;
}
