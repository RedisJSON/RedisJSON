use crate::select::JsonPathError;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SelectValueType {
    Undef,
    Null,
    Bool,
    Long,
    Double,
    String,
    Array,
    Dict,
}

pub trait SelectValue:
    std::fmt::Debug + std::cmp::Eq + std::cmp::PartialEq + std::default::Default + std::clone::Clone
{
    fn get_type(&self) -> SelectValueType;
    fn contains_key(&self, key: &str) -> bool;
    fn values<'a>(&'a self) -> Option<Vec<&'a Self>>;
    fn keys(&self) -> Option<Vec<String>>;
    fn len(&self) -> Option<usize>;
    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self>;
    fn get_index<'a>(&'a self, index: usize) -> Option<&'a Self>;
    fn is_array(&self) -> bool;

    fn get_str(&self) -> String;
    fn get_bool(&self) -> bool;
    fn get_long(&self) -> i64;
    fn get_double(&self) -> f64;
}

pub trait ValueUpdater<T: SelectValue> {
    fn update(&mut self, path: Vec<String>, root: &mut T) -> Result<&mut Self, JsonPathError>;
}
