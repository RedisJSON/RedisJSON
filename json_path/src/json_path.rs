/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use crate::select_value::{SelectValue, SelectValueType, ValueRef};
use itertools::Itertools;
use log::trace;
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;
use redis_module::rediserror::RedisError;
use regex::Regex;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::Debug;

// Macro to handle items() iterator for both Borrowed and Owned ValueRef cases
macro_rules! value_ref_items {
    ($value_ref:expr) => {{
        match $value_ref {
            ValueRef::Borrowed(borrowed_val) => {
                // For borrowed values, convert keys to owned for consistent return type.
                // Empty when `get_type` and `items()` disagree (defensive).
                let collected = borrowed_val
                    .items()
                    .map(|iter| iter.map(|(k, v)| (Cow::Borrowed(k), v)).collect_vec())
                    .unwrap_or_default();
                Box::new(collected.into_iter())
                    as Box<dyn Iterator<Item = (Cow<'_, str>, ValueRef<'_, S>)>>
            }
            ValueRef::Owned(owned_val) => {
                // For owned values, collect first to avoid lifetime issues
                let collected = owned_val
                    .items()
                    .map(|iter| {
                        iter.map(|(k, v)| {
                            (Cow::Owned(k.to_string()), ValueRef::Owned(v.inner_cloned()))
                        })
                        .collect_vec()
                    })
                    .unwrap_or_default();
                Box::new(collected.into_iter())
                    as Box<dyn Iterator<Item = (Cow<'_, str>, ValueRef<'_, S>)>>
            }
        }
    }};
}

// Macro to handle values() iterator for both Borrowed and Owned ValueRef cases
macro_rules! value_ref_values {
    ($value_ref:expr) => {{
        match $value_ref {
            ValueRef::Borrowed(borrowed_val) => {
                // Empty iterator when not a container; filter branch uses `get_type` but values may be absent.
                match borrowed_val.values() {
                    Some(iter) => Box::new(iter) as Box<dyn Iterator<Item = ValueRef<'_, S>>>,
                    None => Box::new(std::iter::empty()) as Box<dyn Iterator<Item = ValueRef<'_, S>>>,
                }
            }
            ValueRef::Owned(owned_val) => {
                let collected = owned_val
                    .values()
                    .map(|iter| iter.map(|v| ValueRef::Owned(v.inner_cloned())).collect_vec())
                    .unwrap_or_default();
                Box::new(collected.into_iter()) as Box<dyn Iterator<Item = ValueRef<'_, S>>>
            }
        }
    }};
}

macro_rules! value_ref_get_key {
    ($value_ref:expr, $curr:expr) => {{
        match &$value_ref {
            ValueRef::Borrowed(v) => v.get_key($curr),
            ValueRef::Owned(v) => v.get_key($curr).map(|v| ValueRef::Owned(v.inner_cloned())),
        }
    }};
}

macro_rules! value_ref_get_index {
    ($value_ref:expr, $i:expr) => {{
        match &$value_ref {
            ValueRef::Borrowed(v) => v.get_index($i),
            ValueRef::Owned(v) => v.get_index($i).map(|v| ValueRef::Owned(v.inner_cloned())),
        }
    }};
}

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct JsonPathParser;

#[derive(Debug, PartialEq, Eq)]
pub enum JsonPathToken {
    String,
    Number,
}

/* Struct that represent a compiled json path query. */
#[derive(Debug)]
pub struct Query<'i> {
    // query: QueryElement<'i>
    pub root: Pairs<'i, Rule>,
    is_static: Option<bool>,
    size: Option<usize>,
}

#[derive(Debug)]
pub struct QueryCompilationError {
    location: usize,
    message: String,
}

impl From<QueryCompilationError> for RedisError {
    fn from(e: QueryCompilationError) -> Self {
        Self::String(e.to_string())
    }
}

impl<'i> Query<'i> {
    /// Pop the last element from the compiled json path.
    /// For example, if the json path is $.foo.bar then `pop_last`
    /// will return bar and leave the json path query with $.foo
    #[allow(dead_code)]
    pub fn pop_last(&mut self) -> Option<(String, JsonPathToken)> {
        self.root.next_back().and_then(|last| match last.as_rule() {
            Rule::literal => Some((last.as_str().to_string(), JsonPathToken::String)),
            Rule::number => Some((last.as_str().to_string(), JsonPathToken::Number)),
            Rule::numbers_list => last.into_inner().next().map(|rule| {
                let stringified = rule.as_str().to_string();
                (stringified, JsonPathToken::Number)
            }),
            Rule::string_list => last.into_inner().next().map(|rule| {
                let unescaped = unescape_string_value(rule).into_owned();
                (unescaped, JsonPathToken::String)
            }),
            _ => None,
        })
    }

    /// Returns the amount of elements in the json path
    /// Example: $.foo.bar has 2 elements
    #[allow(dead_code)]
    pub fn size(&mut self) -> usize {
        if self.size.is_none() {
            self.is_static();
        }
        self.size.unwrap_or(0)
    }

