# -*- coding: utf-8 -*-
"""
Flow tests for auto-creation of deep paths (MOD-18185 / RED-202601).

The `json-auto-create-deep-paths` config is IMMUTABLE, so it cannot be toggled
inside a running env -- these tests run in their own env with the config on, and
`test_auto_create_paths_disabled.py` covers the default (off) behavior.
"""

from RLTest import Env, Defaults

Defaults.decode_responses = True

AUTO_CREATE_ARGS = 'json-auto-create-deep-paths yes'


def _env():
    """A fresh Env with auto-creation enabled."""
    # noDefaultModuleArgs: don't merge the global --module-args, which would
    # otherwise be appended alongside ours.
    return Env(moduleArgs=AUTO_CREATE_ARGS, noDefaultModuleArgs=True)


def test_config_is_enabled_and_immutable():
    env = _env()
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'yes'])
    env.expect('CONFIG', 'SET', 'json-auto-create-deep-paths', 'no').raiseError()


# --------------------------------------------------------------------------- #
# JSON.SET -- existing key
# --------------------------------------------------------------------------- #

def test_json_set_creates_missing_levels_on_existing_key():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":5}}}]')


def test_json_set_creates_a_single_missing_leaf():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":5}}]')


def test_json_set_creates_from_an_empty_document():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c.d', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":{"d":5}}}}]')


def test_json_set_returns_nil_when_intermediate_is_a_scalar():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":3}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{"a":3}]')


def test_json_set_returns_nil_when_intermediate_is_an_array():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":[1,2]}').ok()
    env.expect('JSON.SET', 'k', '$.a.b', '5').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{"a":[1,2]}]')


def test_json_set_preserves_siblings_when_creating():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{"keep":1}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"keep":1,"b":{"c":5}}}]')


def test_json_set_creates_through_an_existing_array_element():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":[{},{}]}').ok()
    env.expect('JSON.SET', 'k', '$.a[1].b.c', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":[{},{"b":{"c":5}}]}]')


# --------------------------------------------------------------------------- #
# JSON.SET -- absent key
# --------------------------------------------------------------------------- #

def test_json_set_creates_a_whole_new_document():
    env = _env()
    env.expect('JSON.SET', 'newkey', '$.a.b.c', '5').ok()
    env.expect('JSON.GET', 'newkey', '$').equal('[{"a":{"b":{"c":5}}}]')


def test_json_set_creates_a_new_document_for_a_single_key():
    env = _env()
    env.expect('JSON.SET', 'newkey', '$.a', '5').ok()
    env.expect('JSON.GET', 'newkey', '$').equal('[{"a":5}]')


def test_json_set_creates_a_new_document_with_an_object_value():
    env = _env()
    env.expect('JSON.SET', 'newkey', '$.a.b', '{"d":{}}').ok()
    env.expect('JSON.GET', 'newkey', '$').equal('[{"a":{"b":{"d":{}}}}]')


def test_json_set_still_errors_on_absent_key_with_an_array_index():
    env = _env()
    env.expect('JSON.SET', 'nk', '$.a[0].b', '5').raiseError().contains(
        'new objects must be created at the root')
    env.expect('EXISTS', 'nk').equal(0)


def test_json_set_still_errors_on_absent_key_with_a_descendant_path():
    env = _env()
    env.expect('JSON.SET', 'nk', '$..a', '5').raiseError().contains(
        'new objects must be created at the root')
    env.expect('EXISTS', 'nk').equal(0)


# --------------------------------------------------------------------------- #
# NX / XX
# --------------------------------------------------------------------------- #

def test_json_set_xx_never_creates():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5', 'XX').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{}]')


def test_json_set_xx_never_creates_a_document():
    env = _env()
    env.expect('JSON.SET', 'nk', '$.a.b', '5', 'XX').equal(None)
    env.expect('EXISTS', 'nk').equal(0)


def test_json_set_nx_creates():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5', 'NX').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":5}}}]')


def test_json_set_nx_does_not_overwrite_an_existing_leaf():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{"b":{"c":1}}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5', 'NX').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":1}}}]')


# --------------------------------------------------------------------------- #
# Multi-target paths
# --------------------------------------------------------------------------- #

def test_json_set_union_creates_missing_and_updates_existing():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{},"b":{"c":1}}').ok()
    env.expect('JSON.SET', 'k', "$['a','b'].c", '9').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"c":9},"b":{"c":9}}]')


def test_json_set_wildcard_creates_under_every_object():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"p":{},"q":{},"s":"str"}').ok()
    env.expect('JSON.SET', 'k', '$.*.n', '7').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"p":{"n":7},"q":{"n":7},"s":"str"}]')


def test_json_set_descendant_creates_under_root_and_every_object():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '$..n', '7').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"n":7},"n":7}]')


def test_json_set_slice_creates_in_each_sliced_element():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":[{},{},{}]}').ok()
    env.expect('JSON.SET', 'k', '$.a[0:2].b', '7').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":[{"b":7},{"b":7},{}]}]')


# --------------------------------------------------------------------------- #
# Preserved errors and no-ops
# --------------------------------------------------------------------------- #

def test_array_indexes_are_never_created():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"arr":[]}').ok()
    env.expect('JSON.SET', 'k', '$.arr[0]', '7').raiseError().contains(
        'array index out of range')
    env.expect('JSON.SET', 'k', '$.a.b[0]', '7').raiseError().contains(
        'array index out of range')
    env.expect('JSON.GET', 'k', '$').equal('[{"arr":[]}]')


def test_max_depth_is_rejected_before_writing_anything():
    env = _env()
    deep = '.'.join('a%d' % i for i in range(200))
    env.expect('JSON.SET', 'nk', '$.' + deep, '5').raiseError().contains(
        'recursion limit exceeded')
    env.expect('EXISTS', 'nk').equal(0)

    env.expect('JSON.SET', 'k', '$', '{"keep":1}').ok()
    env.expect('JSON.SET', 'k', '$.' + deep, '5').raiseError().contains(
        'recursion limit exceeded')
    # nothing half-created
    env.expect('JSON.GET', 'k', '$').equal('[{"keep":1}]')


def test_legacy_path_creates_too():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '.a.b.c', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":5}}}]')


def test_projection_path_is_rejected():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{"a":1}').ok()
    env.expect('JSON.SET', 'k', '$.a + 1', '5').raiseError()


# --------------------------------------------------------------------------- #
# Value formats survive creation (the leaf value is never re-serialized)
# --------------------------------------------------------------------------- #

def test_created_leaf_keeps_a_container_value():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{}').ok()
    env.expect('JSON.SET', 'k', '$.a.b', '{"x":[1,2],"y":null}').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"x":[1,2],"y":null}}}]')


def test_created_leaf_supports_fpha():
    env = _env()
    env.expect('JSON.SET', 'k', '$', '{}').ok()
    env.expect('JSON.SET', 'k', '$.a.b', '[1.5,2.5,3.5]', 'FPHA', 'FP32').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":[1.5,2.5,3.5]}}]')


# --------------------------------------------------------------------------- #
# Persistence
# --------------------------------------------------------------------------- #

def test_created_document_survives_rdb_reload():
    env = _env()
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').ok()
    for _ in env.retry_with_rdb_reload():
        env.assertExists('k')
        env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":5}}}]')
