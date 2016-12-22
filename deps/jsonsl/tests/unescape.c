#undef NDEBUG
#include <stdio.h>
#include <stdlib.h>
#include <jsonsl.h>
#include <assert.h>
#include <string.h>
#include "all-tests.h"

static size_t res;
static jsonsl_error_t err;
static char *out;
static int strtable[0xff] = { 0 };
const char *escaped;

/**
 * Check a single octet escape of four hex digits
 */
void test_single_uescape(void)
{
    escaped = "\\u002B";
    strtable['u'] = 1;
    out = malloc(strlen(escaped)+1);
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    assert(res == 1);
    assert(out[0] == 0x2b);
    free(out);
}

/**
 * Test that we handle the null escape correctly (or that we do it right)
 */
void test_null_escape(void)
{
    escaped = "\\u0000";
    strtable['u'] = 1;
    out = malloc(strlen(escaped)+1);
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    assert(res == 0);
    free(out);
}

/**
 * Test multiple sequences of escapes.
 */
void test_multibyte_escape(void)
{
    unsigned flags;
    const char *exp = "\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d";
    escaped = "\\u05e9\\u05dc\\u05d5\\u05dd";
    strtable['u'] = 1;
    out = malloc(strlen(escaped) + 1);
    res = jsonsl_util_unescape_ex(escaped, out, strlen(escaped), strtable,
                                  &flags, &err, NULL);
    assert(res != 0);
    assert(res == strlen(exp));
    assert(memcmp(exp, out, strlen(exp)) == 0);
    assert(flags & JSONSL_SPECIALf_NONASCII);
    free(out);
}

/**
 * Check that things we don't want being unescaped are not unescaped
 */
void test_ignore_escape(void)
{
    escaped = "Some \\nWeird String";
    out = malloc(strlen(escaped)+1);
    strtable['W'] = 0;
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    out[res] = '\0';
    assert(res == strlen(escaped));
    assert(strncmp(escaped, out, res) == 0);

    escaped = "\\tA String";
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    out[res] = '\0';
    assert(res == strlen(escaped));
    assert(strncmp(escaped, out, res) == 0);
    free(out);
}

/**
 * Check that the built-in mappings for the 'sane' defaults work
 */
void test_replacement_escape(void)
{
    escaped = "This\\tIs\\tA\\tTab";
    out = malloc(strlen(escaped)+1);
    strtable['t'] = 1;
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    assert(res > 0);
    out[res] = '\0';
    assert(out[4] == '\t');
    assert(strcmp(out, "This\tIs\tA\tTab") == 0);
    free(out);
}

void test_invalid_escape(void)
{
    escaped = "\\invalid \\escape";
    out = malloc(strlen(escaped)+1);
    res = jsonsl_util_unescape(escaped, out, strlen(escaped), strtable, &err);
    assert(res == 0);
    assert(err == JSONSL_ERROR_ESCAPE_INVALID);
    free(out);
}

void test_unicode_escape(void)
{
    const char *exp = "\xe2\x82\xac";
    char out_s[64] = { 0 };

    escaped = "\\u20AC";
    strtable['u'] = 1;
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(err == JSONSL_ERROR_SUCCESS);
    assert(res == 3);
    assert(0 == memcmp(exp, out_s, 3));

    escaped = "\\u20ACHello";
    exp = "\xe2\x82\xacHello";
    memset(out_s, 0, sizeof out_s);
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(res == strlen(exp));
    assert(0 == memcmp(exp, out_s, strlen(exp)));

    escaped = "\\u0000";
    memset(out_s, 0, sizeof out_s);
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(err == JSONSL_ERROR_INVALID_CODEPOINT);

    /* Try with a surrogate pair */
    escaped = "\\uD834\\uDD1E";
    exp = "\xf0\x9d\x84\x9e";
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(res == 4);
    assert(0 == memcmp(exp, out_s, 4));

    /* Try with an incomplete surrogate */
    res = jsonsl_util_unescape_ex(escaped, out_s, 6, strtable, NULL, &err, NULL);
    assert(res == 0);
    assert(err == JSONSL_ERROR_INVALID_CODEPOINT);

    /* Try with an invalid pair */
    escaped = "\\uD834\\u0020";
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(res == 0);
    assert(err == JSONSL_ERROR_INVALID_CODEPOINT);

    /* Try with invalid hex */
    escaped = "\\uTTTT";
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(res == 0);
    assert(err == JSONSL_ERROR_PERCENT_BADHEX);

    escaped = "\\uaaa";
    res = jsonsl_util_unescape(escaped, out_s, strlen(escaped), strtable, &err);
    assert(res == 0);
    assert(err == JSONSL_ERROR_UESCAPE_TOOSHORT);

    /* ASCII Escapes */
    exp = "simple space";
    escaped = "simple\\u0020space";
    res = jsonsl_util_unescape_ex(
            escaped, out_s, strlen(escaped), strtable, NULL, &err, NULL);
    assert(res == strlen(exp));
    assert(err == JSONSL_ERROR_SUCCESS);
    assert(memcmp(exp, out_s, res) == 0);
}

JSONSL_TEST_UNESCAPE_FUNC
{
    test_single_uescape();
    test_null_escape();
    test_ignore_escape();
    test_replacement_escape();
    test_invalid_escape();
    test_multibyte_escape();
    test_unicode_escape();
    return 0;
}
