//! The value module contains the `Value` enum, which is used to
//! represent JSON values.

use std::hash::{Hash, Hasher};

/// We use `FxHashMap` from the `fxhash` crate to store the key-value
/// pairs of JSON objects. This is because `FxHashMap` is faster than
/// `HashMap` for keys of length longer than five bytes, and shouldn't
/// be slower for keys shorter than that. Let's try to win some
/// performance here.
use fxhash::FxHashMap as Map;
use serde::Deserialize;
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
