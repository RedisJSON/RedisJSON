use std::collections::HashSet;
use crate::select::select_value::{SelectValue, SelectValueType};

pub(super) struct ValueWalker;

impl<'a> ValueWalker{
    pub fn all_with_num<T>(vec: &[&'a T], tmp: &mut Vec<&'a T>, index: f64) where T: SelectValue {
        Self::walk(vec, tmp, &|v| if v.is_array() {
            if let Some(item) = v.get_index(index as usize) {
                Some(vec![&item])
            } else {
                None
            }
        } else {
            None
        });
    }

    pub fn all_with_str<T>(vec: &[&'a T], tmp: &mut Vec<&'a T>, key: &str, is_filter: bool) where T: SelectValue {
        if is_filter {
            Self::walk(vec, tmp, &|v| match v.get_type() {
                SelectValueType::Dict if v.contains_key(key) => Some(vec![v]),
                _ => None,
            });
        } else {
            Self::walk(vec, tmp, &|v| match v.get_type() {
                SelectValueType::Dict => match v.get_key(key) {
                    Some(v) => Some(vec![&v]),
                    _ => None,
                },
                _ => None,
            });
        }
    }

    pub fn all<T>(vec: &[&'a T], tmp: &mut Vec<&'a T>) where T: SelectValue {
        Self::walk(vec, tmp, &|v| v.values());
    }

    fn walk<F, T>(vec: &[&'a T], tmp: &mut Vec<&'a T>, fun: &F) where F: Fn(&'a T) -> Option<Vec<&'a T>>, T: SelectValue {
        for v in vec {
            Self::_walk(*v, tmp, fun);
        }
    }

    fn _walk<F, T>(v: &'a T, tmp: &mut Vec<&'a T>, fun: &F) where F: Fn(&'a T) -> Option<Vec<&'a T>>, T: SelectValue {
        if let Some(mut ret) = fun(v) {
            tmp.append(&mut ret);
        }

        match v.get_type() {
            SelectValueType::Dict | SelectValueType::Array => {
                for v in v.values().unwrap() {
                    Self::_walk(v, tmp, fun);
                }
            }
            _ => {}
        }
    }

    pub fn walk_dedup<T>(v: &'a T,
                      tmp: &mut Vec<&'a T>,
                      key: &str,
                      visited: &mut HashSet<*const T>) where T: SelectValue {
        match v.get_type() {
            SelectValueType::Dict => {
                if v.contains_key(key) {
                    let ptr = v as *const T;
                    if !visited.contains(&ptr) {
                        visited.insert(ptr);
                        tmp.push(v)
                    }
                }
            }
            SelectValueType::Array => {
                for v in v.values().unwrap() {
                    Self::walk_dedup(v, tmp, key, visited);
                }
            }
            _ => {}
        }
    }
}

