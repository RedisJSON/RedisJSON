/*
* Copyright (C) 2016-2017 Redis Labs
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
#include <stddef.h>
#include <string.h>
#include "redismodule.h"
#include "rmstrndup.h"

/* A patched implementation of strdup that will use our patched calloc */
char *rmstrndup(const char *s, size_t n) {
  char *ret = RedisModule_Calloc(n + 1, sizeof(char));
  if (ret)
    memcpy(ret, s, n);
  return ret;
}