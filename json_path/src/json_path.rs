/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use itertools::Itertools;
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;
use redis_module::RedisError;
use std::cmp::Ordering;

use crate::select_value::{SelectValue, SelectValueType};
use log::trace;
use regex::Regex;
use std::fmt::Debug;

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

impl<'i> Query<'i> {
    /// Pop the last element from the compiled json path.
    /// For example, if the json path is $.foo.bar then `pop_last`
    /// will return bar and leave the json path query with foo only
    /// ($.foo)
    #[allow(dead_code)]
    pub fn pop_last(&mut self) -> Option<(String, JsonPathToken)> {
        self.root.next_back().and_then(|last| match last.as_rule() {
            Rule::literal => Some((last.as_str().to_string(), JsonPathToken::String)),
            Rule::number => Some((last.as_str().to_string(), JsonPathToken::Number)),
            Rule::numbers_list => {
                let first_on_list = last.into_inner().next();
                first_on_list.map(|first| (first.as_str().to_string(), JsonPathToken::Number))
            }
            Rule::string_list => {
                let first_on_list = last.into_inner().next();
                first_on_list.map(|first| (first.as_str().to_string(), JsonPathToken::String))
            }
            _ => panic!("pop last was used in a none static path"),
        })
    }

    /// Returns the amount of elements in the json path
    /// Example: $.foo.bar has 2 elements
    #[allow(dead_code)]
    pub fn size(&mut self) -> usize {
        if self.size.is_some() {
            return self.size.unwrap();
        }
        self.is_static();
        self.size()
    }

    /// Results whether or not the compiled json path is static
    /// Static path is a path that is promised to have at most a single result.
    /// Example:
    ///     static path: $.foo.bar
    ///     none static path: $.*.bar
    #[allow(dead_code)]
    pub fn is_static(&mut self) -> bool {
        if self.is_static.is_some() {
            return self.is_static.unwrap();
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
        self.is_static()
    }
}

impl std::fmt::Display for QueryCompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Error occurred on position {}, {}",
            self.location, self.message
        )
    }
}

