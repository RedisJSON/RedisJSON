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
        
