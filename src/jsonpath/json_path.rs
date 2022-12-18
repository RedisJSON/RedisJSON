/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use pest::iterators::{Pair, Pairs};
use pest::Parser;
use std::cmp::Ordering;

use crate::jsonpath::select_value::{SelectValue, SelectValueType};
use log::trace;
use regex::Regex;
use std::fmt::Debug;

#[derive(Parser)]
#[grammar = "jsonpath/grammer.pest"]
pub struct JsonPathParser;

#[derive(Debug, PartialEq)]
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
    /// For example, if the json path is $.foo.bar then pop_last
    /// will return bar and leave the json path query with foo only
    /// ($.foo)
    #[allow(dead_code)]
    pub fn pop_last(&mut self) -> Option<(String, JsonPathToken)> {
        let last = self.root.next_back();
        match last {
            Some(last) => match last.as_rule() {
                Rule::literal => Some((last.as_str().to_string(), JsonPathToken::String)),
                Rule::number => Some((last.as_str().to_string(), JsonPathToken::Number)),
                Rule::numbers_list => {
                    let first_on_list = last.into_inner().next();
                    match first_on_list {
                        Some(first) => Some((first.as_str().to_string(), JsonPathToken::Number)),
                        None => None,
                    }
                }
                Rule::string_list => {
                    let first_on_list = last.into_inner().next();
                    match first_on_list {
                        Some(first) => Some((first.as_str().to_string(), JsonPathToken::String)),
                        None => None,
                    }
                }
                _ => panic!("pop last was used in a none static path"),
            },
            None => None,
        }
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
        let mut root_copy = self.root.clone();
        while let Some(n) = root_copy.next() {
            size = size + 1;
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

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Rule::literal => write!(f, "<string>"),
            Rule::all => write!(f, "'*'"),
            Rule::full_scan => write!(f, "'..'"),
            Rule::numbers_list => write!(f, "'<number>[,<number>,...]'"),
            Rule::string_list => write!(f, "'<string>[,<string>,...]'"),
            Rule::numbers_range => write!(f, "['start:end:steps']"),
            Rule::number => write!(f, "'<number>'"),
            Rule::filter => write!(f, "'[?(filter_expression)]'"),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Compile the given string query into a query object.
/// Returns error on compilation error.
pub(crate) fn compile(path: &str) -> Result<Query, QueryCompilationError> {
    let query = JsonPathParser::parse(Rule::query, path);
    match query {
        Ok(mut q) => {
            let root = q.next().unwrap();
            Ok(Query {
                root: root.into_inner(),
                is_static: None,
                size: None,
            })
        }
        // pest::error::Error
        Err(e) => {
            let pos = match e.location {
                pest::error::InputLocation::Pos(pos) => pos,
                pest::error::InputLocation::Span((pos, _end)) => pos,
            };
            let msg = match e.variant {
                pest::error::ErrorVariant::ParsingError {
                    ref positives,
                    ref negatives,
                } => {
                    let positives = if positives.is_empty() {
                        None
                    } else {
                        Some(
                            positives
                                .iter()
                                .map(|v| format!("{}", v))
                                .collect::<Vec<_>>()
                                .join(", "),
                        )
                    };
                    let negatives = if negatives.is_empty() {
                        None
                    } else {
                        Some(
                            negatives
                                .iter()
                                .map(|v| format!("{}", v))
                                .collect::<Vec<_>>()
                                .join(", "),
                        )
                    };

                    match (positives, negatives) {
                        (None, None) => "parsing error".to_string(),
                        (Some(p), None) => format!("expected one of the following: {}", p),
                        (None, Some(n)) => format!("unexpected tokens found: {}", n),
                        (Some(p), Some(n)) => format!(
                            "expected one of the following: {}, unexpected tokens found: {}",
                            p, n
                        ),
                    }
                }
                pest::error::ErrorVariant::CustomError { ref message } => message.clone(),
            };

            let final_msg = if pos == path.len() {
                format!("\"{} <<<<----\", {}.", path, msg)
            } else {
                format!("\"{} ---->>>> {}\", {}.", &path[..pos], &path[pos..], msg)
            };
            Err(QueryCompilationError {
                location: pos,
                message: final_msg,
            })
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

#[derive(Debug, PartialEq)]
pub enum PTrackerElement {
    Key(String),
    Index(usize),
}

/* An actual representation of a path that the user gets as a result. */
#[derive(Debug, PartialEq)]
pub struct PTracker {
    pub elemenets: Vec<PTrackerElement>,
}
impl UserPathTracker for PTracker {
    fn add_str(&mut self, s: &str) {
        self.elemenets.push(PTrackerElement::Key(s.to_string()));
    }

    fn add_index(&mut self, i: usize) {
        self.elemenets.push(PTrackerElement::Index(i));
    }

    fn to_string_path(self) -> Vec<String> {
        self.elemenets
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
            elemenets: Vec::new(),
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

const fn create_empty_tracker<'i, 'j>() -> PathTracker<'i, 'j> {
    PathTracker {
        parent: None,
        element: PathTrackerElement::Root,
    }
}

const fn create_str_tracker<'i, 'j>(
    s: &'i str,
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
    Value(&'j S),
    Bool(bool),
    Invalid,
}

enum CmpResult {
    Ord(Ordering),
    NotCmparable,
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
                CmpResult::Ord((&s1[..]).cmp(s2))
            }
            (TermEvaluationResult::String(s1), TermEvaluationResult::String(s2)) => {
                CmpResult::Ord(s1.cmp(s2))
            }
            (TermEvaluationResult::Bool(b1), TermEvaluationResult::Bool(b2)) => {
                CmpResult::Ord(b1.cmp(b2))
            }
            (TermEvaluationResult::Value(v), _) => match v.get_type() {
                SelectValueType::Long => TermEvaluationResult::Integer(v.get_long()).cmp(s),
                SelectValueType::Double => TermEvaluationResult::Float(v.get_double()).cmp(s),
                SelectValueType::String => TermEvaluationResult::Str(v.as_str()).cmp(s),
                SelectValueType::Bool => TermEvaluationResult::Bool(v.get_bool()).cmp(s),
                _ => CmpResult::NotCmparable,
            },
            (_, TermEvaluationResult::Value(v)) => match v.get_type() {
                SelectValueType::Long => self.cmp(&TermEvaluationResult::Integer(v.get_long())),
                SelectValueType::Double => self.cmp(&TermEvaluationResult::Float(v.get_double())),
                SelectValueType::String => self.cmp(&TermEvaluationResult::Str(v.as_str())),
                SelectValueType::Bool => self.cmp(&TermEvaluationResult::Bool(v.get_bool())),
                _ => CmpResult::NotCmparable,
            },
            (_, _) => CmpResult::NotCmparable,
        }
    }
    fn gt(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_gt(),
            CmpResult::NotCmparable => false,
        }
    }

    fn ge(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_ge(),
            CmpResult::NotCmparable => false,
        }
    }

    fn lt(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_lt(),
            CmpResult::NotCmparable => false,
        }
    }

    fn le(&self, s: &Self) -> bool {
        match self.cmp(s) {
            CmpResult::Ord(o) => o.is_le(),
            CmpResult::NotCmparable => false,
        }
    }

    fn eq(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => v1 == v2,
            (_, _) => match self.cmp(s) {
                CmpResult::Ord(o) => o.is_eq(),
                CmpResult::NotCmparable => false,
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
                    SelectValueType::String => Self::re_is_match(regex, v.as_str()),
                    _ => false,
                }
            }
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => {
                match (v1.get_type(), v2.get_type()) {
                    (SelectValueType::String, SelectValueType::String) => {
                        Self::re_is_match(v2.as_str(), v1.as_str())
                    }
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

#[derive(Debug, PartialEq)]
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
    pub fn create(query: &'i Query<'i>) -> PathCalculator<'i, UPTG> {
        PathCalculator {
            query: Some(query),
            tracker_generator: None,
        }
    }

    #[allow(dead_code)]
    pub fn create_with_generator(
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
                    let items = json.items().unwrap();
                    for (key, val) in items {
                        self.calc_internal(
                            pairs.clone(),
                            val,
                            Some(create_str_tracker(key, &pt)),
                            calc_data,
                        );
                        self.calc_full_scan(
                            pairs.clone(),
                            val,
                            Some(create_str_tracker(key, &pt)),
                            calc_data,
                        );
                    }
                } else {
                    let values = json.values().unwrap();
                    for v in values {
                        self.calc_internal(pairs.clone(), v, None, calc_data);
                        self.calc_full_scan(pairs.clone(), v, None, calc_data);
                    }
                }
            }
            SelectValueType::Array => {
                let values = json.values().unwrap();
                if let Some(pt) = path_tracker {
                    for (i, v) in values.enumerate() {
                        self.calc_internal(
                            pairs.clone(),
                            v,
                            Some(create_index_tracker(i, &pt)),
                            calc_data,
                        );
                        self.calc_full_scan(
                            pairs.clone(),
                            v,
                            Some(create_index_tracker(i, &pt)),
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
                        let new_tracker = Some(create_str_tracker(key, &pt));
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
                        let new_tracker = Some(create_index_tracker(i, &pt));
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
                let new_tracker = Some(create_str_tracker(curr.as_str(), &pt));
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
                    _ => panic!("{}", format!("{:?}", c)),
                };
                if let Some(e) = curr_val {
                    let new_tracker = Some(create_str_tracker(s, &pt));
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
                    _ => panic!("{}", format!("{:?}", c)),
                };
                if let Some(e) = curr_val {
                    self.calc_internal(pairs.clone(), e, None, calc_data);
                }
            }
        }
    }

    fn calc_abs_index(&self, i: i64, n: usize) -> usize {
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
                let i = self.calc_abs_index(c.as_str().parse::<i64>().unwrap(), n);
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    let new_tracker = Some(create_index_tracker(i, &pt));
                    self.calc_internal(pairs.clone(), e, new_tracker, calc_data);
                }
            }
        } else {
            for c in curr.into_inner() {
                let i = self.calc_abs_index(c.as_str().parse::<i64>().unwrap(), n);
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
                    self.calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let step = match curr.next() {
                    Some(s) => s.as_str().parse::<usize>().unwrap(),
                    None => 1,
                };
                (start, end, step)
            }
            Rule::all_range => {
                let mut curr = curr.into_inner();
                let step = match curr.next() {
                    Some(s) => s.as_str().parse::<usize>().unwrap(),
                    None => 1,
                };
                (0, n, step)
            }
            Rule::left_range => {
                let mut curr = curr.into_inner();
                let start =
                    self.calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let end = n;
                let step = match curr.next() {
                    Some(s) => s.as_str().parse::<usize>().unwrap(),
                    None => 1,
                };
                (start, end, step)
            }
            Rule::full_range => {
                let mut curr = curr.into_inner();
                let start =
                    self.calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let end =
                    self.calc_abs_index(curr.next().unwrap().as_str().parse::<i64>().unwrap(), n);
                let step = match curr.next() {
                    Some(s) => s.as_str().parse::<usize>().unwrap(),
                    None => 1,
                };
                (start, end, step)
            }
            _ => panic!("{}", format!("{:?}", curr)),
        };

        if let Some(pt) = path_tracker {
            for i in (start..end).step_by(step) {
                let curr_val = json.get_index(i);
                if let Some(e) = curr_val {
                    let new_tracker = Some(create_index_tracker(i, &pt));
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
                panic!("{}", format!("{:?}", term))
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
                Rule::gt => term1_val.gt(&term2_val),
                Rule::ge => term1_val.ge(&term2_val),
                Rule::lt => term1_val.lt(&term2_val),
                Rule::le => term1_val.le(&term2_val),
                Rule::eq => term1_val.eq(&term2_val),
                Rule::ne => term1_val.ne(&term2_val),
                Rule::re => term1_val.re(&term2_val),
                _ => panic!("{}", format!("{:?}", op)),
            }
        } else {
            !matches!(term1_val, TermEvaluationResult::Invalid)
        }
    }

    fn evaluate_filter<'j: 'i, S: SelectValue>(
        &self,
        curr: Pair<'i, Rule>,
        json: &'j S,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        let mut curr = curr.into_inner();
        let first_filter = curr.next().unwrap();
        trace!("evaluate_filter first_filter {:?}", &first_filter);
        let first_result = match first_filter.as_rule() {
            Rule::single_filter => self.evaluate_single_filter(first_filter, json, calc_data),
            Rule::filter => self.evaluate_filter(first_filter, json, calc_data),
            _ => panic!("{}", format!("{:?}", first_filter)),
        };
        trace!("evaluate_filter first_result {:?}", &first_result);

        if let Some(relation) = curr.next() {
            trace!("evaluate_filter relation {:?}", &relation);
            let relation_callback = match relation.as_rule() {
                Rule::and => |a: bool, b: bool| a && b,
                Rule::or => |a: bool, b: bool| a || b,
                _ => panic!("{}", format!("{:?}", relation)),
            };
            let second_filter = curr.next().unwrap();
            trace!("evaluate_filter second_filter {:?}", &second_filter);
            let second_result = match second_filter.as_rule() {
                Rule::single_filter => self.evaluate_single_filter(second_filter, json, calc_data),
                Rule::filter => self.evaluate_filter(second_filter, json, calc_data),
                _ => panic!("{}", format!("{:?}", second_filter)),
            };
            trace!("evaluate_filter second_result {:?}", &second_result);
            trace!(
                "evaluate_filter relation_callback {:?}",
                relation_callback(first_result, second_result)
            );
            relation_callback(first_result, second_result)
        } else {
            first_result
        }
    }

    fn populate_path_tracker<'k, 'l>(&self, pt: &PathTracker<'l, 'k>, upt: &mut UPTG::PT) {
        if let Some(f) = pt.parent {
            self.populate_path_tracker(f, upt);
        }
        match pt.element {
            PathTrackerElement::Index(i) => upt.add_index(i),
            PathTrackerElement::Key(k) => upt.add_str(k),
            PathTrackerElement::Root => {}
        }
    }

    fn generate_path(&self, pt: PathTracker) -> UPTG::PT {
        let mut upt = self.tracker_generator.as_ref().unwrap().generate();
        self.populate_path_tracker(&pt, &mut upt);
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
                        if json.get_type() == SelectValueType::Array
                            || json.get_type() == SelectValueType::Object
                        {
                            /* lets expend the array, this is how most json path engines work.
                             * Pesonally, I think this if should not exists. */
                            let values = json.values().unwrap();
                            if let Some(pt) = path_tracker {
                                trace!(
                                    "calc_internal type {:?} path_tracker {:?}",
                                    json.get_type(),
                                    &pt
                                );
                                for (i, v) in values.enumerate() {
                                    trace!("calc_internal v {:?}", &v);
                                    if self.evaluate_filter(curr.clone(), v, calc_data) {
                                        let new_tracker = Some(create_index_tracker(i, &pt));
                                        self.calc_internal(
                                            pairs.clone(),
                                            v,
                                            new_tracker,
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
                                    if self.evaluate_filter(curr.clone(), v, calc_data) {
                                        self.calc_internal(pairs.clone(), v, None, calc_data);
                                    }
                                }
                            }
                        } else if self.evaluate_filter(curr.clone(), json, calc_data) {
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
                    _ => panic!("{}", format!("{:?}", curr)),
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
            self.calc_internal(root, json, Some(create_empty_tracker()), &mut calc_data);
        } else {
            self.calc_internal(root, json, None, &mut calc_data);
        }
        calc_data.results.drain(..).collect()
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
    use crate::jsonpath::json_path::compile;
    use crate::jsonpath::json_path::JsonPathToken;

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
    fn test_compiler_pop_last_string_brucket_notation() {
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
