#ifndef CLIOPTS_H_
#define CLIOPTS_H_

#ifdef __cplusplus
extern "C" {
#endif /* __cplusplus */

/**
 * Various option types
 */
typedef enum {
    /** takes no argument, dest should be anything big enough to hold a boolean*/
    CLIOPTS_ARGT_NONE,

    /** simple int type, dest should be an 'int' */
    CLIOPTS_ARGT_INT,

    /** dest should be an unsigned int */
    CLIOPTS_ARGT_UINT,

    /** dest should be an unsigned int, but command line format is hex */
    CLIOPTS_ARGT_HEX,

    /** dest should be a char**. Note that the string is allocated, so you should
     * free() it when done */
    CLIOPTS_ARGT_STRING,

    /** dest should be a float* */
    CLIOPTS_ARGT_FLOAT
} cliopts_argtype_t;

typedef struct {
    /**
     * Input parameters
     */

    /** Short option, i.e. -v  (0 for none) */
    char kshort;

    /** long option, i.e. --verbose, NULL for none */
    const char *klong;

    /** type of value */
    cliopts_argtype_t ktype;

    /** destination pointer for value */
    void *dest;

    /** help string for this option */
    const char *help;

    /** description of the value, e.g. --file=FILE */
    const char *vdesc;


    /** set this to true if the user must provide this option */
    int required;


    /**
     * Output parameters
     */

    /** whether this option was encountered on the command line */
    int found;

} cliopts_entry;

struct cliopts_extra_settings {
    /** Assume actual arguments start from argv[0], not argv[1] */
    int argv_noskip;
    /** Don't exit on error */
    int error_noexit;
    /** Don't print help on error */
    int error_nohelp;
    /** Don't interpret --help or -? as help flags */
    int help_noflag;
    /** Program name (defaults to argv[0]) */
    const char *progname;
};

/**
 * Parse options.
 *
 * @param entries an array of cliopts_entry structures. The list should be
 * terminated with a structure which has its dest field set to NULL
 *
 * @param argc the count of arguments
 * @param argv the actual list of arguments
 * @param lastidx populated with the amount of elements from argv actually read
 * @params setting a structure defining extra settings for the argument parser.
 * May be NULL
 *
 * @return 0 for success, -1 on error.
 */
int
cliopts_parse_options(cliopts_entry *entries,
                      int argc,
                      char **argv,
                      int *lastidx,
                      struct cliopts_extra_settings *settings);


#ifdef __cplusplus
}
#endif /* __cplusplus */

#endif /* CLIOPTS_H_ */
