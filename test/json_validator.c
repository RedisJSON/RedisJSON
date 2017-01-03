#include <stdio.h>
#include "../src/json_object.h"

int main(int argc, char **argv) {
 
    if (argc != 2) {
        printf("usage: %s filename\n", argv[0]);
        exit(1);
    }

    FILE *f;
    long len;
    char *json;

    f = fopen(argv[1], "rb");
    fseek(f, 0, SEEK_END);
    len = ftell(f);
    fseek(f, 0, SEEK_SET);
    json = (char *)malloc(len + 1);
    fread(json, 1, len, f);
    json[len] = '\0';
    fclose(f);

    Node *n = NULL;
    char *err = NULL;
    int ret = CreateNodeFromJSON(json, len, &n, &err);

    if (ret || err) {
        ret = 1;
        printf("-%s\n", err ? err : "ERR unknown");
    } else {
        printf("+OK\n");
    }
    if (err) free(err);
    if (n) Node_Free(n);
    free(json);
    return ret;

}