impl From<QueryCompilationError> for RedisError {
    fn from(e: QueryCompilationError) -> Self {
        Self::String(e.to_string())
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

/// Compile the given string query into a query object.
/// Returns error on compilation error.
pub(crate) fn compile(path: &str) -> Result<Query, QueryCompilationError> {
    JsonPathParser::parse(Rule::query, path)
        .map(|mut q| {
            let root = q.next().unwrap();
            Query {
                root: root.into_inner(),
                is_static: None,
                size: None,
            }
        })
        .map_err(|e| {
            let pos = match e.location {
                pest::error::InputLocation::Pos(pos) => pos,
                pest::error::InputLocation::Span((pos, _end)) => pos,
            };
            let msg = match e.variant {
                pest::error::ErrorVariant::ParsingError {
                    positives,
                    negatives,
                } => {
                    let positives = (!positives.is_empty()).then(|| {
                        positives
                            .into_iter()
                            .map(|v| format!("{v}"))
                            .collect_vec()
                            .join(", ")
                    });
                    let negatives = (!negatives.is_empty()).then(|| {
                        negatives
                            .into_iter()
                            .map(|v| format!("{v}"))
                            .collect_vec()
                            .join(", ")
                    });

                    match (positives, negatives) {
                        (None, None) => "parsing error".to_string(),
                        (Some(p), None) => format!("expected one of the following: {p}"),
                        (None, Some(n)) => format!("unexpected tokens found: {n}"),
                        (Some(p), Some(n)) => format!(
                            "expected one of the following: {p}, unexpected tokens found: {n}"
                        ),
                    }
                }
                pest::error::ErrorVariant::CustomError { message } => message,
            };

            let final_msg = if pos == path.len() {
                format!("\"{path} <<<<----\", {msg}.")
            } else {
                format!("\"{} ---->>>> {}\", {}.", &path[..pos], &path[pos..], msg)
            };
            QueryCompilationError {
                location: pos,
                message: final_msg,
            }
        })
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

#[derive(Debug, PartialEq, Eq)]
pub enum PTrackerElement {
    Key(String),
    Index(usize),
}

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
    Key(&'i str),
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

impl<'i, 'j> Default for PathTracker<'i, 'j> {
    fn default() -> Self {
        PathTracker {
            parent: None,
            element: PathTrackerElement::Root,
        }
    }
}

impl<'i, 'j> PathTracker<'i, 'j> {
    const fn str_tracker(s: &'i str, parent: &'j Self) -> Self {
        Self {
            parent: Some(parent),
            element: PathTrackerElement::Key(s),
        }
    }

    const fn index_tracker(i: usize, parent: &'j Self) -> Self {
        Self {
            parent: Some(parent),
            element: PathTrackerElement::Index(i),
        }
    }
}

/* Enum for filter results */
#[derive(Debug)]
enum TermEvaluationResult<'i, 'j, S: SelectValue> {
    Integer(i64),
    Float(f64),
    Str(&'i str),
    String(String),
    Value(&'j S),
    Bool(bool),
    Null,
    Invalid,
}

impl<'i, 'j, S: SelectValue> PartialEq for TermEvaluationResult<'i, 'j, S> {
    fn eq(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => v1 == v2,
            _ => self.partial_cmp(s).map_or(false, Ordering::is_eq),
        }
    }
}

impl<'i, 'j, S: SelectValue> PartialOrd for TermEvaluationResult<'i, 'j, S> {
    fn partial_cmp(&self, s: &Self) -> Option<Ordering> {
        match (self, s) {
            (TermEvaluationResult::Integer(n1), TermEvaluationResult::Integer(n2)) => {
                Some(n1.cmp(n2))
            }
            (TermEvaluationResult::Float(_), TermEvaluationResult::Integer(n2)) => {
                self.partial_cmp(&TermEvaluationResult::Float(*n2 as f64))
            }
            (TermEvaluationResult::Integer(n1), TermEvaluationResult::Float(_)) => {
                TermEvaluationResult::Float(*n1 as f64).partial_cmp(s)
            }
            (TermEvaluationResult::Float(f1), TermEvaluationResult::Float(f2)) => {
                f1.partial_cmp(f2)
            }
            (TermEvaluationResult::Str(s1), TermEvaluationResult::Str(s2)) => Some(s1.cmp(s2)),
            (TermEvaluationResult::Str(s1), TermEvaluationResult::String(s2)) => {
                Some((*s1).cmp(s2))
            }
            (TermEvaluationResult::String(s1), TermEvaluationResult::Str(s2)) => {
                Some((s1[..]).cmp(s2))
            }
            (TermEvaluationResult::String(s1), TermEvaluationResult::String(s2)) => {
                Some(s1.cmp(s2))
            }
            (TermEvaluationResult::Bool(b1), TermEvaluationResult::Bool(b2)) => Some(b1.cmp(b2)),
            (TermEvaluationResult::Null, TermEvaluationResult::Null) => Some(Ordering::Equal),
            (TermEvaluationResult::Value(v), _) => match v.get_type() {
                SelectValueType::Long => TermEvaluationResult::Integer(v.get_long()).partial_cmp(s),
                SelectValueType::Double => {
                    TermEvaluationResult::Float(v.get_double()).partial_cmp(s)
                }
                SelectValueType::String => TermEvaluationResult::Str(v.as_str()).partial_cmp(s),
                SelectValueType::Bool => TermEvaluationResult::Bool(v.get_bool()).partial_cmp(s),
                SelectValueType::Null => TermEvaluationResult::Null.partial_cmp(s),
                _ => None,
            },
            (_, TermEvaluationResult::Value(v)) => match v.get_type() {
                SelectValueType::Long => {
                    self.partial_cmp(&TermEvaluationResult::Integer(v.get_long()))
                }
                SelectValueType::Double => {
                    self.partial_cmp(&TermEvaluationResult::Float(v.get_double()))
                }
                SelectValueType::String => self.partial_cmp(&TermEvaluationResult::Str(v.as_str())),
                SelectValueType::Bool => {
                    self.partial_cmp(&TermEvaluationResult::Bool(v.get_bool()))
                }
                SelectValueType::Null => self.partial_cmp(&TermEvaluationResult::Null),
                _ => None,
            },
            _ => None,
        }
    }
}

impl<'i, 'j, S: SelectValue> TermEvaluationResult<'i, 'j, S> {
    fn re_is_match(regex: &str, s: &str) -> bool {
        Regex::new(regex).map_or(false, |re| Regex::is_match(&re, s))
    }

    fn re(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v), TermEvaluationResult::Str(regex)) => {
                match v.get_type() {
                    SelectValueType::String => Self::re_is_match(regex, v.as_str()),
                    _ => false,
                }
            }
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => {
                match (v1.get_type(), v2.get_type()) {
                    (SelectValueType::String, SelectValueType::String) => {
                        Self::re_is_match(v2.as_str(), v1.as_str())
                    }
                    _ => false,
                }
            }
            _ => false,
        }
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
    pub res: &'i S,
    pub path_tracker: Option<UPT>,
}

#[derive(Debug, PartialEq)]
struct PathCalculatorData<'i, S: SelectValue, UPT: UserPathTracker> {
    results: Vec<CalculationResult<'i, S, UPT>>,
    root: &'i S,
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

    fn calc_full_scan<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        match json.get_type() {
            SelectValueType::Object => {
                if let Some(pt) = path_tracker {
                    json.items().unwrap().for_each(|(key, val)| {
                        self.calc_internal(
                            pairs.clone(),
                            val,
                            Some(PathTracker::str_tracker(key, &pt)),
                            calc_data,
                        );
                        self.calc_full_scan(
                            pairs.clone(),
                            val,
                            Some(PathTracker::str_tracker(key, &pt)),
                            calc_data,
                        );
                    })
                } else {
                    json.values().unwrap().for_each(|v| {
                        self.calc_internal(pairs.clone(), v, None, calc_data);
                        self.calc_full_scan(pairs.clone(), v, None, calc_data);
                    })
                }
            }
            SelectValueType::Array => {
                let values = json.values().unwrap();
                if let Some(pt) = path_tracker {
                    for (i, v) in values.enumerate() {
                        self.calc_internal(
                            pairs.clone(),
                            v,
                            Some(PathTracker::index_tracker(i, &pt)),
                            calc_data,
                        );
                        self.calc_full_scan(
                            pairs.clone(),
                            v,
                            Some(PathTracker::index_tracker(i, &pt)),
                            calc_data,
                        );
                    }
                } else {
                    for v in values {
                        self.calc_internal(pairs.clone(), v, None, calc_data);
                        self.calc_full_scan(pairs.clone(), v, None, calc_data);
                    }
                }
            }
            _ => {}
        }
    }

    fn calc_all<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        match json.get_type() {
            SelectValueType::Object => {
                if let Some(pt) = path_tracker {
                    let items = json.items().unwrap();
                    for (key, val) in items {
                        let new_tracker = Some(PathTracker::str_tracker(key, &pt));
                        self.calc_internal(pairs.clone(), val, new_tracker, calc_data);
                    }
                } else {
                    let values = json.values().unwrap();
                    for v in values {
                        self.calc_internal(pairs.clone(), v, None, calc_data);
                    }
                }
            }
            SelectValueType::Array => {
                let values = json.values().unwrap();
                if let Some(pt) = path_tracker {
                    for (i, v) in values.enumerate() {
                        let new_tracker = Some(PathTracker::index_tracker(i, &pt));
                        self.calc_internal(pairs.clone(), v, new_tracker, calc_data);
                    }
                } else {
                    for v in values {
                        self.calc_internal(pairs.clone(), v, None, calc_data);
                    }
                }
            }
            _ => {}
        }
    }

