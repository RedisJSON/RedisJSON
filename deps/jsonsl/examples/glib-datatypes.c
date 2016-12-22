#include <assert.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include "glib-datatypes.h"

static int MaxOutputLevel = 20;
static int MaxDescentLevel = 20;
static gboolean DumpIncremental = FALSE;
static gboolean DumpProgress = TRUE;
static gboolean DumpCompletion = TRUE;
static gboolean DumpStringContents = TRUE;
static gboolean DumpHashKeys = TRUE;
static gboolean DumpAll = FALSE;
static gboolean Silent = FALSE;

static void pad_level(int level) {
    for (; level > 1; level--) {
        putchar(' ');
    }
}

static void dump_hash(struct hash_st *hash, int level);
static void dump_list(struct list_st *list, int level);
static void dump_string(struct string_st *str, int level);


static void dump_element(struct element_st *root, int level)
{
    char *typestr = "UNKNOWN";
    if (!root) {
        return;
    }
    if (MaxOutputLevel < level) {
        return;
    }
#define X(t) \
    if (root->type == TYPE_##t) \
        typestr = #t;
    _XTYPE_ALL
#undef X

    pad_level(level);
    printf("<%s", typestr);

    switch(root->type) {
    case TYPE_STRING:
        dump_string((struct string_st*)root, level+1);
        break;
    case TYPE_LIST:
        dump_list((struct list_st*)root, level+1);
        break;
    case TYPE_HASH:
        dump_hash((struct hash_st*)root, level+1);
        break;
    default:
        fprintf(stderr, "Unknown type!\n");
        abort();
        break;
    }
    pad_level(level);
    printf("</%s>\n", typestr);
}

static void dump_string(struct string_st *str, int level)
{
    printf(" len=\"%lu\">", strlen(str->data));
    if (DumpStringContents) {
        printf("%s", str->data);
    }
    printf("\n");
}

static void dump_list(struct list_st *list, int level)
{
    GList *node;
    printf(">\n");
    for (node = list->data; node; node = node->next) {
        dump_element((struct element_st*)node->data, level);
    }
}

static void dump_hash_iterfunc(gpointer key, gpointer value, void *data)
{
    int level = *(int*)data;
    level++;
    printf("\n");
    pad_level(level);
    printf("<KEY len=\"%lu\"", strlen((char*)key+1));
    if (DumpHashKeys) {
        printf(" key=\"%s\"", (char*)key+1);
    }
    printf("/>\n");
    dump_element((struct element_st*)value, level+1);
}

static void dump_hash(struct hash_st *hash, int level)
{
    printf(">\n");
    g_hash_table_foreach(hash->data, dump_hash_iterfunc, &level);
}

static inline
void add_to_hash(struct hash_st *parent, struct element_st *value)
{
    assert(parent->pending_key);
    g_hash_table_insert(parent->data, (gpointer)parent->pending_key, value);
    parent->pending_key = NULL;
}

static inline
void add_to_list(struct list_st *parent, struct element_st *value)
{
    parent->data = g_list_append(parent->data, value);
}

static inline void
dump_action_state(jsonsl_t jsn,
                  jsonsl_action_t action,
                  struct jsonsl_state_st *state)
{
    int ii;
    size_t pos = (action == JSONSL_ACTION_POP) ? state->pos_cur : state->pos_begin;
    if (!DumpProgress) {
        return;
    }
    for (ii = 1; ii < state->level; ii++) {
        printf("   ");
    }
    printf("L%d %c%-10s @%lu\n",
           state->level,
           action,
           jsonsl_strtype(state->type),
           pos);
}

static void
create_new_element(jsonsl_t jsn,
                   jsonsl_action_t action,
                   struct jsonsl_state_st *state,
                   const char *buf)
{
    struct element_st *child = NULL, *parent = NULL;
    struct jsonsl_state_st *last_state = jsonsl_last_state(jsn, state);
    parent = (struct element_st*)last_state->data;

    dump_action_state(jsn, action, state);

    switch(state->type) {
    case JSONSL_T_SPECIAL:
    case JSONSL_T_STRING: {
        struct string_st *str = malloc(sizeof(*str));
        str->data = buf;
        str->type = TYPE_STRING;
        child = (struct element_st*)str;
        break;
    }
    case JSONSL_T_HKEY: {
        struct hash_st *hash = (struct hash_st*)parent;
        struct string_st *str = malloc(sizeof(*str));
        assert(hash->type == TYPE_HASH);
        hash->pending_key = buf;
        str->parent = NULL;
        str->data = buf;
        str->type = TYPE_STRING;
        state->data = (struct element_st*)str;
        return; /* nothing to do here */
    }
    case JSONSL_T_LIST: {
        struct list_st *list = malloc(sizeof(*list));
        list->type = TYPE_LIST;
        list->data = g_list_alloc();
        child = (struct element_st*)list;
        break;
    }
    case JSONSL_T_OBJECT: {
        struct hash_st *hash = malloc(sizeof(*hash));
        hash->type = TYPE_HASH;
        hash->data = g_hash_table_new(g_str_hash, g_str_equal);
        child = (struct element_st*)hash;
        break;
    }
    default:
        fprintf(stderr, "Unhandled type %c\n", state->type);
        abort();
        break;
    }

    if (parent->type == TYPE_LIST) {
        add_to_list((struct list_st*)parent, child);
    } else if (parent->type == TYPE_HASH) {
        add_to_hash((struct hash_st*)parent, child);
    } else {
        fprintf(stderr, "Requested to add to non-container parent type!\n");
        abort();
    }

    assert(child);
    state->data = child;
}

static void
cleanup_closing_element(jsonsl_t jsn,
                        jsonsl_action_t action,
                        struct jsonsl_state_st *state,
                        const char *at)
{
    /* termination of an element */

    struct element_st *elem = (struct element_st*)state->data;
    struct string_st *str = (struct string_st*)elem;
    assert(state);
    if (!DumpIncremental) {
        dump_action_state(jsn, action, state);
    }

    if (elem->type == TYPE_STRING) {
        if (*at != '"') {
            return;
        }

        *(char*)at = '\0';
        str->data++;
    }

    if (DumpIncremental) {
        struct objgraph_st *objgraph = jsn->data;
        printf("Incremental dump at input position %lu\n", jsn->pos);
        dump_element(objgraph->root, 0);
    }
}

void nest_callback_initial(jsonsl_t jsn,
                        jsonsl_action_t action,
                        struct jsonsl_state_st *state,
                        const char *at)
{
    struct objgraph_st *objgraph = (struct objgraph_st*)(jsn->data);
    struct element_st *elem;
    dump_action_state(jsn, action, state);

    assert(action == JSONSL_ACTION_PUSH);
    if (state->type == JSONSL_T_LIST) {
        struct list_st *list = malloc(sizeof(*list));
        list->data = g_list_alloc();
        list->type = TYPE_LIST;
        elem = (struct element_st*)list;
    } else if (state->type == JSONSL_T_OBJECT) {
        struct hash_st *hash = malloc(sizeof(*hash));
        hash->data = g_hash_table_new(g_str_hash, g_str_equal);
        hash->type = TYPE_HASH;
        hash->pending_key = NULL;
        elem = (struct element_st*)hash;
    } else {
        fprintf(stderr, "Type is neither hash nor list\n");
        abort();
    }

    elem->parent = NULL;
    objgraph->root = elem;
    state->data = elem;
    jsn->action_callback = NULL;
    jsn->action_callback_PUSH = create_new_element;
    jsn->action_callback_POP = cleanup_closing_element;
}

int error_callback(jsonsl_t jsn,
                    jsonsl_error_t err,
                    struct jsonsl_state_st *state,
                    char *at)
{
    fprintf(stderr, "Got error at pos %lu: %s\n",
            jsn->pos, jsonsl_strerror(err));
    printf("Remaining text: %s\n", at);
    abort();
    return 0;
}


static void parse_one_file(const char *path)
{
    int fd, status;
    struct stat sb;
    jsonsl_t jsn;
    struct objgraph_st graph;
    char *buf, *bufp;
    size_t nread;

    printf("==== %s ====\n", path);

    fd = open(path, O_RDONLY);
    if (fd == -1) {
        perror(path);
        return;
    }

    status = fstat(fd, &sb);
    assert(status == 0);
    assert(sb.st_size < 0x1000000);
    buf = malloc(sb.st_size);

    jsn = jsonsl_new(0x1000);
    jsonsl_enable_all_callbacks(jsn);

    jsn->action_callback = nest_callback_initial;
    jsn->action_callback_PUSH = NULL;
    jsn->action_callback_POP = NULL;
    jsn->error_callback = error_callback;
    jsn->data = &graph;
    jsn->max_callback_level = MaxDescentLevel;

    memset(&graph, 0, sizeof(graph));

    bufp = buf;
    while ( (nread = read(fd, bufp, 4096)) > 0) {
        jsonsl_feed(jsn, bufp, nread);
        bufp += nread;
        if (nread < 4096) {
            break;
        }
    }

    if (DumpCompletion) {
        dump_element(graph.root, 0);
    }
}

static GOptionEntry CLIOptions[] = {
{"dump-completion", 'C', 0, G_OPTION_ARG_NONE, &DumpCompletion, "Dump graph upon completion", NULL },
{"dump-incremental", 'i', 0, G_OPTION_ARG_NONE, &DumpIncremental, "Dump graph incrementally", NULL },
{"output-level", 'L', 0, G_OPTION_ARG_INT, &MaxOutputLevel, "Maximum output level", "LEVEL"},
{"descent-level", 'R', 0, G_OPTION_ARG_INT, &MaxDescentLevel, "Maximum recursion level for graph", "LEVEL" },
{"dump-strings", 's', 0, G_OPTION_ARG_NONE, &DumpStringContents, "Display string values", NULL },
{"dump-keys", 'k', 0,   G_OPTION_ARG_NONE, &DumpHashKeys, "Display dictionary keys", NULL },
{"dump-progresss", 'p', 0, G_OPTION_ARG_NONE, &DumpProgress, "Dump abbreviated progress (incrementally)", NULL },
{"verbose", 'v', 0,     G_OPTION_ARG_NONE, &DumpAll, "Be verbose. dump everything", NULL },
{"quiet", 'q', 0,       G_OPTION_ARG_NONE, &Silent, "Quiet, don't output anything", NULL },
{ NULL }
};

int main(int argc, char **argv) {
    int ii;
    GError *error = NULL;
    GOptionContext *context;

    context = g_option_context_new("FILES..");
    g_option_context_add_main_entries(context, CLIOptions, NULL);
    if (!g_option_context_parse(context, &argc, &argv, &error)) {
        g_print("Option parsing failed: %s\n", error->message);
        exit(EXIT_FAILURE);
    }

    {
        int set_all = -1;
        if (DumpAll) {
            set_all = 1;
        } else if (Silent) {
            set_all = 0;
        }
        if (set_all >= 0) {
            DumpIncremental = set_all;
            DumpCompletion = set_all;
            DumpStringContents = set_all;
            DumpHashKeys = set_all;
            DumpProgress = set_all;
        }
        if (DumpIncremental) {
            DumpProgress = FALSE;
        }
    }

    if (argc < 2) {
        fprintf(stderr, "USAGE: %s FILES...\n", argv[0]);
        exit(EXIT_FAILURE);
    }
    for (ii = 1; ii < argc && argv[ii]; ii++) {
        parse_one_file(argv[ii]);
    }
    return 0;
}
