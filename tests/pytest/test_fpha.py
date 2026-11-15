# -*- coding: utf-8 -*-
"""
Tests for FPHA (Floating Point Hint for Arrays) feature.
"""

from RLTest import Defaults

Defaults.decode_responses = True


def test_fpha_basic_fp32(env):
    """Test JSON.SET with FPHA FP32 option"""
    env.expect(
        "JSON.SET", "fp32_arr", "$", "[10.0, 20.0, 30.0, 40.0, 50.0]", "FPHA", "FP32"
    ).ok()


def test_fpha_basic_fp64(env):
    """Test JSON.SET with FPHA FP64 option"""
    env.expect(
        "JSON.SET", "fp64_arr", "$", "[0.1, 0.2, 0.3, 0.4, 0.5]", "FPHA", "FP64"
    ).ok()


def test_fpha_basic_fp16(env):
    """Test JSON.SET with FPHA FP16 option"""
    env.expect(
        "JSON.SET", "fp16_arr", "$", "[10.0, 20.0, 30.0, 40.0, 50.0]", "FPHA", "FP16"
    ).ok()


def test_fpha_basic_bf16(env):
    """Test JSON.SET with FPHA BF16 option"""
    env.expect(
        "JSON.SET", "bf16_arr", "$", "[10.0, 20.0, 30.0, 40.0, 50.0]", "FPHA", "BF16"
    ).ok()


def test_fpha_invalid_type(env):
    """Test that invalid FPHA syntax returns error"""

    env.expect(
        "JSON.SET", "invalid", "$", "[0.1, 0.2]", "FPHA", "INVALID"
    ).raiseError().contains("invalid FPHA type")

    env.expect("JSON.SET", "invalid", "$", "[0.1, 0.2]", "FPHA").raiseError().contains(
        "wrong number of arguments"
    )


def test_fpha_no_fit_fp16(env):
    """Test JSON.SET with FPHA FP16 option"""
    env.expect(
        "JSON.SET", "fp16_arr", "$", "[0.1, 0.2, 0.3, 1e100, 0.5]", "FPHA", "FP16"
    ).raiseError().contains("value out of range")


def test_fpha_rdb_save_load(env):
    """Test that FPHA type survives RDB save and reload"""
    env.skipOnCluster()
    if env.useAof:
        env.skip()

    env.expect("JSON.SET", "fp32_rdb", "$", "[1.5, 2.5, 3.5]", "FPHA", "FP32").ok()
    env.expect("JSON.GET", "fp32_rdb", "$").equal("[[1.5,2.5,3.5]]")

    for _ in env.retry_with_rdb_reload():
        env.assertExists("fp32_rdb")
        env.expect("JSON.GET", "fp32_rdb", "$").equal("[[1.5,2.5,3.5]]")


def test_fpha_rdb_save_load_after_type_change(env):
    """Test RDB save/load with fallback when array type changed after initial FPHA"""
    env.skipOnCluster()
    if env.useAof:
        env.skip()

    env.expect("JSON.SET", "fp16_rdb", "$", "[10.0, 20.0, 30.0]", "FPHA", "FP16").ok()

    env.expect("JSON.ARRAPPEND", "fp16_rdb", "$", "1e10").noError()

    for _ in env.retry_with_rdb_reload():
        env.assertExists("fp16_rdb")
        env.expect("JSON.GET", "fp16_rdb", "$[0]").noError()


def test_fpha_json_get_short_float_serialization(env):
    """FPHA typed arrays must serialize via JSON.GET with short decimals (not f64 noise)."""
    payload = "[0.3,0.1,0.7,1.0,2.5,100.0]"
    expected = "[[0.3,0.1,0.7,1.0,2.5,100.0]]"
    for fpha in ("FP32", "FP16", "BF16", "FP64"):
        key = "mod14559_" + fpha.lower()
        env.expect("JSON.SET", key, "$", payload, "FPHA", fpha).ok()
        env.expect("JSON.GET", key, "$").equal(expected)


def test_fpha_json_get_negative_short_floats(env):
    """Same short serialization for negative typed floats."""
    payload = "[-0.3,-0.1,-1.0,-2.5,-100.0]"
    expected = "[[-0.3,-0.1,-1.0,-2.5,-100.0]]"
    for fpha in ("FP32", "FP16", "BF16", "FP64"):
        key = "mod14559_neg_" + fpha.lower()
        env.expect("JSON.SET", key, "$", payload, "FPHA", fpha).ok()
        env.expect("JSON.GET", key, "$").equal(expected)




