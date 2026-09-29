/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use ijson::INumber;
use json_path::select_value::{SelectValue, SelectValueType};
use redis_module::RedisResult;

/// Arithmetic shared by OSS writes and creation preflight on every backend.
/// Integer operations must fit i64; floating-point results must be finite.
pub(crate) fn number_op_result<V: SelectValue, F1, F2>(
    value: &V,
    operand: &serde_json::Value,
    op_int: F1,
    op_float: F2,
) -> RedisResult<INumber>
where
    F1: FnOnce(i128, i128) -> Option<i128>,
    F2: FnOnce(f64, f64) -> f64,
{
    match (value.get_type(), operand.as_i64()) {
        (SelectValueType::Long, Some(num2)) => {
            let num1 = value.get_long().ok_or(crate::manager::err_not_a_number())?;
            Ok(op_int(num1 as i128, num2 as i128)
                .and_then(|r| i64::try_from(r).ok())
                .ok_or(crate::manager::err_numeric_overflow())?
                .into())
        }
        _ => {
            let num1 = value
                .get_double()
                .ok_or(crate::manager::err_not_a_number())?;
            let num2 = operand.as_f64().ok_or(crate::manager::err_not_a_number())?;
            INumber::try_from(op_float(num1, num2)).map_err(|_| crate::manager::err_not_a_number())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{err_not_a_number, err_numeric_overflow};
    use ijson::IValue;
    use serde_json::json;

    #[test]
    fn integer_addition_accepts_boundaries_and_rejects_overflow_and_underflow() {
        for (value, operand, expected) in
            [(i64::MAX - 1, 1, i64::MAX), (i64::MIN + 1, -1, i64::MIN)]
        {
            let result = number_op_result(
                &IValue::from(value),
                &json!(operand),
                i128::checked_add,
                |a, b| a + b,
            )
            .unwrap();
            assert_eq!(result.to_i64(), Some(expected));
        }
        for (value, operand) in [(i64::MAX, 1), (i64::MIN, -1)] {
            let error = number_op_result(
                &IValue::from(value),
                &json!(operand),
                i128::checked_add,
                |a, b| a + b,
            )
            .unwrap_err();
            assert_eq!(error.to_string(), err_numeric_overflow().to_string());
        }
        let error = number_op_result(
            &IValue::from(i64::MAX),
            &json!(3),
            |a, b| a.checked_pow(b as u32),
            f64::powf,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), err_numeric_overflow().to_string());
    }

    #[test]
    fn mixed_integer_and_float_addition_uses_float_arithmetic() {
        for (value, operand) in [("1", json!(1.5)), ("1.5", json!(1)), ("1.0", json!(1.5))] {
            let value: IValue = serde_json::from_str(value).unwrap();
            let result =
                number_op_result(&value, &operand, i128::checked_add, |a, b| a + b).unwrap();
            assert_eq!(result.to_f64_lossy(), 2.5);
            assert!(result.has_decimal_point());
        }
    }

    #[test]
    fn non_finite_results_are_rejected_but_finite_underflow_is_allowed() {
        for value in [1e308, -1e308] {
            let error = number_op_result(
                &IValue::from(value),
                &json!(value),
                i128::checked_add,
                |a, b| a + b,
            )
            .unwrap_err();
            assert_eq!(error.to_string(), err_not_a_number().to_string());
        }
        let error = number_op_result(
            &IValue::from(0.0),
            &json!(0.0),
            i128::checked_div,
            |a, b| a / b,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), err_not_a_number().to_string());
        let result = number_op_result(
            &IValue::from(f64::from_bits(1)),
            &json!(2.0),
            i128::checked_div,
            |a, b| a / b,
        )
        .unwrap();
        assert_eq!(result.to_f64_lossy(), 0.0);
    }

    #[test]
    fn large_unsigned_integers_fall_back_to_float() {
        for (value, operand) in [
            (IValue::from(u64::MAX), json!(1)),
            (IValue::from(1), json!(u64::MAX)),
        ] {
            let result = number_op_result(
                &value,
                &operand,
                |_, _| panic!("large unsigned integers require float arithmetic"),
                |a, b| a + b,
            )
            .unwrap();
            assert_eq!(result.to_f64_lossy(), u64::MAX as f64 + 1.0);
        }
    }

    #[test]
    fn non_numeric_inputs_are_rejected() {
        for (value, operand) in [
            (IValue::from("text"), json!(1)),
            (IValue::from(1), json!("text")),
        ] {
            let error =
                number_op_result(&value, &operand, i128::checked_add, |a, b| a + b).unwrap_err();
            assert_eq!(error.to_string(), err_not_a_number().to_string());
        }
    }
}
