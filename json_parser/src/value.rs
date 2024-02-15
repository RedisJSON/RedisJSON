//! The value module contains the `Value` enum, which is used to
//! represent JSON values.

use std::hash::{Hash, Hasher};

/// We use `FxHashMap` from the `fxhash` crate to store the key-value
/// pairs of JSON objects. This is because `FxHashMap` is faster than
/// `HashMap` for keys of length longer than five bytes, and shouldn't
/// be slower for keys shorter than that. Let's try to win some
/// performance here.
// TODO: consider `indexmap`, `hashbrown`.
pub use fxhash::FxHashMap as Map;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeSeq},
    Deserialize, Serialize,
};
/// The entry API for `FxHashMap`.
pub use std::collections::hash_map::Entry as MapEntry;
pub use Vec as Array;

/* serde_json::Value:

pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}
*/

/// Represents a JSON number, which can be unsigned, signed, or a
/// double.
/// See <https://www.w3schools.com/js/js_json_datatypes.asp>.
#[derive(Debug, Copy, Clone)]
pub enum JsonNumber {
    /// An unsigned integer value.
    Unsigned(u64),
    /// A signed integer value.
    Signed(i64),
    /// A floating-point value, represented as a double.
    Double(f64),
}

impl Hash for JsonNumber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            JsonNumber::Unsigned(n) => n.hash(state),
            JsonNumber::Signed(n) => n.hash(state),
            JsonNumber::Double(n) => {
                // Borrowed from serde.
                if *n == 0.0f64 {
                    // There are 2 zero representations, +0 and -0, which
                    // compare equal but have different bits. We use the +0 hash
                    // for both so that hash(+0) == hash(-0).
                    0.0f64.to_bits().hash(state);
                } else {
                    n.to_bits().hash(state);
                }
            }
        }
    }
}

impl PartialEq for JsonNumber {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JsonNumber::Unsigned(a), JsonNumber::Unsigned(b)) => a == b,
            (JsonNumber::Signed(a), JsonNumber::Signed(b)) => a == b,
            (JsonNumber::Double(a), JsonNumber::Double(b)) => a == b,

            (JsonNumber::Unsigned(a), JsonNumber::Signed(b)) => (*a as i64) == *b,
            (JsonNumber::Unsigned(a), JsonNumber::Double(b)) => (*a as f64) == *b,

            (JsonNumber::Signed(a), JsonNumber::Unsigned(b)) => *a == (*b as i64),
            (JsonNumber::Signed(a), JsonNumber::Double(b)) => (*a as f64) == *b,

            (JsonNumber::Double(a), JsonNumber::Unsigned(b)) => *a == (*b as f64),
            (JsonNumber::Double(a), JsonNumber::Signed(b)) => *a == (*b as f64),
        }
    }
}

impl Eq for JsonNumber {}

impl From<serde_json::Number> for JsonNumber {
    fn from(n: serde_json::Number) -> Self {
        if let Some(u) = n.as_u64() {
            return JsonNumber::Unsigned(u);
        }
        if let Some(i) = n.as_i64() {
            return JsonNumber::Signed(i);
        }
        if let Some(f) = n.as_f64() {
            return JsonNumber::Double(f);
        }

        unreachable!("serde_json::Number is not a valid JSON number.")
    }
}

impl Serialize for JsonNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        match self {
            JsonNumber::Unsigned(u) => serializer.serialize_u64(*u),
            JsonNumber::Signed(i) => serializer.serialize_i64(*i),
            JsonNumber::Double(f) => serializer.serialize_f64(*f),
        }
    }
}

