/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use std::{ffi::c_void, ptr::null};

/// Use `SelectValue`
use crate::select_value::{JSONArrayType, SelectValue, SelectValueType, ValueRef};
use ijson::{array::ArrayIterItem, DestructuredRef, IString, IValue, ValueType};
use serde_json::Value;

impl SelectValue for Value {
    fn get_type(&self) -> SelectValueType {
        match self {
            Self::Bool(_) => SelectValueType::Bool,
            Self::String(_) => SelectValueType::String,
            Self::Null => SelectValueType::Null,
            Self::Array(_) => SelectValueType::Array,
            Self::Object(_) => SelectValueType::Object,
            Self::Number(n) if n.is_i64() => SelectValueType::Long,
            Self::Number(n) if n.is_f64() | n.is_u64() => SelectValueType::Double,
            // Code is unused, but we need to satisfy the trait...
            _ => unreachable!("bad type for Number value"),
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Object(o) => o.contains_key(key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>> {
        match self {
            Self::Array(arr) => Some(Box::new(arr.iter().map(ValueRef::Borrowed))),
            Self::Object(o) => Some(Box::new(o.values().map(ValueRef::Borrowed))),
            _ => None,
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(o.keys().map(|k| &k[..]))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, ValueRef<'a, Self>)> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(
                o.iter().map(|(k, v)| (&k[..], ValueRef::Borrowed(v))),
            )),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self {
            Self::Array(arr) => Some(arr.len()),
            Self::Object(obj) => Some(obj.len()),
            _ => None,
        }
    }

    fn is_empty(&self) -> Option<bool> {
        match self {
            Self::Array(arr) => Some(arr.is_empty()),
            Self::Object(obj) => Some(obj.is_empty()),
            _ => None,
        }
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<ValueRef<'a, Self>> {
        match self {
            Self::Object(o) => o.get(key).map(ValueRef::Borrowed),
            _ => None,
        }
    }

    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>> {
        match self {
            Self::Array(arr) => arr.get(index).map(ValueRef::Borrowed),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    fn is_double(&self) -> Option<bool> {
        match self {
            Self::Number(num) => Some(num.is_f64()),
            _ => None,
        }
    }

    fn get_str(&self) -> Option<String> {
        match self {
            Self::String(s) => Some(s.to_string()),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn get_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn get_long(&self) -> Option<i64> {
        match self {
            Self::Number(n) if n.is_i64() => n.as_i64(),
            Self::Number(_) => None,
            _ => None,
        }
    }

    fn get_double(&self) -> Option<f64> {
        match self {
            Self::Number(n) if n.is_f64() => n.as_f64(),
            Self::Number(n) if n.is_u64() => n.as_u64().map(|u| u as f64),
            Self::Number(_) => None,
            _ => None,
        }
    }

    fn get_array(&self) -> *const c_void {
        match self {
            Self::Array(arr) => arr.as_slice().as_ptr() as *const c_void,
            Self::Bool(_) | Self::Null | Self::Number(_) | Self::String(_) | Self::Object(_) => {
                null()
            }
        }
    }

    fn get_array_type(&self) -> Option<JSONArrayType> {
        match self {
            Self::Array(_) => Some(JSONArrayType::Heterogeneous),
            Self::Bool(_) | Self::Null | Self::Number(_) | Self::String(_) | Self::Object(_) => {
                None
            }
        }
    }
}

impl<'a> From<ArrayIterItem<'a>> for ValueRef<'a, IValue> {
    fn from(item: ArrayIterItem<'a>) -> Self {
        match item {
            ArrayIterItem::Borrowed(val) => ValueRef::Borrowed(val),
            ArrayIterItem::Owned(val) => ValueRef::Owned(val),
        }
    }
}

impl SelectValue for IValue {
    fn get_type(&self) -> SelectValueType {
        match self.type_() {
            ValueType::Bool => SelectValueType::Bool,
            ValueType::String => SelectValueType::String,
            ValueType::Null => SelectValueType::Null,
            ValueType::Array => SelectValueType::Array,
            ValueType::Object => SelectValueType::Object,
            ValueType::Number => {
                let num = self.as_number().unwrap();
                if num.has_decimal_point() | num.to_i64().is_none() {
                    SelectValueType::Double
                } else {
                    SelectValueType::Long
                }
            }
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        self.as_object().is_some_and(|o| o.contains_key(key))
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>> {
        match self.destructure_ref() {
            DestructuredRef::Array(arr) => Some(Box::new(arr.iter().map(Into::into))),
            DestructuredRef::Object(o) => Some(Box::new(o.values().map(ValueRef::Borrowed))),
            _ => None,
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self.destructure_ref() {
            DestructuredRef::Object(o) => Some(Box::new(o.keys().map(IString::as_str))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, ValueRef<'a, Self>)> + 'a>> {
        match self.destructure_ref() {
            DestructuredRef::Object(o) => Some(Box::new(
                o.iter().map(|(k, v)| (k.as_str(), ValueRef::Borrowed(v))),
            )),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self.destructure_ref() {
            DestructuredRef::Array(arr) => Some(arr.len() as usize),
            DestructuredRef::Object(o) => Some(o.len() as usize),
            _ => None,
        }
    }

    fn is_empty(&self) -> Option<bool> {
        self.is_empty()
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<ValueRef<'a, Self>> {
        self.as_object()
            .and_then(|o| o.get(key).map(ValueRef::Borrowed))
    }

    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>> {
        use ijson::array::ArraySliceRef;
        let arr = self.as_array()?;
        // Index straight into the backing slice. `ArrayIter` does not override
        // `Iterator::nth`, so `iter().nth(index)` walks the array element by element,
        // which makes an index-by-index pass over an N-element array O(N^2) --
        // painfully visible for high-dimension vectors read through the LLAPI.
        macro_rules! get_indexed {
            ($($variant:ident),*) => {
                match arr.as_slice() {
                    ArraySliceRef::Heterogeneous(s) => s.get(index).map(ValueRef::Borrowed),
                    $(ArraySliceRef::$variant(s) => {
                        s.get(index).map(|&v| ValueRef::Owned(IValue::from(v)))
                    })*
                }
            };
        }
        get_indexed!(I8, U8, I16, U16, F16, BF16, I32, U32, F32, I64, U64, F64)
    }

    fn is_array(&self) -> bool {
        self.is_array()
    }

    fn is_double(&self) -> Option<bool> {
        Some(self.as_number()?.has_decimal_point())
    }

    fn get_str(&self) -> Option<String> {
        self.as_string().map(|s| s.to_string())
    }

    fn as_str(&self) -> Option<&str> {
        self.as_string().map(IString::as_str)
    }

    fn get_bool(&self) -> Option<bool> {
        self.to_bool()
    }

    fn get_long(&self) -> Option<i64> {
        match self.type_() {
            ValueType::Number => self.as_number().and_then(|n| n.to_i64()),
            _ => None,
        }
    }

    fn get_double(&self) -> Option<f64> {
        match self.type_() {
            ValueType::Number => self.as_number().map(|n| n.to_f64_lossy()),
            _ => None,
        }
    }

    fn get_array(&self) -> *const c_void {
        use ijson::array::ArraySliceRef;
        match self.destructure_ref() {
            DestructuredRef::Array(arr) => {
                macro_rules! slice_ptr {
                    ($($variant:ident),*) => {
                        match arr.as_slice() {
                            $(ArraySliceRef::$variant(s) => s.as_ptr() as *const c_void,)*
                        }
                    }
                }
                slice_ptr!(
                    Heterogeneous,
                    I8,
                    U8,
                    I16,
                    U16,
                    F16,
                    BF16,
                    I32,
                    U32,
                    F32,
                    I64,
                    U64,
                    F64
                )
            }
            _ => null(),
        }
    }

    fn get_array_type(&self) -> Option<JSONArrayType> {
        use ijson::array::ArrayTag;
        match self.destructure_ref() {
            DestructuredRef::Array(arr) => {
                let type_tag = arr.as_slice().type_tag();
                Some(match type_tag {
                    ArrayTag::Heterogeneous => JSONArrayType::Heterogeneous,
                    ArrayTag::I8 => JSONArrayType::I8,
                    ArrayTag::U8 => JSONArrayType::U8,
                    ArrayTag::I16 => JSONArrayType::I16,
                    ArrayTag::U16 => JSONArrayType::U16,
                    ArrayTag::F16 => JSONArrayType::F16,
                    ArrayTag::BF16 => JSONArrayType::BF16,
                    ArrayTag::I32 => JSONArrayType::I32,
                    ArrayTag::U32 => JSONArrayType::U32,
                    ArrayTag::F32 => JSONArrayType::F32,
                    ArrayTag::I64 => JSONArrayType::I64,
                    ArrayTag::U64 => JSONArrayType::U64,
                    ArrayTag::F64 => JSONArrayType::F64,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod ivalue_tests {
    use super::*;
    use half::{bf16, f16};
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    macro_rules! assert_typed_array_indexing {
        ($($variant:ident => $primitive:ty : $values:expr),* $(,)?) => {
            $({
                let values: Vec<$primitive> = $values;
                let array = IValue::from(values.clone());
                assert_eq!(
                    array.get_array_type(),
                    Some(JSONArrayType::$variant),
                    "{} values should be stored as a packed array",
                    stringify!($variant)
                );
                for (index, value) in values.iter().enumerate() {
                    let element = array
                        .get_index(index)
                        .unwrap_or_else(|| panic!("{}[{index}] is missing", stringify!($variant)));
                    assert_eq!(
                        element.as_ref(),
                        &IValue::from(*value),
                        "{}[{index}]",
                        stringify!($variant)
                    );
                }
                assert!(
                    array.get_index(values.len()).is_none(),
                    "{}[{}] is out of bounds",
                    stringify!($variant),
                    values.len()
                );
            })*
        };
    }

    #[test]
    fn test_get_index_returns_elements_of_packed_arrays() {
        assert_typed_array_indexing! {
            I8 => i8 : vec![1i8, 2i8, 3i8],
            U8 => u8 : vec![1u8, 2u8, 3u8],
            I16 => i16 : vec![1000i16, 1001i16, 1002i16],
            U16 => u16 : vec![1000u16, 1001u16, 1002u16],
            F16 => f16 : vec![f16::from_f32(1.25), f16::from_f32(2.5)],
            BF16 => bf16 : vec![bf16::from_f32(1.25), bf16::from_f32(2.5)],
            I32 => i32 : vec![1_000_000i32, 2_000_000i32],
            U32 => u32 : vec![1_000_000u32, 2_000_000u32],
            F32 => f32 : vec![1.25f32, 2.5f32],
            I64 => i64 : vec![1i64 << 40, 2i64 << 40],
            U64 => u64 : vec![1u64 << 40, 2u64 << 40],
            F64 => f64 : vec![1.25f64, 2.5f64],
        }
    }

    #[test]
    fn test_get_index_borrows_elements_of_heterogeneous_arrays() {
        let array: IValue =
            serde_json::from_str(r#"["aaa", 1, null, true, [1, 2], {"a": 1}]"#).unwrap();
        assert_eq!(
            array.get_array_type(),
            Some(JSONArrayType::Heterogeneous),
            "mixed values should not be packed"
        );

        for index in 0..array.len().unwrap() {
            let element = array.get_index(index).unwrap();
            match element {
                ValueRef::Borrowed(borrowed) => {
                    assert!(std::ptr::eq(borrowed, &array.as_array().unwrap()[index]));
                }
                ValueRef::Owned(owned) => {
                    panic!("heterogeneous element {index} was cloned: {owned:?}")
                }
            }
        }
        assert!(array.get_index(array.len().unwrap()).is_none());
    }

    #[test]
    fn test_get_index_returns_none_for_empty_array_and_non_arrays() {
        let empty: IValue = serde_json::from_str("[]").unwrap();
        assert!(empty.get_index(0).is_none());

        for json in [r#"{"0": 1}"#, "1", "1.5", r#""aaa""#, "true", "null"] {
            let value: IValue = serde_json::from_str(json).unwrap();
            assert!(
                value.get_index(0).is_none(),
                "get_index on a non-array ({json}) should be None"
            );
        }
    }

    /// `get_index` must not walk the array: RediSearch reads vectors through the LLAPI
    /// one index at a time, so a linear `get_index` makes reading a `dim`-element vector
    /// O(dim^2). Compare the per-element cost of a full indexed pass over a short and a
    /// long array - it stays flat for O(1) access and grows with the length otherwise.
    #[test]
    fn test_get_index_cost_does_not_grow_with_array_length() {
        const SHORT: usize = 512;
        const LONG: usize = 8192;

        fn nanos_per_element(len: usize) -> f64 {
            let array = IValue::from((0..len).map(|i| i as f64 + 0.5).collect::<Vec<f64>>());
            assert_eq!(array.get_array_type(), Some(JSONArrayType::F64));

            // Take the fastest of a few passes, to blunt scheduling noise.
            let best = (0..5)
                .map(|_| {
                    let start = Instant::now();
                    for index in 0..len {
                        black_box(array.get_index(index));
                    }
                    start.elapsed()
                })
                .min()
                .unwrap_or(Duration::MAX);
            best.as_nanos() as f64 / len as f64
        }

        let short = nanos_per_element(SHORT);
        let long = nanos_per_element(LONG);
        // A linear `get_index` would make this ratio ~LONG/SHORT (16x); O(1) keeps it ~1.
        assert!(
            long < short * 4.0,
            "indexed access looks linear: {short:.1}ns/element over {SHORT} elements \
             vs {long:.1}ns/element over {LONG} elements"
        );
    }
}
