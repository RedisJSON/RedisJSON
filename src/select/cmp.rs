use crate::select::select_value::SelectValue;
use array_tool::vec::{Intersect, Union};

pub(super) trait Cmp<'a, T>
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool;

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool;

    fn cmp_string(&self, v1: &str, v2: &str) -> bool;

    fn cmp_json(&self, v1: &[&'a T], v2: &[&'a T]) -> Vec<&'a T>;

    fn default(&self) -> bool {
        false
    }
}

pub(super) struct CmpEq;

impl<'a, T> Cmp<'a, T> for CmpEq
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 == v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        (v1 - v2).abs() == 0_f64
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 == v2
    }

    fn cmp_json(&self, v1: &[&'a T], v2: &[&'a T]) -> Vec<&'a T> {
        v1.to_vec().intersect(v2.to_vec())
    }
}

pub(super) struct CmpNe;

impl<'a, T> Cmp<'a, T> for CmpNe
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 != v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        (v1 - v2).abs() != 0_f64
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 != v2
    }

    fn cmp_json(&self, v1: &[&'a T], v2: &[&'a T]) -> Vec<&'a T> {
        v1.to_vec().intersect_if(v2.to_vec(), |a, b| a != b)
    }
}

pub(super) struct CmpGt;

impl<'a, T> Cmp<'a, T> for CmpGt
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 & !v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        v1 > v2
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 > v2
    }

    fn cmp_json(&self, _: &[&'a T], _: &[&'a T]) -> Vec<&'a T> {
        Vec::new()
    }
}

pub(super) struct CmpGe;

impl<'a, T> Cmp<'a, T> for CmpGe
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 >= v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        v1 >= v2
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 >= v2
    }

    fn cmp_json(&self, _: &[&'a T], _: &[&'a T]) -> Vec<&'a T> {
        Vec::new()
    }
}

pub(super) struct CmpLt;

impl<'a, T> Cmp<'a, T> for CmpLt
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        !v1 & v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        v1 < v2
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 < v2
    }

    fn cmp_json(&self, _: &[&'a T], _: &[&'a T]) -> Vec<&'a T> {
        Vec::new()
    }
}

pub(super) struct CmpLe;

impl<'a, T> Cmp<'a, T> for CmpLe
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 <= v2
    }

    fn cmp_f64(&self, v1: f64, v2: f64) -> bool {
        v1 <= v2
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        v1 <= v2
    }

    fn cmp_json(&self, _: &[&'a T], _: &[&'a T]) -> Vec<&'a T> {
        Vec::new()
    }
}

pub(super) struct CmpAnd;

impl<'a, T> Cmp<'a, T> for CmpAnd
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 && v2
    }

    fn cmp_f64(&self, _v1: f64, _v2: f64) -> bool {
        true
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        !v1.is_empty() && !v2.is_empty()
    }

    fn cmp_json(&self, v1: &[&'a T], v2: &[&'a T]) -> Vec<&'a T> {
        v1.to_vec().intersect(v2.to_vec())
    }
}

pub(super) struct CmpOr;

impl<'a, T> Cmp<'a, T> for CmpOr
where
    T: SelectValue,
{
    fn cmp_bool(&self, v1: bool, v2: bool) -> bool {
        v1 || v2
    }

    fn cmp_f64(&self, _v1: f64, _v2: f64) -> bool {
        true
    }

    fn cmp_string(&self, v1: &str, v2: &str) -> bool {
        !v1.is_empty() || !v2.is_empty()
    }

    fn cmp_json(&self, v1: &[&'a T], v2: &[&'a T]) -> Vec<&'a T> {
        v1.to_vec().union(v2.to_vec())
    }
}

// #[cfg(test)]
// mod cmp_inner_tests {
//     use serde_json::Value;

//     use crate::select::cmp::*;

//     #[test]
//     fn cmp_eq() {
//         let cmp_fn = CmpEq;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), false);
//         assert_eq!(cmp_fn.cmp_bool(true, true), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.1), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), false);
//         assert_eq!(cmp_fn.cmp_string("1", "1"), true);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), false);
//     }

//     #[test]
//     fn cmp_ne() {
//         let cmp_fn = CmpNe;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), false);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.1), false);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), true);
//         assert_eq!(cmp_fn.cmp_string("1", "1"), false);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), true);
//     }

//     #[test]
//     fn cmp_gt() {
//         let cmp_fn = CmpGt;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), false);
//         assert_eq!(cmp_fn.cmp_f64(0.2, 0.1), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), false);
//         assert_eq!(cmp_fn.cmp_string("a", "a"), false);
//         assert_eq!(cmp_fn.cmp_string("b", "a"), true);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), false);
//     }