    /// Returns whether the compiled json path is static
    /// A static path is a path that is promised to have at most a single result.
    /// Example:
    ///     static path: $.foo.bar
    ///     non-static path: $.*.bar
    #[allow(dead_code)]
    pub fn is_static(&mut self) -> bool {
        if let Some(b) = self.is_static {
            return b;
        }
        let mut size = 0;
        let mut is_static = true;
        let root_copy = self.root.clone();
        for n in root_copy {
            size += 1;
            match n.as_rule() {
                Rule::literal | Rule::number => continue,
                Rule::numbers_list | Rule::string_list => {
                    let inner = n.into_inner();
                    if inner.count() > 1 {
                        is_static = false;
                    }
                }
                _ => is_static = false,
            }
        }
        self.size = Some(size);
        self.is_static = Some(is_static);
        is_static
    }
}

impl std::fmt::Display for QueryCompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Error occurred at position {}, {}",
            self.location, self.message
        )
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::literal => write!(f, "<string>"),
            Self::all => write!(f, "'*'"),
            Self::full_scan => write!(f, "'..'"),
            Self::numbers_list => write!(f, "'<number>[,<number>,...]'"),
            Self::string_list => write!(f, "'<string>[,<string>,...]'"),
            Self::numbers_range => write!(f, "['start:end:steps']"),
            Self::number => write!(f, "'<number>'"),
            Self::filter => write!(f, "'[?(filter_expression)]'"),
            _ => write!(f, "{self:?}"),
        }
    }
}

fn unescape_string_value<'a>(pair: Pair<'a, Rule>) -> Cow<'a, str> {
    let s = pair.as_str();
    match pair.as_rule() {
        Rule::string_value => Cow::Borrowed(s),
        Rule::string_value_escape_1 => Cow::Owned(s.replace("\\\\", "\\").replace("\\\"", "\"")),
        Rule::string_value_escape_2 => Cow::Owned(s.replace("\\\\", "\\").replace("\\'", "'")),
        other => unreachable!(
            "unescape_string_value: unexpected rule {:?} (expected string leaf rule)",
            other
        ),
    }
}

/// Compile the given string query into a query object.
/// Returns error on compilation error.
pub(crate) fn compile(path: &str) -> Result<Query<'_>, QueryCompilationError> {
    let query = JsonPathParser::parse(Rule::query, path);
    match query {
        Ok(mut q) => {
            let root = q.next().ok_or_else(|| QueryCompilationError {
                location: 0,
                message: "internal: empty JSONPath parse result".to_string(),
            })?;
            Ok(Query {
                root: root.into_inner(),
                is_static: None,
                size: None,
            })
        }
        // pest::error::Error
        Err(e) => {
            let location = match e.location {
                pest::error::InputLocation::Pos(pos) => pos,
                pest::error::InputLocation::Span((pos, _end)) => pos,
            };
            let msg = match &e.variant {
                pest::error::ErrorVariant::ParsingError {
                    positives,
                    negatives,
                } => {
                    let p = positives.iter().join(", ");
                    let n = negatives.iter().join(", ");
                    match (p.len(), n.len()) {
                        (0, 0) => "parsing error".to_string(),
                        (_, 0) => format!("expected one of the following: {p}"),
                        (0, _) => format!("unexpected tokens found: {n}"),
                        (_, _) => format!(
                            "expected one of the following: {p}, unexpected tokens found: {n}"
                        ),
                    }
                }
                pest::error::ErrorVariant::CustomError { message } => message.clone(),
            };

            let message = format!("Error at position {}: {}", location, msg);
            Err(QueryCompilationError { location, message })
        }
    }
}

pub trait UserPathTracker {
    fn add_str(&mut self, s: &str);
    fn add_index(&mut self, i: usize);
    fn to_string_path(self) -> Vec<String>;
}

pub trait UserPathTrackerGenerator {
    type PT: UserPathTracker;
    fn generate(&self) -> Self::PT;
}

/* Dummy path tracker, indicating that there is no need to track results paths. */
pub struct DummyTracker;

impl UserPathTracker for DummyTracker {
    fn add_str(&mut self, _s: &str) {}
    fn add_index(&mut self, _i: usize) {}
    fn to_string_path(self) -> Vec<String> {
        Vec::new()
    }
}

/* A dummy path tracker generator, indicating that there is no need to track results paths. */
pub struct DummyTrackerGenerator;

