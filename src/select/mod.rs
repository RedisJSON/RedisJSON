use std::collections::HashSet;
use std::fmt;

use jsonpath_lib::parser::*;

mod cmp;
mod expr_term;
mod value_walker;
pub mod select_value;

use self::expr_term::*;
use self::value_walker::ValueWalker;
use self::select_value::{SelectValue, SelectValueType, ValueUpdater};

fn to_f64(n: i64) -> f64 {
    n as f64
}

fn abs_index(n: isize, len: usize) -> usize {
    if n < 0_isize {
        (n + len as isize).max(0) as usize
    } else {
        n.min(len as isize) as usize
    }
}

#[derive(Debug, PartialEq)]
enum FilterKey {
    String(String),
    All,
}

pub enum JsonPathError {
    EmptyPath,
    EmptyValue,
    Path(String),
    Serde(String),
}

impl std::error::Error for JsonPathError {}

impl fmt::Debug for JsonPathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for JsonPathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            JsonPathError::EmptyPath => f.write_str("path not set"),
            JsonPathError::EmptyValue => f.write_str("json value not set"),
            JsonPathError::Path(msg) => f.write_str(&format!("path error: \n{}\n", msg)),
            JsonPathError::Serde(msg) => f.write_str(&format!("serde error: \n{}\n", msg)),
        }
    }
}

#[derive(Debug, Default)]
struct FilterTerms<'a, T>(Vec<Option<ExprTerm<'a, T>>>) where T: SelectValue ;

impl<'a, T> FilterTerms<'a, T> where T: SelectValue {
    fn new_filter_context(&mut self) {
        self.0.push(None);
        debug!("new_filter_context: {:?}", self.0);
    }

    fn is_term_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push_term(&mut self, term: Option<ExprTerm<'a, T>>) {
        self.0.push(term);
    }

