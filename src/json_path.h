#ifndef __JSON_PATH_H__
#define __JSON_PATH_H__
#include <stdio.h>
#include <ctype.h>
#include "path.h"

#define PARSE_OK 0
#define PARSE_ERR 1

// token type identifier
typedef enum {
    T_KEY,
    T_INDEX,
} tokenType;

// tokenizer state
typedef enum {
    S_NULL,
    S_IDENT,
    S_NUMBER,
    S_KEY,
    S_BRACKET,
    S_DOT,
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