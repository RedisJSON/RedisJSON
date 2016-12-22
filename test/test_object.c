#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/json_path.h"
#include "../src/object.h"
#include "../src/path.h"
#include "minunit.h"

MU_TEST(testNodeString) {
    // Test creation of an empty C string
    Node *n1 = NewCStringNode("");
    mu_check(NULL != n1);
    mu_assert_int_eq(0, Node_Length(n1));
    Node_Free(n1);

    // Test creation of an empty buffer string
    n1 = NewStringNode("", 0);
    mu_check(NULL != n1);
    mu_assert_int_eq(0, Node_Length(n1));
    Node_Free(n1);

    // Test appending strings
    n1 = NewCStringNode("foo");
    mu_check(NULL != n1);
    mu_assert_int_eq(3, Node_Length(n1));
    Node *n2 = NewStringNode("bar", 3);
    mu_check(NULL != n2);
    mu_assert_int_eq(3, Node_Length(n2));
    mu_assert_int_eq(OBJ_OK, Node_StringAppend(n1, n2));
    mu_check(NULL != n1);
    mu_assert_int_eq(6, Node_Length(n1));
    mu_check(!strncmp(n1->value.strval.data, "foobar", Node_Length(n1)));
}

MU_TEST(testNodeArray) {
    Node *arr, *arr2, *n;

    // Test creation of a typical empty array
    arr = NewArrayNode(0);
    mu_check(NULL != arr);
    mu_assert_int_eq(Node_Length(arr), 0);
    mu_check(OBJ_ERR == Node_ArrayItem(arr, 0, &n));

    // Test appending and getting a node
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(42)));
    mu_assert_int_eq(Node_Length(arr), 1);
    mu_check(OBJ_ERR == Node_ArrayItem(arr, 1, &n));
    mu_check(OBJ_OK == Node_ArrayItem(arr, 0, &n));
    mu_check(NULL != n);
    mu_check(N_INTEGER == n->type);
    mu_check(42 == n->value.intval);

    // Delete the element
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 0, 1));
    mu_check(OBJ_ERR == Node_ArrayItem(arr, 0, &n));
    mu_assert_int_eq(Node_Length(arr), 0);

    // Test with some more elements
    Node_Free(arr);
    arr = NewArrayNode(1);
    mu_assert_int_eq(Node_Length(arr), 0);
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewStringNode("foo", 3)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewStringNode("bar", 3)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewStringNode("baz", 3)));
    mu_assert_int_eq(Node_Length(arr), 3);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 0, &n));
    mu_check(NULL != n);
    mu_check(N_STRING == n->type);
    // arr = ["foo", "bar", "baz"]

    // Test inserting to the array
    Node *sub = NewArrayNode(2);  // <- [false, null]
    mu_check(NULL != sub);
    mu_check(OBJ_OK == Node_ArrayAppend(sub, NewBoolNode(0)));
    mu_check(OBJ_OK == Node_ArrayAppend(sub, NULL));
    mu_check(OBJ_OK == Node_ArrayInsert(arr, 0, sub));
    mu_assert_int_eq(Node_Length(arr), 5);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 0, &n));
    mu_check(NULL != n);
    mu_check(N_BOOLEAN == n->type);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 1, &n));
    mu_check(NULL == n);
    // arr = [false, null, "foo", "bar", "baz"]

    sub = NewArrayNode(1);  // <- ["qux"]
    mu_check(NULL != sub);
    mu_check(OBJ_OK == Node_ArrayAppend(sub, NewCStringNode("qux")));
    mu_check(OBJ_OK == Node_ArrayInsert(arr, 5, sub));
    mu_assert_int_eq(Node_Length(arr), 6);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 5, &n));
    mu_check(NULL != n);
    mu_check(N_STRING == n->type);
    // arr = [false, null, "foo", "bar", "qux", baz"]

    sub = NewArrayNode(2);
    mu_check(NULL != sub);  // <- [2, 2.719]
    mu_check(OBJ_OK == Node_ArrayAppend(sub, NewIntNode(2)));
    mu_check(OBJ_OK == Node_ArrayAppend(sub, NewDoubleNode(2.719)));
    mu_check(OBJ_OK == Node_ArrayInsert(arr, -1, sub));
    mu_assert_int_eq(Node_Length(arr), 8);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 5, &n));
    mu_check(NULL != n);
    mu_check(N_INTEGER == n->type);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 6, &n));
    mu_check(NULL != n);
    mu_check(N_NUMBER == n->type);
    mu_check(OBJ_OK == Node_ArrayItem(arr, 7, &n));
    mu_check(NULL != n);
    mu_check(N_STRING == n->type);
    // arr = [false, null, "foo", "bar", "qux", 2, 2.719, "baz"]

    // Find some values
    n = NewIntNode(2);
    mu_assert_int_eq(5, Node_ArrayIndex(arr, n, 0, 0));
    mu_assert_int_eq(5, Node_ArrayIndex(arr, n, 0, -1));
    mu_assert_int_eq(5, Node_ArrayIndex(arr, n, -7, -2));
    mu_assert_int_eq(5, Node_ArrayIndex(arr, n, -10, 0));
    mu_assert_int_eq(-1, Node_ArrayIndex(arr, n, 0, 5));
    mu_assert_int_eq(-1, Node_ArrayIndex(arr, n, 0, -3));
    mu_assert_int_eq(-1, Node_ArrayIndex(arr, n, 0, 1));
    mu_assert_int_eq(-1, Node_ArrayIndex(arr, n, -10, -9));
    Node_Free(n);

    n = NewDoubleNode(2.719);
    mu_assert_int_eq(Node_ArrayIndex(arr, n, 0, 0), 6);
    Node_Free(n);

    n = NewBoolNode(0);
    mu_assert_int_eq(Node_ArrayIndex(arr, n, 0, 0), 0);
    Node_Free(n);

    n = NewStringNode("qux", 3);
    mu_assert_int_eq(Node_ArrayIndex(arr, n, 0, 0), 7);
    Node_Free(n);

    n = NewStringNode("QUX", 3);
    mu_assert_int_eq(Node_ArrayIndex(arr, n, 0, 0), -1);
    Node_Free(n);

    mu_assert_int_eq(Node_ArrayIndex(arr, NULL, 0, 0), 1);

    // Delete some more elements
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 0, 1));
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 1, 1));
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 0, 1));
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 4, 1));
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 2, 1));
    mu_assert_int_eq(Node_Length(arr), 3);
    mu_check(OBJ_OK == Node_ArrayDelRange(arr, 0, 1));
    mu_assert_int_eq(Node_Length(arr), 2);

    Node_Free(arr);
}