impl UserPathTrackerGenerator for DummyTrackerGenerator {
    type PT = DummyTracker;
    fn generate(&self) -> Self::PT {
        DummyTracker
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub enum PTrackerElement {
    Key(String),
    Index(usize),
}

#[allow(dead_code)]
/* An actual representation of a path that the user gets as a result. */
#[derive(Debug, PartialEq, Eq)]
pub struct PTracker {
    pub elements: Vec<PTrackerElement>,
}

impl UserPathTracker for PTracker {
    fn add_str(&mut self, s: &str) {
        self.elements.push(PTrackerElement::Key(s.to_string()));
    }

    fn add_index(&mut self, i: usize) {
        self.elements.push(PTrackerElement::Index(i));
    }

    fn to_string_path(self) -> Vec<String> {
        self.elements
            .into_iter()
            .map(|e| match e {
                PTrackerElement::Key(s) => s,
                PTrackerElement::Index(i) => i.to_string(),
            })
            .collect()
    }
}

#[allow(dead_code)]
/* Used to generate paths trackers. */
pub struct PTrackerGenerator;

impl UserPathTrackerGenerator for PTrackerGenerator {
    type PT = PTracker;
    fn generate(&self) -> Self::PT {
        PTracker {
            elements: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum PathTrackerElement<'i> {
    Index(usize),
    Key(Cow<'i, str>),
    Root,
}

/* Struct that used to track paths of query results.
 * This struct is used to hold the path that lead to the
 * current location (when calculating the json path).
 * Once we have a match we can run (in a reverse order)
 * on the path tracker and add the path to the result as
 * a PTracker object. */
#[derive(Clone, Debug)]
struct PathTracker<'i, 'j> {
    parent: Option<&'j PathTracker<'i, 'j>>,
    element: PathTrackerElement<'i>,
}

const fn create_empty_tracker<'i, 'j>() -> PathTracker<'i, 'j> {
    PathTracker {
        parent: None,
        element: PathTrackerElement::Root,
    }
}

fn create_str_tracker<'i, 'j>(
    s: Cow<'i, str>,
    parent: &'j PathTracker<'i, 'j>,
) -> PathTracker<'i, 'j> {
    PathTracker {
        parent: Some(parent),
        element: PathTrackerElement::Key(s),
    }
}

const fn create_index_tracker<'i, 'j>(
    index: usize,
    parent: &'j PathTracker<'i, 'j>,
) -> PathTracker<'i, 'j> {
    PathTracker {
        parent: Some(parent),
        element: PathTrackerElement::Index(index),
    }
}

/* Enum for filter results */
#[derive(Debug)]
enum TermEvaluationResult<'i, 'j, S: SelectValue> {
    Integer(i64),
    Float(f64),
    Str(&'i str),
    String(String),
    Value(ValueRef<'j, S>),
    Bool(bool),
    Null,
    Invalid,
}

enum CmpResult {
    Ord(Ordering),
    NotComparable,
}

impl<'i, 'j, S: SelectValue> TermEvaluationResult<'i, 'j, S> {
    fn cmp(&self, s: &Self) -> CmpResult {
        match (self, s) {
            (TermEvaluationResult::Integer(n1), TermEvaluationResult::Integer(n2)) => {
                CmpResult::Ord(n1.cmp(n2))
            }
            (TermEvaluationResult::Float(_), TermEvaluationResult::Integer(n2)) => {
                self.cmp(&TermEvaluationResult::Float(*n2 as f64))
            }
            (TermEvaluationResult::Integer(n1), TermEvaluationResult::Float(_)) => {
                TermEvaluationResult::Float(*n1 as f64).cmp(s)
            }
            (TermEvaluationResult::Float(f1), TermEvaluationResult::Float(f2)) => {
                if *f1 > *f2 {
                    CmpResult::Ord(Ordering::Greater)
                } else if *f1 < *f2 {
                    CmpResult::Ord(Ordering::Less)
                } else {
                    CmpResult::Ord(Ordering::Equal)
                }
            }
            (TermEvaluationResult::Str(s1), TermEvaluationResult::Str(s2)) => {
                CmpResult::Ord(s1.cmp(s2))
            }
            (TermEvaluationResult::Str(s1), TermEvaluationResult::String(s2)) => {
                CmpResult::Ord((*s1).cmp(s2))
            }
            (TermEvaluationResult::String(s1), TermEvaluationResult::Str(s2)) => {
                CmpResult::Ord((s1[..]).cmp(s2))
            }
            (TermEvaluationResult::String(s1), TermEvaluationResult::String(s2)) => {
                CmpResult::Ord(s1.cmp(s2))
            }
            (TermEvaluationResult::Bool(b1), TermEvaluationResult::Bool(b2)) => {
                CmpResult::Ord(b1.cmp(b2))
            }
            (TermEvaluationResult::Null, TermEvaluationResult::Null) => {
                CmpResult::Ord(Ordering::Equal)
            }
            (TermEvaluationResult::Value(v), _) => match v.get_type() {
                SelectValueType::Long => v
                    .get_long()
                    .map(|n| TermEvaluationResult::Integer(n).cmp(s))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Double => v
                    .get_double()
                    .map(|f| TermEvaluationResult::Float(f).cmp(s))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::String => v
                    .as_str()
                    .map(|st| TermEvaluationResult::Str(st).cmp(s))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Bool => v
                    .get_bool()
                    .map(|b| TermEvaluationResult::Bool(b).cmp(s))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Null => TermEvaluationResult::Null.cmp(s),
                _ => CmpResult::NotComparable,
            },
            (_, TermEvaluationResult::Value(v)) => match v.get_type() {
                SelectValueType::Long => v
                    .get_long()
                    .map(|n| self.cmp(&TermEvaluationResult::Integer(n)))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Double => v
                    .get_double()
                    .map(|f| self.cmp(&TermEvaluationResult::Float(f)))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::String => v
                    .as_str()
                    .map(|st| self.cmp(&TermEvaluationResult::Str(st)))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Bool => v
                    .get_bool()
                    .map(|b| self.cmp(&TermEvaluationResult::Bool(b)))
                    .unwrap_or(CmpResult::NotComparable),
                SelectValueType::Null => self.cmp(&TermEvaluationResult::Null),
                _ => CmpResult::NotComparable,
            },
            (_, _) => CmpResult::NotComparable,
        }
    }
    fn gt(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_gt(),
            CmpResult::NotComparable => false,
        }
    }

    fn ge(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_ge(),
            CmpResult::NotComparable => false,
        }
    }

    fn lt(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_lt(),
            CmpResult::NotComparable => false,
        }
    }

    fn le(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_le(),
            CmpResult::NotComparable => false,
        }
    }

    fn eq(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => v1 == v2,
            (_, _) => match self.cmp(s) {
                CmpResult::Ord(o) => o.is_eq(),
                CmpResult::NotComparable => false,
            },
        }
    }

    fn ne(&self, s: &Self) -> bool {
        !self.eq(s)
    }

    fn re_is_match(regex: &str, s: &str) -> bool {
        Regex::new(regex).map_or_else(|_| false, |re| Regex::is_match(&re, s))
    }

    fn re_match(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v), TermEvaluationResult::Str(regex)) => {
                match v.get_type() {
                    SelectValueType::String => {
                        v.as_str().map_or(false, |s| Self::re_is_match(regex, s))
                    }
                    _ => false,
                }
            }
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => {
                match (v1.get_type(), v2.get_type()) {
                    (SelectValueType::String, SelectValueType::String) => v1
                        .as_str()
                        .zip(v2.as_str())
                        .map(|(s1, s2)| Self::re_is_match(s2, s1))
                        .unwrap_or(false),
                    (_, _) => false,
                }
            }
            (_, _) => false,
        }
    }

    fn re(&self, s: &Self) -> bool {
        self.re_match(s)
    }
}