    #[allow(clippy::option_option)]
    fn pop_term(&mut self) -> Option<Option<ExprTerm<'a, T>>> {
        self.0.pop()
    }

    fn filter_json_term<F: Fn(&Vec<&'a T>, &mut Vec<&'a T>, &mut HashSet<usize>) -> FilterKey>(
        &mut self,
        e: ExprTerm<'a, T>,
        fun: F,
    ) {
        debug!("filter_json_term: {:?}", e);

        if let ExprTerm::Json(rel, fk, vec) = e {
            let mut tmp = Vec::new();
            let mut not_matched = HashSet::new();
            let filter_key = if let Some(FilterKey::String(key)) = fk {
                let key_contained = &vec.iter().map(|v| match v.get_type() {
                    SelectValueType::Dict if v.contains_key(&key) => v.get_key(&key).unwrap(),
                    _ => v,
                }).collect();
                fun(key_contained, &mut tmp, &mut not_matched)
            } else {
                fun(&vec, &mut tmp, &mut not_matched)
            };

            if rel.is_some() {
                self.0.push(Some(ExprTerm::Json(rel, Some(filter_key), tmp)));
            } else {
                let filtered: Vec<&T> = vec.iter().enumerate()
                    .filter(
                        |(idx, _)| !not_matched.contains(idx)
                    )
                    .map(|(_, v)| *v)
                    .collect();

                self.0.push(Some(ExprTerm::Json(Some(filtered), Some(filter_key), tmp)));
            }
        } else {
            unreachable!("unexpected: ExprTerm: {:?}", e);
        }
    }

    fn push_json_term<F: Fn(&Vec<&'a T>, &mut Vec<&'a T>, &mut HashSet<usize>) -> FilterKey>(
        &mut self,
        current: &Option<Vec<&'a T>>,
        fun: F,
    ) {
        debug!("push_json_term: {:?}", &current);

        if let Some(current) = &current {
            let mut tmp = Vec::new();
            let mut not_matched = HashSet::new();
            let filter_key = fun(current, &mut tmp, &mut not_matched);
            self.0.push(Some(ExprTerm::Json(None, Some(filter_key), tmp)));
        }
    }

    fn filter<F: Fn(&Vec<&'a T>, &mut Vec<&'a T>, &mut HashSet<usize>) -> FilterKey>(
        &mut self,
        current: &Option<Vec<&'a T>>,
        fun: F,
    ) {
        if let Some(peek) = self.0.pop() {
            if let Some(e) = peek {
                self.filter_json_term(e, fun);
            } else {
                self.push_json_term(current, fun);
            }
        }
    }

    fn filter_all_with_str(&mut self, current: &Option<Vec<&'a T>>, key: &str) {
        self.filter(current, |vec, tmp, _| {
            ValueWalker::all_with_str(&vec, tmp, key, true);
            FilterKey::All
        });

        debug!("filter_all_with_str : {}, {:?}", key, self.0);
    }

    fn filter_next_with_str(&mut self, current: &Option<Vec<&'a T>>, key: &str) {
        self.filter(current, |vec, tmp, not_matched| {
            let mut visited = HashSet::new();
            for (idx, v) in vec.iter().enumerate() {
                match v.get_type() {
                    SelectValueType::Dict => {
                        if v.contains_key(key) {
                            let ptr = *v as *const T;
                            if !visited.contains(&ptr) {
                                visited.insert(ptr);
                                tmp.push(v)
                            }
                        } else {
                            not_matched.insert(idx);
                        }
                    }
                    SelectValueType::Array => {
                        not_matched.insert(idx);
                        for v1 in vec {
                            ValueWalker::walk_dedup(*v1, tmp, key, &mut visited);
                        }
                    }
                    _ => {
                        not_matched.insert(idx);
                    }
                }
            }

            FilterKey::String(key.to_owned())
        });

        debug!("filter_next_with_str : {}, {:?}", key, self.0);
    }

    fn collect_next_with_num(&mut self, current: &Option<Vec<&'a T>>, index: f64) -> Option<Vec<&'a T>> {
        fn _collect<'a, T>(tmp: &mut Vec<&'a T>, vec: & [&'a T], index: f64) {
            let index = abs_index(index as isize, vec.len());
            if let Some(v) = vec.get(index) {
                tmp.push(v);
            }
        }

        if let Some(current) = current {
            let mut tmp: Vec<&'a T> = Vec::new();
            for c in current {
                match c.get_type() {
                    SelectValueType::Dict => {
                        for k in c.keys().unwrap() {
                            if let Some(v) = c.get_key(&k) {
                                if v.get_type() == SelectValueType::Array{
                                    _collect(&mut tmp, &v.values().unwrap(), index);
                                }
                            }
                        }
                    }
                    SelectValueType::Array => {
                        _collect(&mut tmp, &c.values().unwrap(), index);
                    }
                    _ => {}
                }
            }

            if tmp.is_empty() {
                self.0.pop();
                return Some(vec![]);
            } else {
                return Some(tmp);
            }
        }

        debug!(
            "collect_next_with_num : {:?}, {:?}",
            &index, &current
        );

        None
    }

    fn collect_next_all(&mut self, current: &Option<Vec<&'a T>>) -> Option<Vec<&'a T>> {
        if let Some(current) = current {
            let mut tmp = Vec::new();
            for c in current {
                match c.get_type() {
                    SelectValueType::Dict => {
                        for v in c.values().unwrap() {
                            tmp.push(v)
                        }
                    }
                    SelectValueType::Array => {
                        for v in c.values().unwrap() {
                            tmp.push(v);
                        }
                    }
                    _ => {}
                }
            }
            return Some(tmp);
        }

        debug!("collect_next_all : {:?}", &current);

        None
    }

    fn collect_next_with_str(&mut self, current: &Option<Vec<&'a T>>, keys: &[String]) -> Option<Vec<&'a T>> {
        if let Some(current) = current {
            let mut tmp:Vec<&'a T> = Vec::new();
            for c in current {
                if c.get_type() == SelectValueType::Dict {
                    for key in keys {
                        if let Some(v) = c.get_key(&key) {
                            tmp.push(&v)
                        }
                    }
                }
            }

            if tmp.is_empty() {
                self.0.pop();
                return Some(vec![]);
            } else {
                return Some(tmp);
            }
        }

        debug!(
            "collect_next_with_str : {:?}, {:?}",
            keys, &current
        );

        None
    }

    fn collect_all(&mut self, current: &Option<Vec<&'a T>>) -> Option<Vec<&'a T>> {
        if let Some(current) = current {
            let mut tmp = Vec::new();
            ValueWalker::all(&current, &mut tmp);
            return Some(tmp);
        }
        debug!("collect_all: {:?}", &current);

        None
    }

    fn collect_all_with_str(&mut self, current: &Option<Vec<&'a T>>, key: &str) -> Option<Vec<&'a T>> {
        if let Some(current) = current {
            let mut tmp = Vec::new();
            ValueWalker::all_with_str(&current, &mut tmp, key, false);
            return Some(tmp);
        }

        debug!("collect_all_with_str: {}, {:?}", key, &current);

        None
    }

    fn collect_all_with_num(&mut self, current: &Option<Vec<&'a T>>, index: f64) -> Option<Vec<&'a T>> {
        if let Some(current) = current {
            let mut tmp = Vec::new();
            ValueWalker::all_with_num(&current, &mut tmp, index);
            return Some(tmp);
        }

        debug!("collect_all_with_num: {}, {:?}", index, &current);

        None
    }
}

#[derive(Default, Debug)]
pub struct Selector<'a, 'b, T> where T: SelectValue {
    node: Option<Node>,
    node_ref: Option<&'b Node>,
    value: Option<&'a T>,
    tokens: Vec<ParseToken>,
    current: Option<Vec<&'a T>>,
    selectors: Vec<Selector<'a, 'b, T>>,
    selector_filter: FilterTerms<'a, T>,
}

impl<'a, 'b, T> Selector<'a, 'b, T> where T: SelectValue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn str_path(&mut self, path: &str) -> Result<&mut Self, JsonPathError> {
        debug!("path : {}", path);
        self.node_ref.take();
        self.node = Some(Parser::compile(path).map_err(JsonPathError::Path)?);
        Ok(self)
    }

    // pub fn node_ref(&self) -> Option<&Node> {
    //     if let Some(node) = &self.node {
    //         return Some(node);
    //     }

    //     if let Some(node) = &self.node_ref {
    //         return Some(*node);
    //     }

    //     None
    // }

    pub fn compiled_path(&mut self, node: &'b Node) -> &mut Self {
        self.node.take();
        self.node_ref = Some(node);
        self
    }

    // pub fn reset_value(&mut self) -> &mut Self {
    //     self.current = None;
    //     self
    // }

    pub fn value(&mut self, v: &'a T) -> &mut Self {
        self.value = Some(v);
        self
    }

    fn _select(&mut self) -> Result<(), JsonPathError> {
        if self.node_ref.is_some() {
            let node_ref = self.node_ref.take().unwrap();
            self.visit(node_ref);
            return Ok(());
        }

        if self.node.is_none() {
            return Err(JsonPathError::EmptyPath);
        }

        let node = self.node.take().unwrap();
        self.visit(&node);
        self.node = Some(node);

        Ok(())
    }

    // pub fn select_as<R: serde::de::DeserializeOwned>(&mut self) -> Result<Vec<R>, JsonPathError> {
    //     self._select()?;

    //     match &self.current {
    //         Some(vec) => {
    //             let mut ret = Vec::new();
    //             for v in vec {
    //                 match R::deserialize(*v) {
    //                     Ok(v) => ret.push(v),
    //                     Err(e) => return Err(JsonPathError::Serde(e.to_string())),
    //                 }
    //             }
    //             Ok(ret)
    //         }
    //         _ => Err(JsonPathError::EmptyValue),
    //     }
    // }

    // pub fn select_as_str(&mut self) -> Result<String, JsonPathError> {
    //     self._select()?;

    //     match &self.current {
    //         Some(r) => {
    //             Ok(serde_json::to_string(r).map_err(|e| JsonPathError::Serde(e.to_string()))?)
    //         }
    //         _ => Err(JsonPathError::EmptyValue),
    //     }
    // }

    pub fn select(&mut self) -> Result<Vec<&'a T>, JsonPathError> {
        self._select()?;

        match &self.current {
            Some(r) => Ok(r.to_vec()),
            _ => Err(JsonPathError::EmptyValue),
        }
    }

    fn compute_absolute_path_filter(&mut self, token: &ParseToken) -> bool {
        if !self.selectors.is_empty() {
            match token {
                ParseToken::Absolute | ParseToken::Relative | ParseToken::Filter(_) => {
                    let selector = self.selectors.pop().unwrap();

                    if let Some(current) = &selector.current {
                        let term = current.into();

                        if let Some(s) = self.selectors.last_mut() {
                            s.selector_filter.push_term(Some(term));
                        } else {
                            self.selector_filter.push_term(Some(term));
                        }
                    } else {
                        unreachable!()
                    }
                }
                _ => {}
            }
        }

        if let Some(selector) = self.selectors.last_mut() {
            selector.visit_token(token);
            true
        } else {
            false
        }
    }
}

impl<'a, 'b, T> Selector<'a, 'b, T> where T: SelectValue {
    fn visit_absolute(&mut self) {
        if self.current.is_some() {
            let mut selector = Selector::default();

            if let Some(value) = self.value {
                selector.value = Some(value);
                selector.current = Some(vec![value]);
                self.selectors.push(selector);
            }
            return;
        }

        if let Some(v) = &self.value {
            self.current = Some(vec![v]);
        }
    }

    fn visit_relative(&mut self) {
        if let Some(ParseToken::Array) = self.tokens.last() {
            let array_token = self.tokens.pop();
            if let Some(ParseToken::Leaves) = self.tokens.last() {
                self.tokens.pop();
                self.current = self.selector_filter.collect_all(&self.current);
            }
            self.tokens.push(array_token.unwrap());
        }
        self.selector_filter.new_filter_context();
    }

    fn visit_array_eof(&mut self) {
        if self.is_last_before_token_match(ParseToken::Array) {
            if let Some(Some(e)) = self.selector_filter.pop_term() {
                if let ExprTerm::String(key) = e {
                    self.selector_filter.filter_next_with_str(&self.current, &key);
                    self.tokens.pop();
                    return;
                }

                self.selector_filter.push_term(Some(e));
            }
        }

        if self.is_last_before_token_match(ParseToken::Leaves) {
            self.tokens.pop();
            self.tokens.pop();
            if let Some(Some(e)) = self.selector_filter.pop_term() {
                let selector_filter_consumed = match &e {
                    ExprTerm::Long(n) => {
                        self.current = self.selector_filter.collect_all_with_num(&self.current, to_f64(*n));
                        self.selector_filter.pop_term();
                        true
                    }
                    ExprTerm::Double(n) => {
                        self.current = self.selector_filter.collect_all_with_num(&self.current, *n);
                        self.selector_filter.pop_term();
                        true
                    }
                    ExprTerm::String(key) => {
                        self.current = self.selector_filter.collect_all_with_str(&self.current, key);
                        self.selector_filter.pop_term();
                        true
                    }
                    _ => {
                        self.selector_filter.push_term(Some(e));
                        false
                    }
                };

                if selector_filter_consumed {
                    return;
                }
            }
        }

        if let Some(Some(e)) = self.selector_filter.pop_term() {
            match e {
                ExprTerm::Long(n) => {
                    self.current = self.selector_filter.collect_next_with_num(&self.current, to_f64(n));
                }
                ExprTerm::Double(n) => {
                    self.current = self.selector_filter.collect_next_with_num(&self.current, n);
                }
                ExprTerm::String(key) => {
                    self.current = self.selector_filter.collect_next_with_str(&self.current, &[key]);
                }
                ExprTerm::Json(rel, _, v) => {
                    if v.is_empty() {
                        self.current = Some(vec![]);
                    } else if let Some(vec) = rel {
                        self.current = Some(vec);
                    } else {
                        self.current = Some(v);
                    }
                }
                ExprTerm::Bool(false) => {
                    self.current = Some(vec![]);
                }
                _ => {}
            }
        }

        self.tokens.pop();
    }

    fn is_last_before_token_match(&mut self, token: ParseToken) -> bool {
        if self.tokens.len() > 1 {
            return token == self.tokens[self.tokens.len() - 2];
        }

        false
    }

    fn visit_all(&mut self) {
        if let Some(ParseToken::Array) = self.tokens.last() {
            self.tokens.pop();
        }

        match self.tokens.last() {
            Some(ParseToken::Leaves) => {
                self.tokens.pop();
                self.current = self.selector_filter.collect_all(&self.current);
            }
            Some(ParseToken::In) => {
                self.tokens.pop();
                self.current = self.selector_filter.collect_next_all(&self.current);
            }
            _ => {
                self.current = self.selector_filter.collect_next_all(&self.current);
            }
        }
    }

    fn visit_key(&mut self, key: &str) {
        if let Some(ParseToken::Array) = self.tokens.last() {
            self.selector_filter.push_term(Some(ExprTerm::String(key.to_string())));
            return;
        }

        if let Some(t) = self.tokens.pop() {
            if self.selector_filter.is_term_empty() {
                match t {
                    ParseToken::Leaves => {
                        self.current = self.selector_filter.collect_all_with_str(&self.current, key)
                    }
                    ParseToken::In => {
                        self.current = self.selector_filter.collect_next_with_str(&self.current, &[key.to_string()])
                    }
                    _ => {}
                }
            } else {
                match t {
                    ParseToken::Leaves => {
                        self.selector_filter.filter_all_with_str(&self.current, key);
                    }
                    ParseToken::In => {
                        self.selector_filter.filter_next_with_str(&self.current, key);
                    }
                    _ => {}
                }
            }
        }
    }

    fn visit_keys(&mut self, keys: &[String]) {
        if !self.selector_filter.is_term_empty() {
            unimplemented!("keys in filter");
        }

        if let Some(ParseToken::Array) = self.tokens.pop() {
            self.current = self.selector_filter.collect_next_with_str(&self.current, keys);
        } else {
            unreachable!();
        }
    }

    fn visit_filter(&mut self, ft: &FilterToken) {
        let right = match self.selector_filter.pop_term() {
            Some(Some(right)) => right,
            Some(None) => ExprTerm::Json(
                None,
                None,
                match &self.current {
                    Some(current) => current.to_vec(),
                    _ => unreachable!(),
                },
            ),
            _ => panic!("empty term right"),
        };

        let left = match self.selector_filter.pop_term() {
            Some(Some(left)) => left,
            Some(None) => ExprTerm::Json(
                None,
                None,
                match &self.current {
                    Some(current) => current.to_vec(),
                    _ => unreachable!(),
                },
            ),
            _ => panic!("empty term left"),
        };

        let mut ret = None;
        match ft {
            FilterToken::Equal => left.eq(&right, &mut ret),
            FilterToken::NotEqual => left.ne(&right, &mut ret),
            FilterToken::Greater => left.gt(&right, &mut ret),
            FilterToken::GreaterOrEqual => left.ge(&right, &mut ret),
            FilterToken::Little => left.lt(&right, &mut ret),
            FilterToken::LittleOrEqual => left.le(&right, &mut ret),
            FilterToken::And => left.and(&right, &mut ret),
            FilterToken::Or => left.or(&right, &mut ret),
        };

        if let Some(e) = ret {
            self.selector_filter.push_term(Some(e));
        }
    }

    fn visit_range(&mut self, from: &Option<isize>, to: &Option<isize>, step: &Option<usize>) {
        if !self.selector_filter.is_term_empty() {
            unimplemented!("range syntax in filter");
        }

        if let Some(ParseToken::Array) = self.tokens.pop() {
            let mut tmp:Vec<&'a T> = Vec::new();
            if let Some(current) = &self.current {
                for v in current {
                    if v.get_type() == SelectValueType::Array {
                        let from = if let Some(from) = from {
                            abs_index(*from, v.len().unwrap())
                        } else {
                            0
                        };

                        let to = if let Some(to) = to {
                            abs_index(*to, v.len().unwrap())
                        } else {
                            v.len().unwrap()
                        };

                        for i in (from..to).step_by(match step {
                            Some(step) => *step,
                            _ => 1,
                        }) {
                            if let Some(v) = v.get_index(i) {
                                tmp.push(&v);
                            }
                        }
                    }
                }
            }
            self.current = Some(tmp);
        } else {
            unreachable!();
        }
    }

    fn visit_union(&mut self, indices: &[isize]) {
        if !self.selector_filter.is_term_empty() {
            unimplemented!("union syntax in filter");
        }

        if let Some(ParseToken::Array) = self.tokens.pop() {
            let mut tmp:Vec<&'a T> = Vec::new();
            if let Some(current) = &self.current {
                for v in current {
                    if v.get_type() == SelectValueType::Array {
                        for i in indices {
                            if let Some(v) = v.get_index(abs_index(*i, v.len().unwrap())) {
                                tmp.push(&v);
                            }
                        }
                    }
                }
            }

            self.current = Some(tmp);
        } else {
            unreachable!();
        }
    }
}

impl<'a, 'b, T> NodeVisitor for Selector<'a, 'b, T> where T: SelectValue{
    fn visit_token(&mut self, token: &ParseToken) {
        debug!("token: {:?}, stack: {:?}", token, self.tokens);

        if self.compute_absolute_path_filter(token) {
            return;
        }

        match token {
            ParseToken::Absolute => self.visit_absolute(),
            ParseToken::Relative => self.visit_relative(),
            ParseToken::In | ParseToken::Leaves | ParseToken::Array => {
                self.tokens.push(token.clone());
            }
            ParseToken::ArrayEof => self.visit_array_eof(),
            ParseToken::All => self.visit_all(),
            ParseToken::Bool(b) => {
                self.selector_filter.push_term(Some(ExprTerm::Bool(*b)));
            }
            ParseToken::Key(key) => self.visit_key(key),
            ParseToken::Keys(keys) => self.visit_keys(keys),
            ParseToken::Number(v) => {
                self.selector_filter.push_term(Some(ExprTerm::Double(*v)));
            }
            ParseToken::Filter(ref ft) => self.visit_filter(ft),
            ParseToken::Range(from, to, step) => self.visit_range(from, to, step),
            ParseToken::Union(indices) => self.visit_union(indices),
            ParseToken::Eof => {
                debug!("visit_token eof");
            }
        }
    }
}

#[derive(Default)]
pub struct SelectorMut<'a, T: SelectValue> {
    path: Option<Node>,
    value: Option<&'a mut T>,
}

impl<'a, T> SelectorMut<'a, T> where T: SelectValue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn str_path(&mut self, path: &str) -> Result<&mut Self, JsonPathError> {
        self.path = Some(Parser::compile(path).map_err(JsonPathError::Path)?);
        Ok(self)
    }

    pub fn value(&mut self, value:&'a mut T) -> &mut Self {
        self.value = Some(value);
        self
    }

    fn compute_paths(&self, mut result: Vec<&T>) -> Vec<Vec<String>> {
        fn _walk<T: SelectValue>(
            origin: &T,
            target: &mut Vec<&T>,
            tokens: &mut Vec<String>,
            visited: &mut HashSet<*const T>,
            visited_order: &mut Vec<Vec<String>>,
        ) -> bool {

            if target.is_empty() {
                return true;
            }

            target.retain(|t| {
                if std::ptr::eq(origin, *t) {
                    if visited.insert(*t) {
                        visited_order.push(tokens.to_vec());
                    }
                    false
                } else {
                    true
                }
            });

            match origin.get_type() {
                SelectValueType::Array => {
                    for (i, v) in origin.values().unwrap().iter().enumerate() {
                        tokens.push(i.to_string());
                        if _walk(*v, target, tokens, visited, visited_order) {
                            return true;
                        }
                        tokens.pop();
                    }
                }
                SelectValueType::Dict => {
                    for k in origin.keys().unwrap() {
                        tokens.push(k.clone());
                        if _walk(origin.get_key(&k).unwrap(), target, tokens, visited, visited_order) {
                            return true;
                        }
                        tokens.pop();
                    }
                }
                _ => {}
            }

            false
        }

        let mut visited = HashSet::new();
        let mut visited_order = Vec::new();

        if let Some(origin) = self.value.as_deref() {
            let mut tokens = Vec::new();
            _walk(
                origin,
                &mut result,
                &mut tokens,
                &mut visited,
                &mut visited_order,
            );
        }

        visited_order
    }

    // pub fn delete(&mut self) -> Result<&mut Self, JsonPathError> {
    //     // self.replace_with(&mut |_| Some(Value::Null))
    //     self.replace_with(&mut |_| None)
    // }

    // pub fn remove(&mut self) -> Result<&mut Self, JsonPathError> {
    //     self.replace_with(&mut |_| None)
    // }

    fn select(&self) -> Result<Vec<& T>, JsonPathError> {
        if let Some(node) = &self.path {
            let mut selector = Selector::default();
            selector.compiled_path(&node);

            if let Some(v) = self.value.as_deref() {
                selector.value(v);
            }

            Ok(selector.select()?)
        } else {
            Err(JsonPathError::EmptyPath)
        }
    }

    pub fn replace_with<R: ValueUpdater<T>>(
        &mut self,
        updater: &mut R,
    ) -> Result<&mut Self, JsonPathError> {
        let paths = {
            let result = self.select()?;
            self.compute_paths(result)
        };

        if let Some(v) = self.value.as_deref_mut() {
            for tokens in paths {
                updater.update(tokens, v)?;
                // replace_value(tokens, v, fun);
            }
        }

        Ok(self)
    }
}


// #[cfg(test)]
// mod select_inner_tests {
//     use serde_json::Value;

//     #[test]
//     fn to_f64_i64() {
//         let number = 0_i64;
//         let v: Value = serde_json::from_str(&format!("{}", number)).unwrap();
//         if let Value::Number(n) = v {
//             assert_eq!((super::to_f64(&n) - number as f64).abs() == 0_f64, true);
//         } else {
//             panic!();
//         }
//     }

//     #[test]
//     fn to_f64_f64() {
//         let number = 0.1_f64;
//         let v: Value = serde_json::from_str(&format!("{}", number)).unwrap();
//         if let Value::Number(n) = v {
//             assert_eq!((super::to_f64(&n) - number).abs() == 0_f64, true);
//         } else {
//             panic!();
//         }
//     }

//     #[test]
//     fn to_f64_u64() {
//         let number = u64::max_value();
//         let v: Value = serde_json::from_str(&format!("{}", number)).unwrap();
//         if let Value::Number(n) = v {
//             assert_eq!((super::to_f64(&n) - number as f64).abs() == 0_f64, true);
//         } else {
//             panic!();
//         }
//     }
// }