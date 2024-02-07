//! The value module contains the `Value` enum, which is used to
//! represent JSON values.

use std::hash::{Hash, Hasher};

/// We use `FxHashMap` from the `fxhash` crate to store the key-value
/// pairs of JSON objects. This is because `FxHashMap` is faster than
/// `HashMap` for keys of length longer than five bytes, and shouldn't
/// be slower for keys shorter than that. Let's try to win some
/// performance here.
use fxhash::FxHashMap as Map;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeSeq},
    Deserialize, Serialize,
};
// TODO: consider `indexmap`, `hashbrown`.

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
pub enum Number {
    /// An unsigned integer value.
    Unsigned(u64),
    /// A signed integer value.
    Signed(i64),
    /// A floating-point value, represented as a double.
    Double(f64),
}

impl Hash for Number {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Number::Unsigned(n) => n.hash(state),
            Number::Signed(n) => n.hash(state),
            Number::Double(n) => {
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

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Number::Unsigned(a), Number::Unsigned(b)) => a == b,
            (Number::Signed(a), Number::Signed(b)) => a == b,
            (Number::Double(a), Number::Double(b)) => a == b,

            (Number::Unsigned(a), Number::Signed(b)) => (*a as i64) == *b,
            (Number::Unsigned(a), Number::Double(b)) => (*a as f64) == *b,

            (Number::Signed(a), Number::Unsigned(b)) => *a == (*b as i64),
            (Number::Signed(a), Number::Double(b)) => (*a as f64) == *b,

            (Number::Double(a), Number::Unsigned(b)) => *a == (*b as f64),
            (Number::Double(a), Number::Signed(b)) => *a == (*b as f64),
        }
    }
}

impl Eq for Number {}

impl From<serde_json::Number> for Number {
    fn from(n: serde_json::Number) -> Self {
        if let Some(u) = n.as_u64() {
            return Number::Unsigned(u);
        }
        if let Some(i) = n.as_i64() {
            return Number::Signed(i);
        }
        if let Some(f) = n.as_f64() {
            return Number::Double(f);
        }

        unreachable!("serde_json::Number is not a valid JSON number.")
    }
}

impl Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        match self {
            Number::Unsigned(u) => serializer.serialize_u64(*u),
            Number::Signed(i) => serializer.serialize_i64(*i),
            Number::Double(f) => serializer.serialize_f64(*f),
        }
    }
}

/// A destructured representation of a JSON value.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Value {
    /// Null.
    #[default]
    Null,
    /// Boolean.
    Bool(bool),
    /// Number.
    Number(Number),
    /// String.
    String(String),
    /// Array.
    Array(Vec<Self>),
    /// Object.
    Object(Map<String, Self>),
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
