#ifndef JSONSL_ALLTESTS_H
#define JSONSL_ALLTESTS_H

/**
 * I made this file primarily for Windows, where I didn't want to make
 * 3 separate projects for each executable
 */

#ifdef JSONSL_SINGLE_TEST_EXE
#define JSONSL_TEST_UNESCAPE_FUNC int jsonsl_test_unescape(void)
#define JSONSL_TEST_JPR_FUNC int jsonsl_test_jpr(void)
#define JSONSL_TEST_JSON_FUNC int jsonsl_test_json(int argc, char **argv)

JSONSL_TEST_UNESCAPE_FUNC;
JSONSL_TEST_JPR_FUNC;
JSONSL_TEST_JSON_FUNC;

#else
#define JSONSL_TEST_UNESCAPE_FUNC int main(void)
#define JSONSL_TEST_JPR_FUNC int main(void)
#define JSONSL_TEST_JSON_FUNC int main(int argc, char **argv)

#endif /* SINGLE_TEST_EXE */

#ifdef _WIN32
#define S_ISDIR(x) ((x & _S_IFMT) == _S_IFDIR)
#define DEVNULL "nul"
#define setenv(k, v, o) _putenv_s(k, v)
#else
#define DEVNULL "/dev/null"
#endif

#endif /* JSONSL_ALLTESTS_H */
