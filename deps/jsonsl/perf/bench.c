#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <time.h>
#include <jsonsl.h>

int main(int argc, char **argv)
{
    struct stat sb;
    char *buf;
    FILE *fh;
    jsonsl_t jsn;
    int rv, itermax, ii;
    int is_rawscan = 0;
    time_t begin_time;
    size_t total_size;
    unsigned long duration;
    unsigned stuff = 0;

    if (argc < 3) {
        fprintf(stderr, "%s: FILE ITERATIONS [MODE]\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    if (argc > 3) {
        if (strcmp("raw", argv[3]) == 0) {
            is_rawscan = 1;
        }
    }

    sscanf(argv[2], "%d", &itermax);
    rv = stat(argv[1], &sb);
    if (rv != 0) {
        perror(argv[1]);
        exit(EXIT_FAILURE);
    }

    fh = fopen(argv[1], "rb");
    if (fh == NULL) {
        perror(argv[1]);
        exit(EXIT_FAILURE);
    }
    buf = malloc(sb.st_size + 1);
    fread(buf, 1, sb.st_size, fh);
    buf[sb.st_size] = '\0';
    begin_time = time(NULL);

    jsn = jsonsl_new(512);

    if (is_rawscan) {
        for (ii = 0; ii < itermax; ii++) {
            unsigned jj;
            for (jj = 0; jj < sb.st_size; jj++) {
                if (buf[jj] == '"') {
                    stuff++;
                }
            }
        }
    } else {
        for (ii = 0; ii < itermax; ii++) {
            jsonsl_reset(jsn);
            jsonsl_feed(jsn, buf, sb.st_size);
        }
    }

    total_size = sb.st_size * itermax;
    total_size /= (1024*1024);
    duration = time(NULL) - begin_time;
    if (!duration) {
        duration = 1;
    }
    if (stuff) {
        fprintf(stderr, "Random value (don't optimize out!): %u\n", stuff);
    }
    fprintf(stderr, "SPEED: %lu MB/sec\n", total_size/duration);

    jsonsl_dump_global_metrics();
    return 0;
}