impl JsonNumber {
    /// Returns the value as an unsigned integer, if possible.
    pub fn get_unsigned(&self) -> Option<u64> {
        match self {
            JsonNumber::Unsigned(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the value as a signed integer, if possible.
    pub fn get_signed(&self) -> Option<i64> {
        match self {
            JsonNumber::Signed(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the value as a double, if possible.
    pub fn get_double(&self) -> Option<f64> {
        match self {
            JsonNumber::Double(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns `true` if the value can hold a decimal point.
    pub fn has_decimal_point(&self) -> bool {
        matches!(self, JsonNumber::Double(_))
    }
}

macro_rules! impl_number_from_unsigned {
    ($($t:ty),*) => {
        $(
            impl From<$t> for JsonNumber {
                fn from(n: $t) -> Self {
                    JsonNumber::Unsigned(n as u64)
                }
            }

            impl TryFrom<JsonNumber> for $t {
                type Error = ();

                fn try_from(n: JsonNumber) -> Result<Self, Self::Error> {
                    match n {
                        JsonNumber::Unsigned(u) => Ok(u as $t),
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

macro_rules! impl_number_from_signed {
    ($($t:ty),*) => {
        $(
            impl From<$t> for JsonNumber {
                fn from(n: $t) -> Self {
                    JsonNumber::Signed(n as i64)
                }
            }

            impl TryFrom<JsonNumber> for $t {
                type Error = ();

                fn try_from(n: JsonNumber) -> Result<Self, Self::Error> {
                    match n {
                        JsonNumber::Signed(u) => Ok(u as $t),
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

macro_rules! impl_number_from_double {
    ($($t:ty),*) => {
        $(
            impl From<$t> for JsonNumber {
                fn from(n: $t) -> Self {
                    JsonNumber::Double(n as f64)
                }
            }

            impl TryFrom<JsonNumber> for $t {
                type Error = ();

                fn try_from(n: JsonNumber) -> Result<Self, Self::Error> {
                    match n {
                        JsonNumber::Double(u) => Ok(u as $t),
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

macro_rules! impl_number_numeric_methods {
    ($($t:ty),*) => {
        /// Numeric methods.
        impl JsonNumber {
            $(
                concat_idents::concat_idents!(fn_name = to_, $t {
                    pub fn fn_name(&self) -> Option<$t> {
                        $t::try_from(*self).ok()
                    }
                });
            )*
        }
    };
}

impl_number_from_unsigned!(usize, u64, u32, u16, u8);
impl_number_from_signed!(isize, i64, i32, i16, i8);
impl_number_from_double!(f64, f32);
impl_number_numeric_methods!(usize, u64, u32, u16, u8, isize, i64, i32, i16, i8, f64, f32);

/// The JsonString type is an alias for the `String` type, and is used
/// to represent JSON strings.
pub type JsonString = String;

/// A destructured representation of a JSON value.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Value {
    /// Null.
    #[default]
    Null,
    /// Boolean.
    Bool(bool),
    /// Number.
    Number(JsonNumber),
    /// String.
    String(JsonString),
    /// Array.
    Array(Vec<Self>),
    /// Object.
    Object(Map<String, Self>),
}

/// Basic enum methods.
impl Value {
    /// Returns `true` if the value holds a JSON null.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns the boolean value of this json value, if it is a
    /// boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An alias for [`Self::as_bool`].
    pub fn to_bool(&self) -> Option<bool> {
        self.as_bool()
    }

    /// Returns `true` if the value holds a JSON boolean.
    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    /// Returns the JSON number as a reference, if this [`Value`] is a
    /// number.
    pub fn as_number(&self) -> Option<&JsonNumber> {
        match self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Returns the JSON number as a mutable reference, if this
    /// [`Value`] is a number.
    pub fn as_number_mut(&mut self) -> Option<&mut JsonNumber> {
        match self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Returns `true` if the value holds a JSON number.
    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    /// Returns the JSON string as a reference, if this [`Value`] is a
    /// string.
    pub fn as_string(&self) -> Option<&JsonString> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the JSON string as a mutable reference, if this
    /// [`Value`] is a string.
    pub fn as_string_mut(&mut self) -> Option<&mut JsonString> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns `true` if the value holds a JSON string.
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    /// Returns the JSON array as a reference, if this [`Value`] is an
    /// array.
    pub fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            Self::Array(vec) => Some(vec),
            _ => None,
        }
    }

    /// Returns the JSON array as a mutable reference, if this
    /// [`Value`] is an array.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(vec) => Some(vec),
            _ => None,
        }
    }

    /// Returns `true` if the value holds a JSON array.
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    /// Returns `true` if the value holds a JSON object.
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Returns the JSON object as a map, if this [`Value`] is an
    /// object.
    pub fn as_object(&self) -> Option<&Map<String, Self>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Returns the JSON object as a map, if this [`Value`] is an
    /// object.
    pub fn as_object_mut(&mut self) -> Option<&mut Map<String, Self>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }
}

macro_rules! impl_value_numeric_methods {
    ($($t:ty),*) => {
        /// Numeric methods.
        impl Value {
            $(
                concat_idents::concat_idents!(fn_name = to_, $t {
                    pub fn fn_name(&self) -> Option<$t> {
                        match self {
                            Self::Number(n) => $t::try_from(*n).ok(),
                            _ => None,
                        }
                    }
                });
            )*
        }
    };
}

impl_value_numeric_methods!(usize, u64, u32, u16, u8, isize, i64, i32, i16, i8, f64, f32);

/// Additional useful methods.
impl Value {
    /// Takes the value out of this object, leaving a [`Self::Null`] in
    /// its place.
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, Self::Null)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Null
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<JsonString> for Value {
    fn from(s: JsonString) -> Self {
        Self::String(s)
    }
}

impl From<Vec<Value>> for Value {
    fn from(vec: Vec<Value>) -> Self {
        Self::Array(vec)
    }
}

impl From<Map<String, Value>> for Value {
    fn from(map: Map<String, Value>) -> Self {
        Self::Object(map)
    }
}

impl<T> From<T> for Value
where
    JsonNumber: From<T>,
{
    fn from(n: T) -> Self {
        Self::Number(n.into())
    }
}

impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => Self::Number(n.into()),
            serde_json::Value::String(s) => Self::String(s),
            serde_json::Value::Array(vec) => {
                // let mut vec = Vec::with_capacity(a.len());
                // for v in a {
                //     vec.push(v.into());
                // }
                Self::Array(vec.into_iter().map(|v| v.into()).collect())
            }
            serde_json::Value::Object(map) => {
                // let mut map = Map::default();
                // for (k, v) in o {
                //     map.insert(k, v.into());
                // }
                // Self::Object(map)
                Self::Object(map.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

// impl<'de> Deserialize<'de> for Value {
//     #[inline]
//     fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         struct ValueVisitor;

//         impl<'de> Visitor<'de> for ValueVisitor {
//             type Value = Value;

//             fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//                 formatter.write_str("any valid JSON value")
//             }

//             #[inline]
//             fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
//                 Ok(Value::Bool(value))
//             }

//             #[inline]
//             fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
//                 Ok(Value::Number(value.into()))
//             }

//             #[inline]
//             fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
//                 Ok(Value::Number(value.into()))
//             }

//             #[inline]
//             fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
//                 Ok(Number::from_f64(value).map_or(Value::Null, Value::Number))
//             }

//             #[inline]
//             fn visit_str<E>(self, value: &str) -> Result<Value, E>
//             where
//                 E: serde::de::Error,
//             {
//                 self.visit_string(String::from(value))
//             }

//             #[inline]
//             fn visit_string<E>(self, value: String) -> Result<Value, E> {
//                 Ok(Value::String(value))
//             }

//             #[inline]
//             fn visit_none<E>(self) -> Result<Value, E> {
//                 Ok(Value::Null)
//             }

//             #[inline]
//             fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
//             where
//                 D: serde::Deserializer<'de>,
//             {
//                 Deserialize::deserialize(deserializer)
//             }

//             #[inline]
//             fn visit_unit<E>(self) -> Result<Value, E> {
//                 Ok(Value::Null)
//             }

//             #[inline]
//             fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
//             where
//                 V: SeqAccess<'de>,
//             {
//                 let mut vec = Vec::new();

//                 while let Some(elem) = visitor.next_element()? {
//                     vec.push(elem);
//                 }

//                 Ok(Value::Array(vec))
//             }

//             // fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
//             // where
//             //     V: MapAccess<'de>,
//             // {
//             //     match visitor.next_key_seed(KeyClassifier)? {
//             //         #[cfg(feature = "arbitrary_precision")]
//             //         Some(KeyClass::Number) => {
//             //             let number: NumberFromString = tri!(visitor.next_value());
//             //             Ok(Value::Number(number.value))
//             //         }
//             //         #[cfg(feature = "raw_value")]
//             //         Some(KeyClass::RawValue) => {
//             //             let value = tri!(visitor.next_value_seed(crate::raw::BoxedFromString));
//             //             crate::from_str(value.get()).map_err(de::Error::custom)
//             //         }
//             //         Some(KeyClass::Map(first_key)) => {
//             //             let mut values = Map::new();

//             //             values.insert(first_key, tri!(visitor.next_value()));
//             //             while let Some((key, value)) = tri!(visitor.next_entry()) {
//             //                 values.insert(key, value);
//             //             }

//             //             Ok(Value::Object(values))
//             //         }
//             //         None => Ok(Value::Object(Map::new())),
//             //     }
//             // }
//         }

//         deserializer.deserialize_any(ValueVisitor)
//     }
// }

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(b) => serializer.serialize_bool(*b),
            Self::Number(n) => n.serialize(serializer),
            Self::String(s) => serializer.serialize_str(s),
            Self::Array(vec) => {
                let mut seq = serializer.serialize_seq(Some(vec.len()))?;
                for e in vec {
                    seq.serialize_element(e)?;
                }
                seq.end()
            }
            Self::Object(map) => {
                let mut map_ser = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    map_ser.serialize_entry(k, v)?;
                }
                map_ser.end()
            }
        }
    }
}

// impl Serialize for Value {
//     #[inline]
//     fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
//     where
//         S: ::serde::Serializer,
//     {
//         match self {
//             Value::Null => serializer.serialize_unit(),
//             Value::Bool(b) => serializer.serialize_bool(*b),
//             Value::Number(n) => n.serialize(serializer),
//             Value::String(s) => serializer.serialize_str(s),
//             Value::Array(v) => v.serialize(serializer),
//             #[cfg(any(feature = "std", feature = "alloc"))]
//             Value::Object(m) => {
//                 use serde::ser::SerializeMap;
//                 let mut map = tri!(serializer.serialize_map(Some(m.len())));
//                 for (k, v) in m {
//                     tri!(map.serialize_entry(k, v));
//                 }
//                 map.end()
//             }
//         }
//     }
// }

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // TODO: for now let's have just that, and then optimise
        // further later.
        Ok(serde_json::Value::deserialize(deserializer)?.into())
    }
}
