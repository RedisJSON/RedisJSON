#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#include "all-tests.h"
#include "cliopts.h"

static int WantQuiet = 0;
static int WantFail = 0;
const char *mode = "all";
const char *json_files = NULL;
const char *file_list = NULL;
const char **JsonFileList = NULL;

enum {
    TEST_TYPE_JSON = 0x1,
    TEST_TYPE_JPR = 0x2,
    TEST_TYPE_UNESCAPE = 0x4,
};

#define TEST_TYPE_ALL (TEST_TYPE_JSON|TEST_TYPE_JPR|TEST_TYPE_UNESCAPE)

cliopts_entry entries[] = {
        { 'q', "quiet", CLIOPTS_ARGT_NONE,
                &WantQuiet,
                "Whether to not output verbose test information"
        },
        { 'F', "fail", CLIOPTS_ARGT_NONE,
                &WantFail,
                "For JSON tests, whether the parser is expected to return an "
                "error when parsing the inputs"
        },
        { 'm', "mode", CLIOPTS_ARGT_STRING,
                &mode,
                "Mode to test, can be 'all', 'jpr' 'json', or 'unescape'"
        },
        { 'f', "file", CLIOPTS_ARGT_STRING,
                &json_files,
                "Path to a single file for the 'json' test"
        },
        { 0, "file-list", CLIOPTS_ARGT_STRING,
                &file_list,
                "Path to a list of files to pass to the 'json' test"
        },
        { 0 }
};

static int build_list(void)
{
    int curalloc = 0;
    int curpos = 0;
    FILE *fp;
    char rbuf[512];

    if (json_files) {
        JsonFileList = malloc(sizeof(char**), 2);
        JsonFileList[1] = NULL;
        JsonFileList[0] = json_files;
        return 0;
    }

    if (!file_list) {
        fprintf(stderr, "Must have file or file list for JPR tests\n");
        return -1;
    }

    fp = fopen(file_list, "r");
    if (!fp) {
        perror(file_list);
        return -1;
    }

    while (fgets(rbuf, sizeof(rbuf), fp)) {
        char *curbuf;
        int slen = strlen(rbuf);
        curbuf = malloc(slen + 1);
        memcpy(curbuf, rbuf, slen);
        curbuf[slen-1] = '\0';

        if (curpos == curalloc) {
            if (!curalloc) {
                curalloc = 32;
            } else {
                curalloc *=  2;
            }
            JsonFileList = realloc(JsonFileList,
                                   (curalloc + 1) * sizeof(char**));
        }
        JsonFileList[curpos++] = curbuf;
    }
    fclose(fp);
    if (!curpos) {
        fprintf(stderr, "No files in list\n");
        return -1;
    }
    JsonFileList[curpos] = NULL;
    return 0;
}

int main(int argc, char **argv)
{
    int last_opt = 0;
    int test_mode = 0;
    cliopts_parse_options(entries, argc, argv, &last_opt, NULL);

    if (strcmp(mode, "all") == 0) {
        test_mode = TEST_TYPE_ALL;

    } else if (strcmp(mode, "json") == 0) {
        test_mode = TEST_TYPE_JSON;

    } else if (strcmp(mode, "jpr") == 0) {
        test_mode = TEST_TYPE_JPR;

    } else if (strcmp(mode, "unescape") == 0) {
        test_mode = TEST_TYPE_UNESCAPE;

    } else {
        fprintf(stderr, "Unrecognized mode '%s'\n", mode);
        exit(EXIT_FAILURE);
    }

    if (WantQuiet) {
        freopen(DEVNULL, "w", stdout);
    }
    if (WantFail) {
        setenv("JSONSL_FAIL_TESTS", "1", 1);
    }

    if (test_mode & TEST_TYPE_JSON) {
        char **curbuf;
        if (build_list() == -1) {
            return EXIT_FAILURE;
        }
        for (curbuf = JsonFileList; *curbuf; curbuf++) {
            if (jsonsl_test_json(2, curbuf-1) != 0) {
                return EXIT_FAILURE;
            }
        }
    }

    if (test_mode & TEST_TYPE_UNESCAPE) {
        if (jsonsl_test_unescape() != 0) {
            return EXIT_FAILURE;
        }
    }
    if (test_mode & TEST_TYPE_JPR) {

        if (jsonsl_test_jpr() != 0) {
            return EXIT_FAILURE;
        }
    }
    return 0;
}