MU_TEST(testObject) {
    Node *root = NewDictNode(1);
    mu_check(root != NULL);

    mu_check(OBJ_OK == Node_DictSet(root, "foo", NewStringNode("bar", 3)));
    mu_check(OBJ_OK == Node_DictSet(root, "bar", NewBoolNode(0)));
    mu_check(OBJ_OK == Node_DictSet(root, "baz", NewArrayNode(0)));
    mu_assert_int_eq(Node_Length(root), 3);

    Node *arr, *n;
    int rc = Node_DictGet(root, "non existing", &arr);
    mu_assert_int_eq(OBJ_ERR, rc);

    rc = Node_DictGet(root, "baz", &arr);
    mu_assert_int_eq(OBJ_OK, rc);

    mu_check(arr != NULL);

    mu_assert_int_eq(Node_ArrayAppend(arr, NewDoubleNode(3.141)), OBJ_OK);
    mu_assert_int_eq(Node_ArrayAppend(arr, NewIntNode(1337)), OBJ_OK);
    mu_assert_int_eq(Node_ArrayAppend(arr, NewStringNode("foo", 3)), OBJ_OK);
    mu_assert_int_eq(Node_ArrayAppend(arr, NULL), OBJ_OK);

    rc = Node_ArrayItem(arr, 0, &n);
    mu_assert_int_eq(OBJ_OK, rc);
    mu_check(n != NULL);
    mu_check(n->type == N_NUMBER);

    Node_Free(root);
}