/* This struct is used to calculate a json path on a json object.
 * The struct contains the query and the tracker generator that allows to create
 * path tracker to tracker paths that lead to different results. */
#[derive(Debug)]
pub struct PathCalculator<'i, UPTG: UserPathTrackerGenerator> {
    pub query: Option<&'i Query<'i>>,
    pub tracker_generator: Option<UPTG>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CalculationResult<'i, S: SelectValue, UPT: UserPathTracker> {
    pub res: ValueRef<'i, S>,
    pub path_tracker: Option<UPT>,
}

#[derive(Debug, PartialEq)]
struct PathCalculatorData<'i, S: SelectValue, UPT: UserPathTracker> {
    results: Vec<CalculationResult<'i, S, UPT>>,
    root: ValueRef<'i, S>,
}

// The following block of code is used to create a unified iterator for arrays and objects.
// This can be used in places where we need to iterate over both arrays and objects, create a path tracker from them.
enum Item<'a, S: SelectValue> {
    ArrayItem(usize, ValueRef<'a, S>),
    ObjectItem(Cow<'a, str>, ValueRef<'a, S>),
}

impl<'a, S: SelectValue> Item<'a, S> {
    fn value(&self) -> ValueRef<'a, S> {
        match self {
            Item::ArrayItem(_, v) => v.clone(),
            Item::ObjectItem(_, v) => v.clone(),
        }
    }

    fn create_tracker<'i>(&'a self, parent: &'i PathTracker<'a, 'i>) -> PathTracker<'a, 'i> {
        match self {
            Item::ArrayItem(index, _) => create_index_tracker(*index, parent),
            Item::ObjectItem(key, _) => create_str_tracker(key.clone(), parent),
        }
    }
}

enum UnifiedIter<'a, S: SelectValue> {
    Array(std::iter::Enumerate<Box<dyn Iterator<Item = ValueRef<'a, S>> + 'a>>),
    Object(Box<dyn Iterator<Item = (Cow<'a, str>, ValueRef<'a, S>)> + 'a>),
}

impl<'a, S: SelectValue> Iterator for UnifiedIter<'a, S> {
    type Item = Item<'a, S>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            UnifiedIter::Array(iter) => iter.next().map(|(i, v)| Item::ArrayItem(i, v)),
            UnifiedIter::Object(iter) => iter.next().map(|(k, v)| Item::ObjectItem(k, v)),
        }
    }
}