//     #[test]
//     fn cmp_ge() {
//         let cmp_fn = CmpGe;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), true);
//         assert_eq!(cmp_fn.cmp_f64(0.2, 0.1), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.1), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), false);
//         assert_eq!(cmp_fn.cmp_string("1", "1"), true);
//         assert_eq!(cmp_fn.cmp_string("ab", "a"), true);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), false);
//     }

//     #[test]
//     fn cmp_lt() {
//         let cmp_fn = CmpLt;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), false);
//         assert_eq!(cmp_fn.cmp_bool(false, true), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), false);
//         assert_eq!(cmp_fn.cmp_bool(false, false), false);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.1), false);
//         assert_eq!(cmp_fn.cmp_f64(0.2, 0.1), false);
//         assert_eq!(cmp_fn.cmp_string("a", "a"), false);
//         assert_eq!(cmp_fn.cmp_string("ab", "b"), true);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), true);
//     }

//     #[test]
//     fn cmp_le() {
//         let cmp_fn = CmpLe;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), false);
//         assert_eq!(cmp_fn.cmp_bool(false, true), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), true);
//         assert_eq!(cmp_fn.cmp_bool(false, false), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.2), true);
//         assert_eq!(cmp_fn.cmp_f64(0.1, 0.1), true);
//         assert_eq!(cmp_fn.cmp_f64(0.2, 0.1), false);
//         assert_eq!(cmp_fn.cmp_string("a", "a"), true);
//         assert_eq!(cmp_fn.cmp_string("ab", "b"), true);
//         assert_eq!(cmp_fn.cmp_string("abd", "abc"), false);
//         assert_eq!(cmp_fn.cmp_string("1", "2"), true);
//     }

//     #[test]
//     fn cmp_and() {
//         let cmp_fn = CmpAnd;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), false);
//         assert_eq!(cmp_fn.cmp_bool(false, true), false);
//         assert_eq!(cmp_fn.cmp_bool(true, true), true);
//         assert_eq!(cmp_fn.cmp_bool(false, false), false);
//         assert_eq!(cmp_fn.cmp_f64(0.0, 0.0), true);
//         assert_eq!(cmp_fn.cmp_string("a", "a"), true);
//     }

//     #[test]
//     fn cmp_or() {
//         let cmp_fn = CmpOr;
//         assert_eq!(cmp_fn.default(), false);
//         assert_eq!(cmp_fn.cmp_bool(true, false), true);
//         assert_eq!(cmp_fn.cmp_bool(false, true), true);
//         assert_eq!(cmp_fn.cmp_bool(true, true), true);
//         assert_eq!(cmp_fn.cmp_bool(false, false), false);
//         assert_eq!(cmp_fn.cmp_f64(0.0, 0.0), true);
//         assert_eq!(cmp_fn.cmp_string("a", "a"), true);
//     }

//     #[test]
//     fn cmp_json() {
//         let v1 = Value::Bool(true);
//         let v2 = Value::String("1".to_string());
//         let left = [&v1, &v2];
//         let right = [&v1, &v2];
//         let empty: Vec<&Value> = Vec::new();

//         assert_eq!(CmpEq.cmp_json(&left, &right), left.to_vec());
//         assert_eq!(CmpNe.cmp_json(&left, &right), left.to_vec());
//         assert_eq!(CmpGt.cmp_json(&left, &right), empty);
//         assert_eq!(CmpGe.cmp_json(&left, &right), empty);
//         assert_eq!(CmpLt.cmp_json(&left, &right), empty);
//         assert_eq!(CmpLe.cmp_json(&left, &right), empty);
//         assert_eq!(CmpAnd.cmp_json(&left, &right), left.to_vec());
//         assert_eq!(CmpOr.cmp_json(&left, &right), left.to_vec());

//         assert_eq!(
//             CmpEq.cmp_json(&[&Value::Bool(true)], &[&Value::Bool(true)]),
//             vec![&Value::Bool(true)]
//         );
//         assert_eq!(
//             CmpEq.cmp_json(&[&Value::Bool(true)], &[&Value::Bool(false)]),
//             empty
//         );
//         assert_eq!(
//             CmpNe.cmp_json(&[&Value::Bool(true)], &[&Value::Bool(true)]),
//             empty
//         );
//         assert_eq!(
//             CmpNe.cmp_json(&[&Value::Bool(false)], &[&Value::Bool(true)]),
//             vec![&Value::Bool(false)]
//         );
//         assert_eq!(
//             CmpAnd.cmp_json(&[&Value::Bool(true)], &[&Value::Bool(true)]),
//             vec![&Value::Bool(true)]
//         );
//         assert_eq!(
//             CmpOr.cmp_json(&[&Value::Bool(true)], &[&Value::Bool(false)]),
//             vec![&Value::Bool(true), &Value::Bool(false)]
//         );
//     }
// }
