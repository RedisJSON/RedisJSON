#include "json_path.h"
#include <string.h>

int _tokenizePath(const char *json, size_t len, SearchPath *path) {
    tokenizerState st = S_NULL;
    size_t offset = 0;
    char *pos = (char *)json;
    token tok;
    tok.s = pos;
    tok.len = 0;
    while (offset < len) {
        char c = *pos;
        switch (st) {

            // initial state
            case S_NULL: {
                switch (c) {
                    // . at the beginning means "root"
                    case '.':
                        tok.s++;
                        st = S_IDENT;
                        break;
                    // start of key/index specifier
                    case '[':
                        tok.s++;
                        st = S_BRACKET;
                        break;
                    default:
                        // only alpha allowed in the beginning
                        if (isalpha(c)) {
                            tok.len++;
                            st = S_IDENT;
                            break;
                        } 
                        
                        goto syntaxerror;
                }
            }
            break;

            // we're after a square bracket opening
            case S_BRACKET: // [
                // quote after brackets means dict key
                if (c == '"') {
                    // skip to the beginnning of the key
                    tok.s++;
                    st = S_KEY;
                // digit after bracket means numeric index
                } else if (isdigit(c)) {
                    tok.len++;
                    st = S_NUMBER;
                } else {
                    goto syntaxerror;
                }
                break;
                
            // we're after a dot
            case S_DOT:
                // start of ident token
                if (isalpha(c)) {
                    tok.len++;
                    st = S_IDENT;
                } else {
                    goto syntaxerror;
                }
                break;

            // we're within a number
            case S_NUMBER:
                if (isdigit(c)) {
                    tok.len++;
                    continue;
                }
                if (c == ']') {
                    st = S_NULL;
                    tok.type = T_INDEX;
                    pos++;
                    offset++;
                    goto tokenend;
                }
                goto syntaxerror;
            
            // we're within an ident string
            case S_IDENT:
                // end of ident
                if (c == '.' || c == '[') {
                    st = c == '.' ? S_DOT : S_BRACKET;
                    tok.type = T_KEY;
                    pos++;
                    offset++;
                    goto tokenend;

                }
                // we only allow letters, numbers and underscores in identifiers
                if (!isalnum(c) && c != '_') {
                    goto syntaxerror;
                }
                // advance one
                tok.len++;
                break;
            
            // we're withing a bracketed string key 
            case S_KEY:
                // end of key
                if ( c == '"') {
                    if (offset < len - 1 && *(pos+1) == ']') {
                        tok.type = T_KEY;
                        pos+=2;
                        offset+=2;
                        st = S_NULL;
                        goto tokenend;
                    } else {
                        goto syntaxerror;
                    }
                }
                tok.len++;
                break;
        }
        offset++;
        pos++;
        continue;
tokenend: {
        
            printf("token: %.*s\n", tok.len, tok.s);
            if (tok.type == T_INDEX) {
                // convert the string to int. we can't use atoi because it expects NULL
                // termintated strings
                int64_t num = 0;
                for (int i = 0; i < tok.len; i++) {
                    int digit = tok.s[i] - '0';
                    num = num*10 + digit; 
                }
                
                SearchPath_AppendIndex(path, num);
            } else if (tok.type == T_KEY) {
                SearchPath_AppendKey(path, strndup(tok.s, tok.len));
            }
            tok.s = pos;
            tok.len = 0;
        }
    }

    // these are the only legal states at the end of consuming the string
    if (st == S_NULL || st == S_IDENT) {
        return OBJ_OK;
    }

syntaxerror:
    printf("syntax error at offset %zd ('%c')\n", offset, json[offset]);
    return OBJ_ERR;
    


}

int ParseJSONPath(const char *json, size_t len, SearchPath *path) {
    return _tokenizePath(json, len, path);
}
