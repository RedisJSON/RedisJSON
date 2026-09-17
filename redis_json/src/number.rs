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