impl<'i, UPTG: UserPathTrackerGenerator> PathCalculator<'i, UPTG> {
    #[must_use]
    pub const fn create(query: &'i Query<'i>) -> PathCalculator<'i, UPTG> {
        PathCalculator {
            query: Some(query),
            tracker_generator: None,
        }
    }

    #[allow(dead_code)]
    pub const fn create_with_generator(
        query: &'i Query<'i>,
        tracker_generator: UPTG,
    ) -> PathCalculator<'i, UPTG> {
        PathCalculator {
            query: Some(query),
            tracker_generator: Some(tracker_generator),
        }
    }

    fn calc_full_scan<'j, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        match json.get_type() {
            SelectValueType::Object => {
                for (key, val) in value_ref_items!(json) {
                    let path_tracker = path_tracker.as_ref().map(|pt| create_str_tracker(key, pt));
                    self.calc_internal(pairs.clone(), val.clone(), path_tracker.clone(), calc_data);
                    self.calc_full_scan(pairs.clone(), val, path_tracker, calc_data);
                }
            }
            SelectValueType::Array => {
                for (i, v) in value_ref_values!(json).enumerate() {
                    let path_tracker = path_tracker.as_ref().map(|pt| create_index_tracker(i, pt));
                    self.calc_internal(pairs.clone(), v.clone(), path_tracker.clone(), calc_data);
                    self.calc_full_scan(pairs.clone(), v, path_tracker, calc_data);
                }
            }
            _ => {}
        }
    }

    fn calc_all<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        match json.get_type() {
            SelectValueType::Object => {
                for (key, val) in value_ref_items!(json) {
                    let new_tracker = path_tracker.as_ref().map(|pt| create_str_tracker(key, pt));
                    self.calc_internal(pairs.clone(), val, new_tracker, calc_data);
                }
            }
            SelectValueType::Array => {
                for (i, v) in value_ref_values!(json).enumerate() {
                    let new_tracker = path_tracker.as_ref().map(|pt| create_index_tracker(i, pt));
                    self.calc_internal(pairs.clone(), v, new_tracker, calc_data);
                }
            }
            _ => {}
        }
    }

    fn calc_literal<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        let key = curr.as_str();
        value_ref_get_key!(json, key).map(|val| {
            let new_tracker = path_tracker
                .as_ref()
                .map(|pt| create_str_tracker(Cow::Borrowed(key), pt));
            self.calc_internal(pairs, val, new_tracker, calc_data);
        });
    }

    fn calc_strings<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        for c in curr.into_inner() {
            let unescaped = unescape_string_value(c);
            value_ref_get_key!(json, &unescaped).map(|val| {
                let new_tracker = path_tracker
                    .as_ref()
                    .map(|pt| create_str_tracker(unescaped, pt));
                self.calc_internal(pairs.clone(), val, new_tracker, calc_data);
            });
        }
    }

    fn calc_abs_index(i: i64, n: usize) -> usize {
        if i >= 0 {
            (i as usize).min(n)
        } else {
            (i + n as i64).max(0) as usize
        }
    }

    /// Parse a string as i64, saturating to i64::MAX or i64::MIN on overflow
    /// instead of panicking. The PEG grammar (`number` rule) already guarantees
    /// the input is well-formed (optional '-' followed by ASCII digits), so
    /// overflow is the only reason parsing can fail.
    fn parse_index(s: &str) -> i64 {
        s.parse::<i64>().unwrap_or_else(|_| {
            if s.starts_with('-') {
                i64::MIN
            } else {
                i64::MAX
            }
        })
    }

    /// Parse a string as usize, saturating to usize::MAX on overflow. The PEG
    /// grammar (`pos_number` rule) guarantees only ASCII digits reach here, so
    /// overflow is the only reason parsing can fail.
    fn parse_step(s: &str) -> usize {
        s.parse::<usize>().unwrap_or(usize::MAX)
    }

    fn calc_indexes<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        if json.get_type() != SelectValueType::Array {
            return;
        }
        let Some(n) = json.len() else {
            return;
        };
        for c in curr.into_inner() {
            let i = Self::calc_abs_index(Self::parse_index(c.as_str()), n);
            value_ref_get_index!(json, i).map(|e| {
                let new_tracker = path_tracker.as_ref().map(|pt| create_index_tracker(i, pt));
                self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
            });
        }
    }

    fn calc_range<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        if json.get_type() != SelectValueType::Array {
            return;
        }
        let Some(n) = json.len() else {
            return;
        };
        let Some(range_spec) = curr.into_inner().next() else {
            trace!("calc_range: missing range specification");
            return;
        };
        let (start, end, step) = match range_spec.as_rule() {
            Rule::right_range => {
                let mut it = range_spec.into_inner();
                let start = 0;
                let Some(p) = it.next() else {
                    trace!("calc_range right_range: missing end index");
                    return;
                };
                let end = Self::calc_abs_index(Self::parse_index(p.as_str()), n);
                let step = it.next().map_or(1, |s| Self::parse_step(s.as_str()));
                (start, end, step)
            }
            Rule::all_range => {
                let mut it = range_spec.into_inner();
                let step = it.next().map_or(1, |s| Self::parse_step(s.as_str()));
                (0, n, step)
            }
            Rule::left_range => {
                let mut it = range_spec.into_inner();
                let Some(p) = it.next() else {
                    trace!("calc_range left_range: missing start index");
                    return;
                };
                let start = Self::calc_abs_index(Self::parse_index(p.as_str()), n);
                let end = n;
                let step = it.next().map_or(1, |s| Self::parse_step(s.as_str()));
                (start, end, step)
            }
            Rule::full_range => {
                let mut it = range_spec.into_inner();
                let Some(p1) = it.next() else {
                    trace!("calc_range full_range: missing start");
                    return;
                };
                let Some(p2) = it.next() else {
                    trace!("calc_range full_range: missing end");
                    return;
                };
                let start = Self::calc_abs_index(Self::parse_index(p1.as_str()), n);
                let end = Self::calc_abs_index(Self::parse_index(p2.as_str()), n);
                let step = it.next().map_or(1, |s| Self::parse_step(s.as_str()));
                (start, end, step)
            }
            other => {
                trace!("calc_range: unexpected inner rule {:?}", other);
                return;
            }
        };

        for i in (start..end).step_by(step) {
            value_ref_get_index!(json, i).map(|e| {
                let new_tracker = path_tracker.as_ref().map(|pt| create_index_tracker(i, pt));
                self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
            });
        }
    }

    fn evaluate_single_term<'j: 'i, S: SelectValue>(
        &self,
        term: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        match term.as_rule() {
            Rule::decimal => {
                if let Ok(i) = term.as_str().parse::<i64>() {
                    TermEvaluationResult::Integer(i)
                } else if let Ok(f) = term.as_str().parse::<f64>() {
                    TermEvaluationResult::Float(f)
                } else {
                    TermEvaluationResult::Invalid
                }
            }
            Rule::boolean_true => TermEvaluationResult::Bool(true),
            Rule::boolean_false => TermEvaluationResult::Bool(false),
            Rule::null => TermEvaluationResult::Null,
            Rule::string_value | Rule::string_value_escape_1 | Rule::string_value_escape_2 => {
                match unescape_string_value(term) {
                    Cow::Borrowed(s) => TermEvaluationResult::Str(s),
                    Cow::Owned(s) => TermEvaluationResult::String(s),
                }
            }
            Rule::from_current => match term.into_inner().next() {
                Some(term) => {
                    let mut calc_data = PathCalculatorData {
                        results: Vec::new(),
                        root: json.clone(),
                    };
                    self.calc_internal(term.into_inner(), json, None, &mut calc_data);
                    match calc_data.results.len() {
                        1 => calc_data
                            .results
                            .pop()
                            .map(|r| TermEvaluationResult::Value(r.res))
                            .unwrap_or(TermEvaluationResult::Invalid),
                        _ => TermEvaluationResult::Invalid,
                    }
                }
                None => TermEvaluationResult::Value(json),
            },
            Rule::from_root => match term.into_inner().next() {
                Some(term) => {
                    let mut new_calc_data = PathCalculatorData {
                        results: Vec::new(),
                        root: calc_data.root.clone(),
                    };
                    self.calc_internal(
                        term.into_inner(),
                        calc_data.root.clone(),
                        None,
                        &mut new_calc_data,
                    );
                    match new_calc_data.results.len() {
                        1 => new_calc_data
                            .results
                            .pop()
                            .map(|r| TermEvaluationResult::Value(r.res))
                            .unwrap_or(TermEvaluationResult::Invalid),
                        _ => TermEvaluationResult::Invalid,
                    }
                }
                None => TermEvaluationResult::Value(calc_data.root.clone()),
            },
            _ => {
                trace!("evaluate_single_term: unhandled rule {:?}", term.as_rule());
                TermEvaluationResult::Invalid
            }
        }
    }

    fn evaluate_single_filter<'j: 'i, S: SelectValue>(
        &self,
        curr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        let mut curr = curr.into_inner();
        let Some(term1) = curr.next() else {
            trace!("evaluate_single_filter: missing first term");
            return false;
        };
        trace!("evaluate_single_filter term1 {:?}", &term1);
        let term1_val = self.evaluate_single_term(term1, json.clone(), calc_data);
        trace!("evaluate_single_filter term1_val {:?}", &term1_val);
        if let Some(op) = curr.next() {
            trace!("evaluate_single_filter op {:?}", &op);
            let Some(term2) = curr.next() else {
                trace!("evaluate_single_filter: missing second term");
                return false;
            };
            trace!("evaluate_single_filter term2 {:?}", &term2);
            let term2_val = self.evaluate_single_term(term2, json, calc_data);
            trace!("evaluate_single_filter term2_val {:?}", &term2_val);
            match op.as_rule() {
                Rule::gt => term1_val.gt(&term2_val),
                Rule::ge => term1_val.ge(&term2_val),
                Rule::lt => term1_val.lt(&term2_val),
                Rule::le => term1_val.le(&term2_val),
                Rule::eq => term1_val.eq(&term2_val),
                Rule::ne => term1_val.ne(&term2_val),
                Rule::re => term1_val.re(&term2_val),
                _ => {
                    trace!(
                        "evaluate_single_filter: unknown comparison op {:?}",
                        op.as_rule()
                    );
                    false
                }
            }
        } else {
            !matches!(term1_val, TermEvaluationResult::Invalid)
        }
    }

    fn evaluate_filter<'j: 'i, S: SelectValue>(
        &self,
        mut curr: Pairs<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        let Some(first_filter) = curr.next() else {
            trace!("evaluate_filter: missing first operand");
            return false;
        };
        trace!("evaluate_filter first_filter {:?}", &first_filter);
        let mut first_result = match first_filter.as_rule() {
            Rule::single_filter => {
                self.evaluate_single_filter(first_filter, json.clone(), calc_data)
            }
            Rule::filter => {
                self.evaluate_filter(first_filter.into_inner(), json.clone(), calc_data)
            }
            _ => {
                trace!(
                    "evaluate_filter: unexpected first rule {:?}",
                    first_filter.as_rule()
                );
                false
            }
        };
        trace!("evaluate_filter first_result {:?}", &first_result);

        // Evaluate filter operands with operator (relation) precedence of AND before OR, e.g.,
        //  A && B && C || D || E && F ===> (A && B && C) || D || (E && F)
        //  A || B && C ===> A || (B && C)
        // When encountering AND operator, if previous value is false then skip evaluating the rest until an OR operand is encountered or no more operands.
        // When encountering OR operator, if previous value is true then break, if previous value is false then tail-recurse to continue evaluating the rest.
        //
        // When a parenthesized filter is encountered (Rule::filter), e.g., ... || ( A || B ) && C,
        //  recurse on it and use the result as the operand.

        while let Some(relation) = curr.next() {
            match relation.as_rule() {
                Rule::and => {
                    // Consume the operand even if not needed for evaluation
                    let Some(second_filter) = curr.next() else {
                        trace!("evaluate_filter &&: missing operand");
                        return false;
                    };
                    trace!("evaluate_filter && second_filter {:?}", &second_filter);
                    if !first_result {
                        continue; // Skip eval till next OR
                    }
                    first_result = match second_filter.as_rule() {
                        Rule::single_filter => {
                            self.evaluate_single_filter(second_filter, json.clone(), calc_data)
                        }
                        Rule::filter => self.evaluate_filter(
                            second_filter.into_inner(),
                            json.clone(),
                            calc_data,
                        ),
                        _ => {
                            trace!(
                                "evaluate_filter &&: unexpected rule {:?}",
                                second_filter.as_rule()
                            );
                            false
                        }
                    };
                }
                Rule::or => {
                    trace!("evaluate_filter ||");
                    if first_result {
                        break; // can return True
                    }
                    // Tail recursion with the rest of the expression to give precedence to AND
                    return self.evaluate_filter(curr, json, calc_data);
                }
                _ => {
                    trace!(
                        "evaluate_filter: unexpected relation {:?}",
                        relation.as_rule()
                    );
                    return false;
                }
            }
        }
        first_result
    }

    fn populate_path_tracker(pt: &PathTracker<'_, '_>, upt: &mut UPTG::PT) {
        if let Some(parent) = pt.parent {
            Self::populate_path_tracker(parent, upt)
        }
        match pt.element {
            PathTrackerElement::Index(i) => upt.add_index(i),
            PathTrackerElement::Key(ref k) => upt.add_str(k),
            PathTrackerElement::Root => {}
        }
    }

    fn generate_path(&self, pt: PathTracker) -> UPTG::PT {
        // Invariant: `generate_path` is only used when building tracked results; the calculator
        // must have been created with a real `tracker_generator` (not the `calc_once` dummy config).
        let mut upt = self
            .tracker_generator
            .as_ref()
            .expect("internal: generate_path requires tracker_generator")
            .generate();
        Self::populate_path_tracker(&pt, &mut upt);
        upt
    }

    fn calc_internal<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        mut pairs: Pairs<'i, Rule>,
        json: ValueRef<'j, S>,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        let curr = pairs.next();
        match curr {
            Some(curr) => {
                trace!("calc_internal curr {:?}", &curr.as_rule());
                match curr.as_rule() {
                    Rule::full_scan => {
                        self.calc_internal(
                            pairs.clone(),
                            json.clone(),
                            path_tracker.clone(),
                            calc_data,
                        );
                        self.calc_full_scan(pairs, json, path_tracker, calc_data);
                    }
                    Rule::all => self.calc_all(pairs, json, path_tracker, calc_data),
                    Rule::literal => self.calc_literal(pairs, curr, json, path_tracker, calc_data),
                    Rule::string_list => {
                        self.calc_strings(pairs, curr, json, path_tracker, calc_data);
                    }
                    Rule::numbers_list => {
                        self.calc_indexes(pairs, curr, json, path_tracker, calc_data);
                    }
                    Rule::numbers_range => {
                        self.calc_range(pairs, curr, json, path_tracker, calc_data);
                    }
                    Rule::filter => {
                        let json_type = json.get_type();
                        if json_type == SelectValueType::Array
                            || json_type == SelectValueType::Object
                        {
                            /* lets expend the array, this is how most json path engines work.
                             * Personally, I think this if should not exists. */
                            let unified_iter = if json_type == SelectValueType::Object {
                                UnifiedIter::Object(value_ref_items!(json))
                            } else {
                                UnifiedIter::Array(value_ref_values!(json).enumerate())
                            };

                            if let Some(pt) = path_tracker {
                                trace!("calc_internal type {:?} path_tracker {:?}", json_type, &pt);
                                for item in unified_iter {
                                    let v = item.value();
                                    trace!("calc_internal v {:?}", &v);
                                    if self.evaluate_filter(
                                        curr.clone().into_inner(),
                                        v.clone(),
                                        calc_data,
                                    ) {
                                        let new_tracker = Some(item.create_tracker(&pt));
                                        self.calc_internal(
                                            pairs.clone(),
                                            v,
                                            new_tracker,
                                            calc_data,
                                        );
                                    }
                                }
                            } else {
                                trace!("calc_internal type {:?} path_tracker None", json_type);
                                for item in unified_iter {
                                    let v = item.value();
                                    trace!("calc_internal v {:?}", &v);
                                    if self.evaluate_filter(
                                        curr.clone().into_inner(),
                                        v.clone(),
                                        calc_data,
                                    ) {
                                        self.calc_internal(pairs.clone(), v, None, calc_data);
                                    }
                                }
                            }
                        } else if self.evaluate_filter(curr.into_inner(), json.clone(), calc_data) {
                            trace!(
                                "calc_internal type {:?} path_tracker {:?}",
                                json_type,
                                &path_tracker
                            );
                            self.calc_internal(pairs, json, path_tracker, calc_data);
                        }
                    }
                    Rule::EOI => {
                        calc_data.results.push(CalculationResult {
                            res: json,
                            path_tracker: path_tracker.map(|pt| self.generate_path(pt)),
                        });
                    }
                    _ => {
                        trace!("calc_internal: unhandled rule {:?}", curr.as_rule());
                    }
                }
            }
            None => {
                calc_data.results.push(CalculationResult {
                    res: json,
                    path_tracker: path_tracker.map(|pt| self.generate_path(pt)),
                });
            }
        }
    }

    pub fn calc_with_paths_on_root<'j: 'i, S: SelectValue>(
        &self,
        json: ValueRef<'j, S>,
        root: Pairs<'i, Rule>,
    ) -> Vec<CalculationResult<'j, S, UPTG::PT>> {
        let mut calc_data = PathCalculatorData {
            results: Vec::new(),
            root: json.clone(),
        };
        if self.tracker_generator.is_some() {
            self.calc_internal(root, json, Some(create_empty_tracker()), &mut calc_data);
        } else {
            self.calc_internal(root, json, None, &mut calc_data);
        }
        calc_data.results.drain(..).collect()
    }

    pub fn calc_with_paths<'j: 'i, S: SelectValue>(
        &self,
        json: ValueRef<'j, S>,
    ) -> Vec<CalculationResult<'j, S, UPTG::PT>> {
        // Invariant: only valid on calculators from `create` / `create_with_generator` (hold `query`).
        // Not for the internal `calc_once` configuration with `query: None`.
        let root = self
            .query
            .as_ref()
            .expect("internal: calc_with_paths requires compiled query")
            .root
            .clone();
        self.calc_with_paths_on_root(json, root)
    }

    pub fn calc<'j: 'i, S: SelectValue>(&self, json: &'j S) -> Vec<ValueRef<'j, S>> {
        self.calc_with_paths(ValueRef::Borrowed(json))
            .into_iter()
            .map(|e| e.res)
            .collect()
    }

    #[allow(dead_code)]
    pub fn calc_paths<'j: 'i, S: SelectValue>(&self, json: &'j S) -> Vec<Vec<String>> {
        self.calc_with_paths(ValueRef::Borrowed(json))
            .into_iter()
            .filter_map(|e| e.path_tracker.map(|pt| pt.to_string_path()))
            .collect()
    }
}

