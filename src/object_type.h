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

#ifndef __OBJECT_TYPE_H__
#define __OBJECT_TYPE_H__

#include <string.h>
#include <vector.h>
#include "object.h"
#include "redismodule.h"

/* Custom Redis data type API. */
void *ObjectTypeRdbLoad(RedisModuleIO *rdb);
void ObjectTypeRdbSave(RedisModuleIO *rdb, void *value);
void ObjectTypeFree(void *value);

/* Replies with a RESP representation of the node. */
void ObjectTypeToRespReply(RedisModuleCtx *ctx, Node *node);

#endif