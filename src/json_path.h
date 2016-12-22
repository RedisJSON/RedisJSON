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

#ifndef __JSON_PATH_H__
#define __JSON_PATH_H__

#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include "path.h"

#ifdef REDIS_MODULE_TARGET
#include <alloc.h>
#endif

#define PARSE_OK 0
#define PARSE_ERR 1

// token type identifier
typedef enum {
    T_KEY,
    T_INDEX,
} tokenType;

// tokenizer state
typedef enum {
    S_NULL,         // start state
    S_IDENT,        // an identifier
    S_NUMBER,       // an index number
    S_DKEY,         // a key in double quotes
    S_SKEY,         // a key in single quotes
    S_BRACKET,      // subscript (could be a key or an index)
    S_DOT,          // child separator
    S_MINUS,        // a negative index
} tokenizerState;

// the token we're now on
typedef struct {
    tokenType type;
    char *s;
    size_t len;
} token;

int _tokenizePath(const char *json, size_t len, SearchPath *path);

/**
* Parse a JSON path expression into the initialized search path object.
* A search path is a simple JSON-like object hierarchy, e.g.:
*   foo.bar.baz[3]
*   foo["bar"]["baz"][3]
*   foo[3]
*
*   Note: string keys right now need to be ascii, we do not support unicode keys
*/
int ParseJSONPath(const char *json, size_t len, SearchPath *path);

#endif