# Each document is a single-element array stored with a numeric (typed) tag.
TYPED_SINGLETONS = [
    ("[0]",),
    ("[-1]",),
    ("[300]",),
    ("[-300]",),
    ("[70000]",),
    ("[5000000000]",),
    ("[1.5]",),
    ("[1.5]", "FPHA", "FP16"),
    ("[1.5]", "FPHA", "BF16"),
    ("[1.5]", "FPHA", "FP32"),
    ("[1.5]", "FPHA", "FP64"),
]
NON_NUMERIC_VALUES = ['{"a":1}', '"s"', "true", "null", "[1]"]


def _set_typed_singleton(env, key, doc):
    env.expect("JSON.SET", key, "$", '{"a":%s}' % doc[0], *doc[1:]).ok()


def test_replace_sole_typed_element_with_non_numeric(env):
    """MOD-20160: replacing the only element of a typed array with a non-number must not crash."""
    for doc in TYPED_SINGLETONS:
        for value in NON_NUMERIC_VALUES:
            _set_typed_singleton(env, "k", doc)
            env.expect("JSON.SET", "k", "$.a[0]", value).ok()
            env.expect("JSON.GET", "k", "$.a").equal("[[%s]]" % value)

            _set_typed_singleton(env, "k", doc)
            env.expect("JSON.MSET", "k", "$.a[0]", value).ok()
            env.expect("JSON.GET", "k", "$.a").equal("[[%s]]" % value)
    env.expect("PING").noError()


def test_add_non_numeric_to_emptied_typed_array(env):
    """MOD-20160: an emptied typed array must accept non-numbers via ARRAPPEND/ARRINSERT."""
    empty_commands = [
        ("JSON.ARRPOP", "k", "$.a"),
        ("JSON.ARRTRIM", "k", "$.a", "1", "0"),
        ("JSON.DEL", "k", "$.a[0]"),
        ("JSON.CLEAR", "k", "$.a"),
    ]
    for doc in TYPED_SINGLETONS:
        for empty in empty_commands:
            for value in NON_NUMERIC_VALUES:
                _set_typed_singleton(env, "k", doc)
                env.cmd(*empty)
                env.expect("JSON.GET", "k", "$.a").equal("[[]]")
                env.expect("JSON.ARRAPPEND", "k", "$.a", value, "7").equal([2])
                env.expect("JSON.GET", "k", "$.a").equal("[[%s,7]]" % value)

                _set_typed_singleton(env, "k", doc)
                env.cmd(*empty)
                env.expect("JSON.ARRINSERT", "k", "$.a", "0", value).equal([1])
                env.expect("JSON.GET", "k", "$.a").equal("[[%s]]" % value)
    env.expect("PING").noError()


def test_numeric_replace_of_sole_typed_element(env):
    """Numeric replacement of a typed singleton keeps working."""
    env.expect("JSON.SET", "k", "$", "[0]").ok()
    env.expect("JSON.SET", "k", "$[0]", "5").ok()
    env.expect("JSON.GET", "k", "$").equal("[[5]]")
    env.expect("JSON.SET", "k", "$[0]", "2.5").ok()
    env.expect("JSON.GET", "k", "$").equal("[[2.5]]")
    env.expect("JSON.SET", "k", "$", "[1.5]", "FPHA", "FP32").ok()
    env.expect("JSON.SET", "k", "$[0]", "2.5").ok()
    env.expect("JSON.GET", "k", "$").equal("[[2.5]]")


def test_emptied_typed_array_survives_rdb_reload(env):
    """An emptied typed array reloaded from RDB must accept non-numbers."""
    env.skipOnCluster()
    if env.useAof:
        env.skip()

    env.expect("JSON.SET", "k", "$", '{"a":[300]}').ok()
    env.cmd("JSON.ARRPOP", "k", "$.a")
    for _ in env.retry_with_rdb_reload():
        env.expect("JSON.GET", "k", "$.a").equal("[[]]")
    env.expect("JSON.ARRAPPEND", "k", "$.a", '{"b":1}').equal([1])
    env.expect("JSON.GET", "k", "$.a").equal('[[{"b":1}]]')
