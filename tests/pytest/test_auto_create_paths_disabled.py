# -*- coding: utf-8 -*-
"""
Pins the default (config off) behavior of the paths that auto-creation changes,
so enabling it stays strictly opt-in (MOD-18185 / RED-202601).

Runs in the default env -- `json-auto-create-deep-paths` is IMMUTABLE and
defaults to off, so nothing here needs special module args.
"""

from RLTest import Defaults

Defaults.decode_responses = True


def test_config_defaults_to_off(env):
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'no'])


def test_missing_intermediate_still_returns_nil(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{}}]')


def test_absent_key_still_errors(env):
    env.expect('JSON.SET', 'nk', '$.a.b.c', '5').raiseError().contains(
        'new objects must be created at the root')
    env.expect('EXISTS', 'nk').equal(0)


def test_missing_leaf_under_an_existing_object_still_works(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.SET', 'k', '$.a.b', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":5}}]')


def test_non_static_path_with_no_match_still_errors(env):
    env.expect('JSON.SET', 'k', '$', '{"p":{},"q":{}}').ok()
    env.expect('JSON.SET', 'k', '$.*.n', '7').raiseError().contains('static path')
    env.expect('JSON.GET', 'k', '$').equal('[{"p":{},"q":{}}]')


def test_union_still_updates_only_existing_targets(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{},"b":{"c":1}}').ok()
    env.expect('JSON.SET', 'k', "$['a','b'].c", '9').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{},"b":{"c":9}}]')


def test_array_index_errors_are_unchanged(env):
    env.expect('JSON.SET', 'k', '$', '{"arr":[]}').ok()
    env.expect('JSON.SET', 'k', '$.arr[0]', '7').raiseError().contains(
        'array index out of range')
    env.expect('JSON.SET', 'k', '$.a.b[0]', '7').raiseError().contains(
        'array index out of range')


def test_merge_missing_intermediate_still_returns_nil(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.MERGE', 'k', '$.a.b.c', '5').equal(None)
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{}}]')


def test_merge_absent_key_still_errors(env):
    env.expect('JSON.MERGE', 'nk', '$.a.b', '5').raiseError().contains(
        'new objects must be created at the root')
    env.expect('EXISTS', 'nk').equal(0)


def test_merge_missing_leaf_under_an_existing_object_still_works(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{}}').ok()
    env.expect('JSON.MERGE', 'k', '$.a.b', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":5}}]')


def test_non_static_path_that_matches_still_updates(env):
    env.expect('JSON.SET', 'k', '$', '{"a":{"a":1}}').ok()
    env.expect('JSON.SET', 'k', '$..a', '5').ok()
    env.expect('JSON.GET', 'k', '$').equal('[{"a":5}]')
    env.expect('JSON.SET', 'k2', '$', '{"a":{"a":1}}').ok()
    env.expect('JSON.MERGE', 'k2', '$..a', '{"a":"b"}').ok()
    env.expect('JSON.GET', 'k2', '$').equal('[{"a":{"a":{"a":"b"}}}]')
