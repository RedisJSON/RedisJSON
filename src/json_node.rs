use crate::select::select_value::{SelectValue, SelectValueType, ValueUpdater};
use crate::select::JsonPathError;
use crate::select::SelectorMut;
use serde_json::map::Entry;
use serde_json::Value;

impl SelectValue for Value {
    fn get_type(&self) -> SelectValueType {
        match self {
            Value::Bool(_) => SelectValueType::Bool,
            Value::String(_) => SelectValueType::String,
            Value::Null => SelectValueType::Null,
            Value::Array(_) => SelectValueType::Array,
            Value::Object(_) => SelectValueType::Dict,
            Value::Number(n) => {
                if n.is_i64() {
                    SelectValueType::Long
                } else if n.is_u64() {
                    SelectValueType::Long
                } else if n.is_f64() {
                    SelectValueType::Double
                } else {
                    SelectValueType::Undef
                }
            }
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        match self {
            Value::Object(o) => o.contains_key(key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Vec<&'a Self>> {
        match self {
            Value::Array(arr) => {
                let mut res = Vec::new();
                for e in arr {
                    res.push(e);
                }
                Some(res)
            }
            Value::Object(o) => {
                let mut res = Vec::new();
                for e in o.values() {
                    res.push(e);
                }
                Some(res)
            }
            _ => None,
        }
    }

    fn keys(&self) -> Option<Vec<String>> {
        match self {
            Value::Object(o) => {
                let mut res = Vec::new();
                for k in o.keys() {
                    res.push(k.to_string());
                }
                Some(res)
            }
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self {
            Value::Array(arr) => Some(arr.len()),
            _ => None,
        }
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
        match self {
            Value::Object(o) => o.get(key),
            _ => None,
        }
    }

    fn get_index<'a>(&'a self, index: usize) -> Option<&'a Self> {
        match self {
            Value::Array(arr) => arr.get(index),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        match self {
            Value::Array(_) => true,
            _ => false,
        }
    }

    fn get_str(&self) -> String {
        match self {
            Value::String(s) => s.to_string(),
            _ => {
                assert!(false, "not a string");
                String::new()
            }
        }
    }

    fn get_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => {
                assert!(false, "not a bool");
                false
            }
        }
    }

    fn get_long(&self) -> i64 {
        match self {
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    n.as_i64().unwrap()
                } else {
                    assert!(false, "not a long");
                    0
                }
            }
            _ => {
                assert!(false, "not a long");
                0
            }
        }
    }

    fn get_double(&self) -> f64 {
        match self {
            Value::Number(n) => {
                if n.is_f64() {
                    n.as_f64().unwrap()
                } else {
                    assert!(false, "not a double");
                    0.1
                }
            }
            _ => {
                assert!(false, "not a double");
                0.1
            }
        }
    }
}

pub struct JsonValueUpdater<F: FnMut(Value) -> Option<Value>> {
    func: F,
}

impl<F> JsonValueUpdater<F>
where
    F: FnMut(Value) -> Option<Value>,
{
    pub fn new(func: F) -> JsonValueUpdater<F> {
        JsonValueUpdater { func: func }
    }
}

impl<F> ValueUpdater<Value> for JsonValueUpdater<F>
where
    F: FnMut(Value) -> Option<Value>,
{
    fn update(
        &mut self,
        mut path: Vec<String>,
        root: &mut Value,
    ) -> Result<&mut Self, JsonPathError> {
        let mut target = root;

        let last_index = path.len().saturating_sub(1);
        for (i, token) in path.drain(..).enumerate() {
            let target_once = target;
            let is_last = i == last_index;
            let target_opt = match *target_once {
                Value::Object(ref mut map) => {
                    if is_last {
                        if let Entry::Occupied(mut e) = map.entry(token) {
                            let v = e.insert(Value::Null);
                            if let Some(res) = (self.func)(v) {
                                e.insert(res);
                            } else {
                                e.remove();
                            }
                        }
                        return Ok(self);
                    }
                    map.get_mut(&token)
                }
                Value::Array(ref mut vec) => {
                    if let Ok(x) = token.parse::<usize>() {
                        if is_last {
                            let v = std::mem::replace(&mut vec[x], Value::Null);
                            if let Some(res) = (self.func)(v) {
                                vec[x] = res;
                            } else {
                                vec.remove(x);
                            }
                            return Ok(self);
                        }
                        vec.get_mut(x)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(t) = target_opt {
                target = t;
            } else {
                break;
            }
        }

        Ok(self)
    }
}

pub fn replace_with<'a, F>(
    value: &'a mut Value,
    path: &str,
    fun: &mut F,
) -> Result<SelectorMut<'a, Value>, JsonPathError>
where
    F: FnMut(Value) -> Option<Value>,
{
    let mut selector = SelectorMut::default();
    let mut updater = JsonValueUpdater::new(fun);
    selector
        .str_path(path)?
        .value(value)
        .replace_with(&mut updater)?;
    Ok(selector)
}