MU_TEST(testPath) {
    Node *root = NewDictNode(1);
    mu_check(root != NULL);

    mu_check(OBJ_OK == Node_DictSet(root, "foo", NewStringNode("bar", 3)));
    mu_check(OBJ_OK == Node_DictSet(root, "bar", NewBoolNode(0)));

    Node *arr = NewArrayNode(0);
    Node_ArrayAppend(arr, NewStringNode("hello", 5));
    Node_ArrayAppend(arr, NewStringNode("world", 5));

    mu_check(OBJ_OK == Node_DictSet(root, "baz", arr));

    SearchPath sp = NewSearchPath(2);
    SearchPath_AppendKey(&sp, "baz", 3);
    SearchPath_AppendIndex(&sp, 0);

    Node *n = NULL;
    PathError pe = SearchPath_Find(&sp, root, &n);

    mu_check(pe == E_OK);
    mu_check(n != NULL);

    mu_check(n->type == N_STRING);
    mu_check(!strcmp(n->value.strval.data, "hello"));

    SearchPath_Free(&sp);
    Node_Free(root);
}

MU_TEST(testPathEx) {
    Node *root = NewDictNode(1);
    mu_check(NULL != root);

    mu_check(OBJ_OK == Node_DictSet(root, "foo", NewStringNode("bar", 3)));
    mu_check(OBJ_OK == Node_DictSet(root, "bar", NewBoolNode(0)));

    Node *arr = NewArrayNode(0);
    mu_check(NULL != arr);
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewStringNode("hello", 5)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewStringNode("world", 5)));
    mu_check(OBJ_OK == Node_DictSet(root, "arr", arr));

    Node *dict = NewDictNode(0);
    mu_check(NULL != dict);
    mu_check(OBJ_OK == Node_DictSet(dict, "f1", NULL));
    mu_check(OBJ_OK == Node_DictSet(dict, "f2", NewIntNode(6379)));
    mu_check(OBJ_OK == Node_DictSet(root, "dict", dict));

    Node *n = NULL;
    Node *p = NULL;
    int errlevel = 0;
    SearchPath sp;
    PathError pe;

    // sanity of a valid path
    sp = NewSearchPath(2);
    SearchPath_AppendKey(&sp, "arr", 3);
    SearchPath_AppendIndex(&sp, 0);
    pe = SearchPath_FindEx(&sp, root, &n, &p, &errlevel);
    mu_check(pe == E_OK);
    mu_check(arr == p);
    mu_check(n != NULL);
    mu_check(n->type == N_STRING);
    mu_check(!strcmp(n->value.strval.data, "hello"));
    SearchPath_Free(&sp);

    // check for non existing key in root
    sp = NewSearchPath(1);
    SearchPath_AppendKey(&sp, "qux", 3);
    pe = SearchPath_FindEx(&sp, root, &n, &p, &errlevel);
    mu_check(E_NOKEY == pe);
    mu_check(0 == errlevel);
    mu_check(p == root);
    SearchPath_Free(&sp);

    // check for non existing key in sub dictionary
    sp = NewSearchPath(2);
    SearchPath_AppendKey(&sp, "dict", 4);
    SearchPath_AppendKey(&sp, "f0", 2);
    pe = SearchPath_FindEx(&sp, root, &n, &p, &errlevel);
    mu_check(E_NOKEY == pe);
    mu_check(1 == errlevel);
    mu_check(p == dict);
    SearchPath_Free(&sp);

    // bad type
    sp = NewSearchPath(2);
    SearchPath_AppendKey(&sp, "foo", 3);
    SearchPath_AppendIndex(&sp, 0);
    pe = SearchPath_FindEx(&sp, root, &n, &p, &errlevel);
    mu_check(E_BADTYPE == pe);
    mu_check(1 == errlevel);
    SearchPath_Free(&sp);

    // check for non existing index
    sp = NewSearchPath(2);
    SearchPath_AppendKey(&sp, "arr", 3);
    SearchPath_AppendIndex(&sp, 99);
    pe = SearchPath_FindEx(&sp, root, &n, &p, &errlevel);
    mu_check(E_NOINDEX == pe);
    mu_check(1 == errlevel);
    mu_check(arr == p);
    SearchPath_Free(&sp);

    Node_Free(root);
}