    fn calc_literal<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        let curr_val = json.get_key(curr.as_str());
        if let Some(e) = curr_val {
            if let Some(pt) = path_tracker {
                let new_tracker = Some(PathTracker::str_tracker(curr.as_str(), &pt));
                self.calc_internal(pairs, e, new_tracker, calc_data);
            } else {
                self.calc_internal(pairs, e, None, calc_data);
            }
        }
    }

    fn calc_strings<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        if let Some(pt) = path_tracker {
            for c in curr.into_inner() {
                let s = c.as_str();
                let curr_val = match c.as_rule() {
                    Rule::string_value => json.get_key(s),
                    Rule::string_value_escape_1 => {
                        json.get_key(&(s.replace("\\\\", "\\").replace("\\'", "'")))
                    }
                    Rule::string_value_escape_2 => {
                        json.get_key(&(s.replace("\\\\", "\\").replace("\\\"", "\"")))
                    }
                    _ => panic!("{c:?}"),
                };
                if let Some(e) = curr_val {
                    let new_tracker = Some(PathTracker::str_tracker(s, &pt));
                    self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
                }
            }
        } else {
            for c in curr.into_inner() {
                let s = c.as_str();
                let curr_val = match c.as_rule() {
                    Rule::string_value => json.get_key(s),
                    Rule::string_value_escape_1 => {
                        json.get_key(&(s.replace("\\\\", "\\").replace("\\\"", "\"")))
                    }
                    Rule::string_value_escape_2 => {
                        json.get_key(&(s.replace("\\\\", "\\").replace("\\'", "'")))
                    }
                    _ => panic!("{c:?}"),
                };
                if let Some(e) = curr_val {
                    self.calc_internal(pairs.clone(), e, None, calc_data);
                }
            }
        }
    }

    fn calc_abs_index(i: i64, n: usize) -> usize {
        if i >= 0 {
            (i as usize).min(n)
        } else {
            (i + n as i64).max(0) as usize
        }
    }

    fn calc_indexes<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        if json.get_type() != SelectValueType::Array {
            return;
        }
        let n = json.len().unwrap();
        if let Some(pt) = path_tracker {
            for c in curr.into_inner() {
                let i = Self::calc_abs_index(c.as_str().parse::<i64>().unwrap(), n);
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    let new_tracker = Some(PathTracker::index_tracker(i, &pt));
                    self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
                }
            }
        } else {
            for c in curr.into_inner() {
                let i = Self::calc_abs_index(c.as_str().parse::<i64>().unwrap(), n);
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    self.calc_internal(pairs.clone(), e, None, calc_data);
                }
            }
        }
    }

    fn calc_range<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        pairs: Pairs<'i, Rule>,
        curr: Pair<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        if json.get_type() != SelectValueType::Array {
            return;
        }
        let n = json.len().unwrap();
        let curr = curr.into_inner().next().unwrap();
        let (start, end, step) = match curr.as_rule() {
            Rule::right_range => {
                let mut curr = curr.into_inner();
                let start = 0;
                let end =
                    Self::calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let step = curr
                    .next()
                    .map_or(1, |s| s.as_str().parse::<usize>().unwrap());
                (start, end, step)
            }
            Rule::all_range => {
                let mut curr = curr.into_inner();
                let step = curr
                    .next()
                    .map_or(1, |s| s.as_str().parse::<usize>().unwrap());
                (0, n, step)
            }
            Rule::left_range => {
                let mut curr = curr.into_inner();
                let start =
                    Self::calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let end = n;
                let step = curr
                    .next()
                    .map_or(1, |s| s.as_str().parse::<usize>().unwrap());
                (start, end, step)
            }
            Rule::full_range => {
                let mut curr = curr.into_inner();
                let start =
                    Self::calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let end =
                    Self::calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let step = curr
                    .next()
                    .map_or(1, |s| s.as_str().parse::<usize>().unwrap());
                (start, end, step)
            }
            _ => panic!("{curr:?}"),
        };

        if let Some(pt) = path_tracker {
            for i in (start..end).step_by(step) {
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    let new_tracker = Some(PathTracker::index_tracker(i, &pt));
                    self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
                }
            }
        } else {
            for i in (start..end).step_by(step) {
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    self.calc_internal(pairs.clone(), e, None, calc_data);
                }
            }
        }
    }

    fn evaluate_single_term<'j: 'i, S: SelectValue>(
        &self,
        term: Pair<'i, Rule>,
        json: &'j S,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        match term.as_rule() {
            Rule::decimal => {
                if let Ok(i) = term.as_str().parse::<i64>() {
                    TermEvaluationResult::Integer(i)
                } else {
                    TermEvaluationResult::Float(term.as_str().parse::<f64>().unwrap())
                }
            }
            Rule::boolean_true => TermEvaluationResult::Bool(true),
            Rule::boolean_false => TermEvaluationResult::Bool(false),
            Rule::null => TermEvaluationResult::Null,
            Rule::string_value => TermEvaluationResult::Str(term.as_str()),
            Rule::string_value_escape_1 => TermEvaluationResult::String(
                term.as_str().replace("\\\\", "\\").replace("\\'", "'"),
            ),
            Rule::string_value_escape_2 => TermEvaluationResult::String(
                term.as_str().replace("\\\\", "\\").replace("\\\"", "\""),
            ),
            Rule::from_current => match term.into_inner().next() {
                Some(term) => {
                    let mut calc_data = PathCalculatorData {
                        results: Vec::new(),
                        root: json,
                    };
                    self.calc_internal(term.into_inner(), json, None, &mut calc_data);
                    if calc_data.results.len() == 1 {
                        TermEvaluationResult::Value(calc_data.results.pop().unwrap().res)
                    } else {
                        TermEvaluationResult::Invalid
                    }
                }
                None => TermEvaluationResult::Value(json),
            },
            Rule::from_root => match term.into_inner().next() {
                Some(term) => {
                    let mut new_calc_data = PathCalculatorData {
                        results: Vec::new(),
                        root: calc_data.root,
                    };
                    self.calc_internal(term.into_inner(), calc_data.root, None, &mut new_calc_data);
                    if new_calc_data.results.len() == 1 {
                        TermEvaluationResult::Value(new_calc_data.results.pop().unwrap().res)
                    } else {
                        TermEvaluationResult::Invalid
                    }
                }
                None => TermEvaluationResult::Value(calc_data.root),
            },
            _ => {
                panic!("{term:?}")
            }
        }
    }

    fn evaluate_single_filter<'j: 'i, S: SelectValue>(
        &self,
        curr: Pair<'i, Rule>,
        json: &'j S,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        let mut curr = curr.into_inner();
        let term1 = curr.next().unwrap();
        trace!("evaluate_single_filter term1 {:?}", &term1);
        let term1_val = self.evaluate_single_term(term1, json, calc_data);
        trace!("evaluate_single_filter term1_val {:?}", &term1_val);
        if let Some(op) = curr.next() {
            trace!("evaluate_single_filter op {:?}", &op);
            let term2 = curr.next().unwrap();
            trace!("evaluate_single_filter term2 {:?}", &term2);
            let term2_val = self.evaluate_single_term(term2, json, calc_data);
            trace!("evaluate_single_filter term2_val {:?}", &term2_val);
            match op.as_rule() {
                Rule::gt => term1_val > term2_val,
                Rule::ge => term1_val >= term2_val,
                Rule::lt => term1_val < term2_val,
                Rule::le => term1_val <= term2_val,
                Rule::eq => term1_val == term2_val,
                Rule::ne => term1_val != term2_val,
                Rule::re => term1_val.re(&term2_val),
                _ => panic!("{op:?}"),
            }
        } else {
            !matches!(term1_val, TermEvaluationResult::Invalid)
        }
    }

    fn evaluate_filter<'j: 'i, S: SelectValue>(
        &self,
        mut curr: Pairs<'i, Rule>,
        json: &'j S,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        let first_filter = curr.next().unwrap();
        trace!("evaluate_filter first_filter {:?}", &first_filter);
        let mut first_result = match first_filter.as_rule() {
            Rule::single_filter => self.evaluate_single_filter(first_filter, json, calc_data),
            Rule::filter => self.evaluate_filter(first_filter.into_inner(), json, calc_data),
            _ => panic!("{first_filter:?}"),
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
                    let second_filter = curr.next().unwrap();
                    trace!("evaluate_filter && second_filter {:?}", &second_filter);
                    if !first_result {
                        continue; // Skip eval till next OR
                    }
                    first_result = match second_filter.as_rule() {
                        Rule::single_filter => {
                            self.evaluate_single_filter(second_filter, json, calc_data)
                        }
                        Rule::filter => {
                            self.evaluate_filter(second_filter.into_inner(), json, calc_data)
                        }
                        _ => panic!("{second_filter:?}"),
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
                _ => panic!("{relation:?}"),
            }
        }
        first_result
    }

    fn populate_path_tracker(pt: &PathTracker<'_, '_>, upt: &mut UPTG::PT) {
        if let Some(f) = pt.parent {
            Self::populate_path_tracker(f, upt);
        }
        match pt.element {
            PathTrackerElement::Index(i) => upt.add_index(i),
            PathTrackerElement::Key(k) => upt.add_str(k),
            PathTrackerElement::Root => {}
        }
    }

    fn generate_path(&self, pt: PathTracker) -> UPTG::PT {
        let mut upt = self.tracker_generator.as_ref().unwrap().generate();
        Self::populate_path_tracker(&pt, &mut upt);
        upt
    }

    fn calc_internal<'j: 'i, 'k, 'l, S: SelectValue>(
        &self,
        mut pairs: Pairs<'i, Rule>,
        json: &'j S,
        path_tracker: Option<PathTracker<'l, 'k>>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) {
        let curr = pairs.next();
        match curr {
            Some(curr) => {
                trace!("calc_internal curr {:?}", &curr.as_rule());
                match curr.as_rule() {
                    Rule::full_scan => {
                        self.calc_internal(pairs.clone(), json, path_tracker.clone(), calc_data);
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
                        if matches!(
                            json.get_type(),
                            SelectValueType::Array | SelectValueType::Object
                        ) {
                            /* lets expend the array, this is how most json path engines work.
                             * Personally, I think this if should not exists. */
                            let values = json.values().unwrap();
                            if let Some(pt) = path_tracker {
                                trace!(
                                    "calc_internal type {:?} path_tracker {:?}",
                                    json.get_type(),
                                    &pt
                                );
                                for (i, v) in values.enumerate() {
                                    trace!("calc_internal v {:?}", &v);
                                    if self.evaluate_filter(curr.clone().into_inner(), v, calc_data)
                                    {
                                        self.calc_internal(
                                            pairs.clone(),
                                            v,
                                            Some(PathTracker::index_tracker(i, &pt)),
                                            calc_data,
                                        );
                                    }
                                }
                            } else {
                                trace!(
                                    "calc_internal type {:?} path_tracker None",
                                    json.get_type()
                                );
                                for v in values {
                                    trace!("calc_internal v {:?}", &v);
                                    if self.evaluate_filter(curr.clone().into_inner(), v, calc_data)
                                    {
                                        self.calc_internal(pairs.clone(), v, None, calc_data);
                                    }
                                }
                            }
                        } else if self.evaluate_filter(curr.into_inner(), json, calc_data) {
                            trace!(
                                "calc_internal type {:?} path_tracker {:?}",
                                json.get_type(),
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
                    _ => panic!("{curr:?}"),
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
        json: &'j S,
        root: Pairs<'i, Rule>,
    ) -> Vec<CalculationResult<'j, S, UPTG::PT>> {
        let mut calc_data = PathCalculatorData {
            results: Vec::new(),
            root: json,
        };
        if self.tracker_generator.is_some() {
            self.calc_internal(root, json, Some(PathTracker::default()), &mut calc_data);
        } else {
            self.calc_internal(root, json, None, &mut calc_data);
        }
        calc_data.results
    }

    pub fn calc_with_paths<'j: 'i, S: SelectValue>(
        &self,
        json: &'j S,
    ) -> Vec<CalculationResult<'j, S, UPTG::PT>> {
        self.calc_with_paths_on_root(json, self.query.unwrap().root.clone())
    }

    pub fn calc<'j: 'i, S: SelectValue>(&self, json: &'j S) -> Vec<&'j S> {
        self.calc_with_paths(json)
            .into_iter()
            .map(|e| e.res)
            .collect()
    }

    #[allow(dead_code)]
    pub fn calc_paths<'j: 'i, S: SelectValue>(&self, json: &'j S) -> Vec<Vec<String>> {
        self.calc_with_paths(json)
            .into_iter()
            .map(|e| e.path_tracker.unwrap().to_string_path())
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
