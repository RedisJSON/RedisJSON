/*
 * Copyright (c) 2007-2011, Lloyd Hilaiel <lloyd@hilaiel.com>
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <jsonsl.h>
#include "documents.h"

/* a platform specific defn' of a function to get a high res time in a
 * portable format */
#ifndef WIN32
#include <sys/time.h>
static double mygettime(void) {
    struct timeval now;
    gettimeofday(&now, NULL);
    return now.tv_sec + (now.tv_usec / 1000000.0);
}
#else
#define _WIN32 1
#include <windows.h>
static double mygettime(void) {
    long long tval;
	FILETIME ft;
	GetSystemTimeAsFileTime(&ft);
	tval = ft.dwHighDateTime;
	tval <<=32;
	tval |= ft.dwLowDateTime;
	return tval / 10000000.00;
}
#endif

#define PARSE_TIME_SECS 3

static int
error_callback(jsonsl_t jsn, jsonsl_error_t err, struct jsonsl_state_st *state,
        char *at)
{
    fprintf(stderr, "Got error %s at pos %lu (remaining: %s)\n",
            jsonsl_strerror(err), jsn->pos, at);
    abort();
}

static int
run(int validate_utf8)
{
    long long times = 0;
    double starttime;

    starttime = mygettime();
    jsonsl_t jsn = jsonsl_new(128);
    jsn->error_callback = error_callback;

    /* allocate a parser */
    for (;;) {
		int i;
        {
            double now = mygettime();
            if (now - starttime >= PARSE_TIME_SECS) break;
        }
        for (i = 0; i < 100; i++) {
            const char ** d;
            jsonsl_reset(jsn);
            for (d = get_doc(times % num_docs()); *d; d++) {
                jsonsl_feed(jsn, (char *) *d, strlen(*d));
            }
            times++;
        }
    }
    jsonsl_destroy(jsn);

    /* parsed doc 'times' times */
    {
        double throughput;
        double now;
        const char * all_units[] = { "B/s", "KB/s", "MB/s", (char *) 0 };
        const char ** units = all_units;
        int i, avg_doc_size = 0;

        now = mygettime();

        for (i = 0; i < num_docs(); i++) avg_doc_size += doc_size(i);
        avg_doc_size /= num_docs();

        throughput = (times * avg_doc_size) / (now - starttime);

        while (*(units + 1) && throughput > 1024) {
            throughput /= 1024;
            units++;
        }

        printf("Parsing speed: %g %s\n", throughput, *units);
    }

    return 0;
}

int
main(void)
{
    int rv = 0;

    printf("-- speed tests determine parsing throughput given %d different sample documents --\n",
           num_docs());

    printf("Without UTF8 validation:\n");
    rv = run(0);
    jsonsl_dump_global_metrics();
    return rv;
}