MU_TEST(testPathArray) {
    Node *n, *arr = NewArrayNode(0);
    SearchPath sp;
    PathError pe;

    mu_check(NULL != arr);
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(0)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(1)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(2)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(3)));
    mu_check(OBJ_OK == Node_ArrayAppend(arr, NewIntNode(4)));
    mu_assert_int_eq(Node_Length(arr), 5);

    // test positive index path
    for (int i = 0; i < 5; i++) {
        sp = NewSearchPath(1);
        SearchPath_AppendIndex(&sp, i);
        pe = SearchPath_Find(&sp, arr, &n);
        mu_check(pe == E_OK);
        mu_check(NULL != n);
        mu_check(N_INTEGER == n->type);
        mu_check(i == n->value.intval);
        SearchPath_Free(&sp);
    }

    // test negative index path
    for (int i = -1; i > -6; i--) {
        sp = NewSearchPath(1);
        SearchPath_AppendIndex(&sp, i);
        pe = SearchPath_Find(&sp, arr, &n);
        mu_check(pe == E_OK);
        mu_check(NULL != n);
        mu_check(N_INTEGER == n->type);
        mu_check(5 + i == n->value.intval);
        SearchPath_Free(&sp);
    }

    // verify that out of bounds access errs
    sp = NewSearchPath(1);
    SearchPath_AppendIndex(&sp, 5);
    pe = SearchPath_Find(&sp, arr, &n);
    mu_check(E_NOINDEX == pe);
    SearchPath_Free(&sp);

    sp = NewSearchPath(1);
    SearchPath_AppendIndex(&sp, -6);
    pe = SearchPath_Find(&sp, arr, &n);
    mu_check(E_NOINDEX == pe);
    SearchPath_Free(&sp);

    Node_Free(arr);
}

MU_TEST(testPathParse) {
    const char *path = "foo.bar[3][\"baz\"].bar[\"boo\"][''][6379][-17].$nake_ca$e____";

    SearchPath sp = NewSearchPath(0);
    int rc = ParseJSONPath(path, strlen(path), &sp);
    mu_assert_int_eq(rc, PARSE_OK);

    mu_assert_int_eq(sp.len, 10);

    mu_check(sp.nodes[0].type == NT_KEY && !strcmp(sp.nodes[0].value.key, "foo"));
    mu_check(sp.nodes[1].type == NT_KEY && !strcmp(sp.nodes[1].value.key, "bar"));
    mu_check(sp.nodes[2].type == NT_INDEX && sp.nodes[2].value.index == 3);
    mu_check(sp.nodes[3].type == NT_KEY && !strcmp(sp.nodes[3].value.key, "baz"));
    mu_check(sp.nodes[4].type == NT_KEY && !strcmp(sp.nodes[4].value.key, "bar"));
    mu_check(sp.nodes[5].type == NT_KEY && !strcmp(sp.nodes[5].value.key, "boo"));
    mu_check(sp.nodes[6].type == NT_KEY && !strcmp(sp.nodes[6].value.key, ""));
    mu_check(sp.nodes[7].type == NT_INDEX && sp.nodes[7].value.index == 6379);
    mu_check(sp.nodes[8].type == NT_INDEX && sp.nodes[8].value.index == -17);

    const char *badpaths[] = {
        "3",        "6379",        "foo[bar]", "foo[]",         "foo[3",        "bar[\"]",
        "foo..bar", "foo[\"bar']", "foo/bar",  "foo.bar[-1.2]", "foo.bar[1.1]", "foo.bar[+3]",
        "1foo",     "f?oo",        "foo\n",    "foo\tbar",      "foobar[-i]",   NULL};

    for (int idx = 0; badpaths[idx] != NULL; idx++) {
        mu_check(ParseJSONPath(badpaths[idx], strlen(badpaths[idx]), &sp) == PARSE_ERR);
    }

    SearchPath_Free(&sp);
}

MU_TEST(testPathParseRoot) {
    const char *path = ".";

    SearchPath sp = NewSearchPath(0);
    int rc = ParseJSONPath(path, strlen(path), &sp);
    mu_assert_int_eq(rc, PARSE_OK);
    mu_assert_int_eq(sp.len, 1);
    mu_check(NT_ROOT == sp.nodes[0].type);

    SearchPath_Free(&sp);
}

MU_TEST_SUITE(test_object) {
    // MU_SUITE_CONFIGURE(&test_setup, &test_teardown);

    MU_RUN_TEST(testNodeString);
    MU_RUN_TEST(testNodeArray);
    MU_RUN_TEST(testObject);
    MU_RUN_TEST(testPath);
    MU_RUN_TEST(testPathEx);
    MU_RUN_TEST(testPathArray);
    MU_RUN_TEST(testPathParse);
    MU_RUN_TEST(testPathParseRoot);
}

int main(int argc, char *argv[]) {
    MU_RUN_SUITE(test_object);
    MU_REPORT();
    return minunit_fail;
}