#[cfg(test)]
mod json_path_compiler_tests {
    use crate::json_path::compile;
    use crate::json_path::JsonPathToken;

    #[test]
    fn test_compiler_pop_last() {
        let query = compile("$.foo");
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("foo".to_string(), JsonPathToken::String)
        );
    }

    #[test]
    fn test_compiler_pop_last_number() {
        let query = compile("$.[1]");
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("1".to_string(), JsonPathToken::Number)
        );
    }

    #[test]
    fn test_compiler_pop_last_string_bracket_notation() {
        let query = compile("$.[\"foo\"]");
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("foo".to_string(), JsonPathToken::String)
        );
    }

    #[test]
    fn test_compiler_pop_last_escaped_backslash() {
        let query = compile(r#"$["\\"]"#);
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("\\".to_string(), JsonPathToken::String)
        );
    }

    #[test]
    fn test_compiler_pop_last_escaped_double_backslash() {
        let query = compile(r#"$["\\\\"]"#);
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("\\\\".to_string(), JsonPathToken::String)
        );
    }

    #[test]
    fn test_compiler_pop_last_escaped_quote() {
        let query = compile(r#"$["\""]"#);
        assert_eq!(
            query.unwrap().pop_last().unwrap(),
            ("\"".to_string(), JsonPathToken::String)
        );
    }

    #[test]
    fn test_compiler_is_static() {
        let query = compile("$.[\"foo\"]");
        assert!(query.unwrap().is_static());

        let query = compile("$.[\"foo\", \"bar\"]");
        assert!(!query.unwrap().is_static());
    }

    #[test]
    fn test_compiler_size() {
        let query = compile("$.[\"foo\"]");
        assert_eq!(query.unwrap().size(), 1);

        let query = compile("$.[\"foo\"].bar");
        assert_eq!(query.unwrap().size(), 2);

        let query = compile("$.[\"foo\"].bar[1]");
        assert_eq!(query.unwrap().size(), 3);
    }
}
