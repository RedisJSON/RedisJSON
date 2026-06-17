/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use crate::select_value::{is_equal, SelectValue, SelectValueType, ValueRef};
use itertools::Itertools;
use log::trace;
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;
use redis_module::rediserror::RedisError;
use regex::Regex;
use serde_json::Value;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
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

/// Recursively build an owned JSON `Value` from an array/object literal parse subtree
/// (e.g. `[1,{"a":2}]`). Used for structured filter operands; compared against the
/// document value via the cross-type `is_equal`.
///
/// PERF/TODO: this runs per element during filter evaluation, so a constant literal
/// (e.g. the array in `$.a[?@ in [1,2,...]]`) is rebuilt for every node. A follow-up
/// could materialize literals once at `compile()` time and reuse them.
fn build_literal(pair: Pair<Rule>) -> Value {
    match pair.as_rule() {
        Rule::array_literal => Value::Array(pair.into_inner().map(build_literal).collect()),
        Rule::object_literal => Value::Object(
            pair.into_inner()
                .map(|member| {
                    let mut it = member.into_inner();
                    let key = it
                        .next()
                        .map(|k| unescape_string_value(k).into_owned())
                        .unwrap_or_default();
                    let value = it.next().map_or(Value::Null, build_literal);
                    (key, value)
                })
                .collect(),
        ),
        Rule::decimal => {
            let s = pair.as_str();
            if let Ok(i) = s.parse::<i64>() {
                Value::from(i)
            } else if let Some(n) = s.parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
                Value::Number(n)
            } else {
                Value::Null
            }
        }
        Rule::string_value | Rule::string_value_escape_1 | Rule::string_value_escape_2 => {
            Value::String(unescape_string_value(pair).into_owned())
        }
        Rule::boolean_true => Value::Bool(true),
        Rule::boolean_false => Value::Bool(false),
        Rule::null => Value::Null,
        other => {
            trace!("build_literal: unexpected rule {other:?}");
            Value::Null
        }
    }
}

/// A numeric value extracted from a term/value for membership comparison.
#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    Float(f64),
}

/// Numeric equality matching `==`: exact for two integers, by `f64` value otherwise.
///
/// NOTE: mixed int/float comparison goes through `f64`, which loses precision above
/// 2^53 — e.g. `9007199254740993` and `9007199254740992` compare equal. This matches
/// the existing `==`/`is_equal` numeric coercion, but `in`/`nin` now route through here
/// too, so large-integer membership can match a near-but-unequal value.
fn numbers_equal(a: Num, b: Num) -> bool {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => x == y,
        (Num::Float(x), Num::Float(y)) => x == y,
        (Num::Int(x), Num::Float(y)) | (Num::Float(y), Num::Int(x)) => x as f64 == y,
    }
}

/// Numeric view of a `SelectValue` (integer or double), `None` if not a number.
fn value_as_number<V: SelectValue>(v: &V) -> Option<Num> {
    match v.get_type() {
        SelectValueType::Long => v.get_long().map(Num::Int),
        SelectValueType::Double => v.get_double().map(Num::Float),
        _ => None,
    }
}

/// Equality between two document values with `in`/`nin` number coercion: integers and
/// doubles compare by numeric value (`1` == `1.0`); everything else uses deep `is_equal`
/// equality. Mirrors `equals_value`, so the set operators agree with `in`/`nin`.
fn values_equal<A: SelectValue, B: SelectValue>(a: &A, b: &B) -> bool {
    match (value_as_number(a), value_as_number(b)) {
        (Some(x), Some(y)) => numbers_equal(x, y),
        _ => is_equal(a, b),
    }
}

/// True if `needle` equals any element of the array-shaped term `haystack` (array value,
/// array literal, or nodelist). Comparison coerces numbers like `in`/`nin` (`1` == `1.0`)
/// and uses deep `is_equal` for everything else, so the set operators agree with
/// membership. Non-array `haystack` ⇒ false.
fn value_in_array<'i, 'j, S: SelectValue, V: SelectValue>(
    needle: &V,
    haystack: &TermEvaluationResult<'i, 'j, S>,
) -> bool {
    match haystack {
        TermEvaluationResult::Value(v) if v.as_ref().get_type() == SelectValueType::Array => v
            .as_ref()
            .values()
            .is_some_and(|mut it| it.any(|e| values_equal(needle, e.as_ref()))),
        TermEvaluationResult::Literal(Value::Array(items)) => {
            items.iter().any(|it| values_equal(needle, it))
        }
        TermEvaluationResult::NodeList(list) => {
            list.iter().any(|v| values_equal(needle, v.as_ref()))
        }
        _ => false,
    }
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }
}

/// Apply a binary arithmetic operator to two numbers. Integer operands stay integer
/// for `+ - *` (falling back to float on overflow) and `%`; division is always float.
///
/// Division/modulo by zero returns `None`, which the caller turns into `Invalid`
/// (RFC 9535 "Nothing"): the surrounding comparison then evaluates to false and the
/// node is skipped. NOTE: rather than raising a hard "division by zero" error, we
/// return Nothing because the path evaluator has no error channel — every operand
/// resolves to a value or to Nothing, never to an error — so a single bad element
/// cannot abort the whole command. (Same
/// rule applies to any non-numeric arithmetic operand, e.g. `@.str * 2` → Nothing.)
fn num_binop<'i, 'j, S: SelectValue>(
    operator: Rule,
    a: Num,
    b: Num,
) -> Option<TermEvaluationResult<'i, 'j, S>> {
    use Num::Int;
    match operator {
        Rule::add | Rule::sub | Rule::mul => {
            if let (Int(x), Int(y)) = (a, b) {
                let checked = match operator {
                    Rule::add => x.checked_add(y),
                    Rule::sub => x.checked_sub(y),
                    Rule::mul => x.checked_mul(y),
                    _ => unreachable!("num_binop: outer arm guarantees add/sub/mul"),
                };
                if let Some(v) = checked {
                    return Some(TermEvaluationResult::Integer(v));
                }
            }
            let (x, y) = (a.as_f64(), b.as_f64());
            let v = match operator {
                Rule::add => x + y,
                Rule::sub => x - y,
                Rule::mul => x * y,
                _ => unreachable!("num_binop: outer arm guarantees add/sub/mul"),
            };
            Some(TermEvaluationResult::Float(v))
        }
        Rule::div => {
            let y = b.as_f64();
            (y != 0.0).then(|| TermEvaluationResult::Float(a.as_f64() / y))
        }
        Rule::rem => match (a, b) {
            // `checked_rem` yields None for a zero divisor and for the i64::MIN % -1
            // overflow, both of which become Nothing (rather than panicking).
            (Int(x), Int(y)) => x.checked_rem(y).map(TermEvaluationResult::Integer),
            _ => {
                let y = b.as_f64();
                (y != 0.0).then(|| TermEvaluationResult::Float(a.as_f64() % y))
            }
        },
        _ => None,
    }
}

/// Apply a binary arithmetic operator to two term results (both must be numbers).
fn arith_binop<'i, 'j, S: SelectValue>(
    operator: Rule,
    a: &TermEvaluationResult<'i, 'j, S>,
    b: &TermEvaluationResult<'i, 'j, S>,
) -> TermEvaluationResult<'i, 'j, S> {
    match (a.as_number(), b.as_number()) {
        (Some(x), Some(y)) => num_binop(operator, x, y).unwrap_or(TermEvaluationResult::Invalid),
        _ => TermEvaluationResult::Invalid,
    }
}

/// Apply a unary `+`/`-` to a term result (must be a number).
fn arith_unary<'i, 'j, S: SelectValue>(
    operator: Rule,
    v: TermEvaluationResult<'i, 'j, S>,
) -> TermEvaluationResult<'i, 'j, S> {
    match v.as_number() {
        Some(Num::Int(n)) => match operator {
            Rule::neg => n.checked_neg().map_or_else(
                || TermEvaluationResult::Float(-(n as f64)),
                TermEvaluationResult::Integer,
            ),
            _ => TermEvaluationResult::Integer(n),
        },
        Some(Num::Float(f)) => match operator {
            Rule::neg => TermEvaluationResult::Float(-f),
            _ => TermEvaluationResult::Float(f),
        },
        None => TermEvaluationResult::Invalid,
    }
}

/// RFC 9535 `length()` on a value: chars for strings, element/member count for
/// arrays/objects, `None` (Nothing) otherwise. Generic over any `SelectValue`.
fn value_length<V: SelectValue>(v: &V) -> Option<usize> {
    match v.get_type() {
        SelectValueType::String => v.as_str().map(|s| s.chars().count()),
        SelectValueType::Array | SelectValueType::Object => v.len(),
        _ => None,
    }
}

fn function_length<'i, 'j, S: SelectValue>(
    arg: &TermEvaluationResult<'i, 'j, S>,
) -> TermEvaluationResult<'i, 'j, S> {
    match arg {
        TermEvaluationResult::Str(s) => Some(s.chars().count()),
        TermEvaluationResult::String(s) => Some(s.chars().count()),
        TermEvaluationResult::Value(v) => value_length(v.as_ref()),
        TermEvaluationResult::Literal(l) => value_length(l),
        TermEvaluationResult::NodeList(list) if list.len() == 1 => value_length(list[0].as_ref()),
        TermEvaluationResult::NodeList(_)
        | TermEvaluationResult::Integer(_)
        | TermEvaluationResult::Float(_)
        | TermEvaluationResult::Bool(_)
        | TermEvaluationResult::Null
        | TermEvaluationResult::Invalid => None,
    }
    .map_or(TermEvaluationResult::Invalid, |n| {
        TermEvaluationResult::Integer(n as i64)
    })
}

/// RFC 9535 `count()`: number of nodes in a nodelist. A single value counts as 1,
/// an empty/absent query (`Invalid`) as 0.
fn function_count<'i, 'j, S: SelectValue>(arg: &TermEvaluationResult<'i, 'j, S>) -> i64 {
    match arg {
        TermEvaluationResult::Invalid => 0,
        TermEvaluationResult::NodeList(list) => list.len() as i64,
        TermEvaluationResult::Integer(_)
        | TermEvaluationResult::Float(_)
        | TermEvaluationResult::Str(_)
        | TermEvaluationResult::String(_)
        | TermEvaluationResult::Value(_)
        | TermEvaluationResult::Literal(_)
        | TermEvaluationResult::Bool(_)
        | TermEvaluationResult::Null => 1,
    }
}

/// RFC 9535 `value()`: the value of a single-node nodelist, otherwise Nothing.
fn function_value<'i, 'j, S: SelectValue>(
    arg: TermEvaluationResult<'i, 'j, S>,
) -> TermEvaluationResult<'i, 'j, S> {
    match arg {
        TermEvaluationResult::NodeList(mut list) if list.len() == 1 => list
            .pop()
            .map_or(TermEvaluationResult::Invalid, TermEvaluationResult::Value),
        v @ TermEvaluationResult::Value(_) => v,
        TermEvaluationResult::NodeList(_)
        | TermEvaluationResult::Integer(_)
        | TermEvaluationResult::Float(_)
        | TermEvaluationResult::Str(_)
        | TermEvaluationResult::String(_)
        | TermEvaluationResult::Literal(_)
        | TermEvaluationResult::Bool(_)
        | TermEvaluationResult::Null
        | TermEvaluationResult::Invalid => TermEvaluationResult::Invalid,
    }
}

/// Borrow the string content of a term result (for `match`/`search` operands).
fn term_as_str<'a, 'i, 'j, S: SelectValue>(
    arg: &'a TermEvaluationResult<'i, 'j, S>,
) -> Option<&'a str> {
    match arg {
        TermEvaluationResult::Str(s) => Some(*s),
        TermEvaluationResult::String(s) => Some(s.as_str()),
        TermEvaluationResult::Value(v) => v.as_ref().as_str(),
        TermEvaluationResult::Literal(l) => l.as_str(),
        TermEvaluationResult::Integer(_)
        | TermEvaluationResult::Float(_)
        | TermEvaluationResult::Bool(_)
        | TermEvaluationResult::Null
        | TermEvaluationResult::NodeList(_)
        | TermEvaluationResult::Invalid => None,
    }
}

type RegexCache = HashMap<String, Option<Regex>>;

/// Compile `pattern` (caching the result in `cache`) and test it against `s`. `full`
/// anchors the pattern for RFC 9535 `match()`; otherwise it is a substring search
/// (`search()` / the `=~` operator). A constant pattern is invariant across the elements
/// of a filter, so the cache compiles it once per query instead of once per element.
fn regex_matches(cache: &mut RegexCache, pattern: &str, full: bool, s: &str) -> bool {
    // Past the cap we compile uncached; already-cached patterns (the common constant case) still hit.
    const MAX_REGEX_CACHE: usize = 64;
    let key = if full {
        format!("^(?:{pattern})$")
    } else {
        pattern.to_string()
    };
    if cache.len() < MAX_REGEX_CACHE || cache.contains_key(&key) {
        cache
            .entry(key)
            .or_insert_with_key(|k| Regex::new(k).ok())
            .as_ref()
            .is_some_and(|re| re.is_match(s))
    } else {
        Regex::new(&key).is_ok_and(|re| re.is_match(s))
    }
}

/// Convert a finite `f64` to `i64`, returning `None` when it falls outside the `i64`
/// range (rather than saturating, as a bare `as i64` would). Callers round or truncate
/// first; this enforces the overflow/range policy in one place.
fn f64_to_i64(v: f64) -> Option<i64> {
    // `i64::MAX` is not exactly representable in f64: `i64::MAX as f64` rounds up to 2^63
    // (one past MAX), so the upper bound must be strict `<` to reject it. `i64::MIN`
    // (-2^63) is exact, so `>=` is correct there.
    (v.is_finite() && v >= i64::MIN as f64 && v < i64::MAX as f64).then_some(v as i64)
}

/// Dispatch a filter-expression function call to its RFC 9535 implementation.
/// `ceiling(n)`/`floor(n)`: round a number to an integer using `round`
/// (`f64::ceil`/`f64::floor`). Integers pass through unchanged; a non-numeric argument
/// or a result outside the `i64` range is Nothing.
fn function_round<'i, 'j, S: SelectValue>(
    arg: Option<&TermEvaluationResult<'i, 'j, S>>,
    round: fn(f64) -> f64,
) -> TermEvaluationResult<'i, 'j, S> {
    match arg.and_then(TermEvaluationResult::as_number) {
        Some(Num::Int(n)) => TermEvaluationResult::Integer(n),
        Some(Num::Float(f)) => f64_to_i64(round(f))
            .map_or(TermEvaluationResult::Invalid, TermEvaluationResult::Integer),
        None => TermEvaluationResult::Invalid,
    }
}

/// `abs(n)`: absolute value. Integers stay integers (i64::MIN overflows -> Nothing);
/// doubles stay doubles. A non-numeric argument is Nothing.
fn function_abs<'i, 'j, S: SelectValue>(
    arg: Option<&TermEvaluationResult<'i, 'j, S>>,
) -> TermEvaluationResult<'i, 'j, S> {
    match arg.and_then(TermEvaluationResult::as_number) {
        Some(Num::Int(n)) => n
            .checked_abs()
            .map_or(TermEvaluationResult::Invalid, TermEvaluationResult::Integer),
        Some(Num::Float(f)) => TermEvaluationResult::Float(f.abs()),
        None => TermEvaluationResult::Invalid,
    }
}

/// `concat(s1, s2, ...)`: concatenate string arguments into one string. Any non-string
/// argument yields Nothing.
fn function_concat<'i, 'j, S: SelectValue>(
    args: &[TermEvaluationResult<'i, 'j, S>],
) -> TermEvaluationResult<'i, 'j, S> {
    let mut out = String::new();
    for a in args {
        match term_as_str(a) {
            Some(s) => out.push_str(s),
            None => return TermEvaluationResult::Invalid,
        }
    }
    TermEvaluationResult::String(out)
}

/// Collect the elements of an array-shaped term (array value, array literal, or
/// nodelist) as `f64`s for the numeric aggregations. Nothing (`None`) if the term is not
/// an array, is empty, or contains a non-numeric element.
fn term_number_seq<'i, 'j, S: SelectValue>(
    arg: &TermEvaluationResult<'i, 'j, S>,
) -> Option<Vec<f64>> {
    let mut out = Vec::new();
    match arg {
        TermEvaluationResult::Value(v) if v.as_ref().get_type() == SelectValueType::Array => {
            for e in v.as_ref().values()? {
                out.push(value_as_number(e.as_ref())?.as_f64());
            }
        }
        TermEvaluationResult::Literal(Value::Array(items)) => {
            for it in items {
                out.push(value_as_number(it)?.as_f64());
            }
        }
        TermEvaluationResult::NodeList(list) => {
            for v in list {
                out.push(value_as_number(v.as_ref())?.as_f64());
            }
        }
        _ => return None,
    }
    (!out.is_empty()).then_some(out)
}

fn agg_sum(s: &[f64]) -> f64 {
    s.iter().sum()
}
fn agg_min(s: &[f64]) -> f64 {
    s.iter().copied().fold(f64::INFINITY, f64::min)
}
fn agg_max(s: &[f64]) -> f64 {
    s.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
fn agg_avg(s: &[f64]) -> f64 {
    // Guaranteed non-empty by `term_number_seq` (returns Nothing for an empty array).
    debug_assert!(!s.is_empty(), "agg over an empty sequence");
    s.iter().sum::<f64>() / s.len() as f64
}
/// Population standard deviation (divides by N).
fn agg_stddev(s: &[f64]) -> f64 {
    debug_assert!(!s.is_empty(), "agg over an empty sequence");
    let mean = agg_avg(s);
    let variance = s.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / s.len() as f64;
    variance.sqrt()
}

/// `min`/`max`/`avg`/`sum`/`stddev`: reduce an array of numbers to a double. A non-array
/// argument, a non-numeric element, or an empty array is Nothing.
fn function_aggregate<'i, 'j, S: SelectValue>(
    arg: Option<&TermEvaluationResult<'i, 'j, S>>,
    agg: fn(&[f64]) -> f64,
) -> TermEvaluationResult<'i, 'j, S> {
    match arg.and_then(term_number_seq) {
        Some(seq) => TermEvaluationResult::Float(agg(&seq)),
        None => TermEvaluationResult::Invalid,
    }
}

/// `first(a)`/`last(a)`/`index(a, n)`: the element at a (possibly negative) index of an
/// array. Out-of-range, non-array, or non-integer index is Nothing. Takes the argument by
/// value so the element can be moved/borrowed out with the document lifetime `'j`.
fn function_index<'i, 'j, S: SelectValue>(
    arg: Option<TermEvaluationResult<'i, 'j, S>>,
    idx: i64,
) -> TermEvaluationResult<'i, 'j, S> {
    // Map a signed index (negative counts from the end) into `0..len`. A negative result
    // fails `try_from` (-> None); the filter then bounds-checks against `len`.
    fn resolve(idx: i64, len: usize) -> Option<usize> {
        let i = if idx < 0 { idx + len as i64 } else { idx };
        usize::try_from(i).ok().filter(|&u| u < len)
    }
    match arg {
        Some(TermEvaluationResult::Value(v)) if v.as_ref().get_type() == SelectValueType::Array => {
            let Some(i) = resolve(idx, v.as_ref().len().unwrap_or(0)) else {
                return TermEvaluationResult::Invalid;
            };
            match v {
                // A borrowed array yields an element borrowed for the same `'j`.
                ValueRef::Borrowed(r) => r
                    .get_index(i)
                    .map_or(TermEvaluationResult::Invalid, TermEvaluationResult::Value),
                // An owned array must clone the element out before it is dropped.
                ValueRef::Owned(s) => s.get_index(i).map_or(TermEvaluationResult::Invalid, |e| {
                    TermEvaluationResult::Value(ValueRef::Owned(e.inner_cloned()))
                }),
            }
        }
        Some(TermEvaluationResult::Literal(Value::Array(mut items))) => {
            match resolve(idx, items.len()) {
                Some(i) => TermEvaluationResult::Literal(items.swap_remove(i)),
                None => TermEvaluationResult::Invalid,
            }
        }
        Some(TermEvaluationResult::NodeList(mut list)) => match resolve(idx, list.len()) {
            Some(i) => TermEvaluationResult::Value(list.swap_remove(i)),
            None => TermEvaluationResult::Invalid,
        },
        _ => TermEvaluationResult::Invalid,
    }
}

fn eval_function<'i, 'j, S: SelectValue>(
    name: &str,
    mut args: Vec<TermEvaluationResult<'i, 'j, S>>,
    cache: &mut RegexCache,
) -> TermEvaluationResult<'i, 'j, S> {
    // Reject the wrong number of arguments (-> Nothing) for this PR's functions, so a
    // malformed query like `ceiling(@, 9)` or `concat()` doesn't silently operate on a
    // subset of its arguments instead of failing.
    let arity_ok = match name {
        "concat" => !args.is_empty(),
        "index" => args.len() == 2,
        "ceiling" | "floor" | "abs" | "sum" | "min" | "max" | "avg" | "stddev" | "first"
        | "last" => args.len() == 1,
        _ => true,
    };
    if !arity_ok {
        return TermEvaluationResult::Invalid;
    }
    match name {
        "length" => args
            .first()
            .map_or(TermEvaluationResult::Invalid, function_length),
        "count" => args.first().map_or(TermEvaluationResult::Invalid, |a| {
            TermEvaluationResult::Integer(function_count(a))
        }),
        "value" if args.len() == 1 => function_value(args.pop().unwrap()),
        "ceiling" => function_round(args.first(), f64::ceil),
        "floor" => function_round(args.first(), f64::floor),
        "abs" => function_abs(args.first()),
        "concat" => function_concat(&args),
        "sum" => function_aggregate(args.first(), agg_sum),
        "min" => function_aggregate(args.first(), agg_min),
        "max" => function_aggregate(args.first(), agg_max),
        "avg" => function_aggregate(args.first(), agg_avg),
        "stddev" => function_aggregate(args.first(), agg_stddev),
        "first" => function_index(args.into_iter().next(), 0),
        "last" => function_index(args.into_iter().next(), -1),
        "index" => {
            // index(array, n) — a fractional n is truncated toward zero; a non-numeric n
            // is Nothing.
            let mut it = args.into_iter();
            let array = it.next();
            let idx = it.next().and_then(|a| match a.as_number() {
                Some(Num::Int(n)) => Some(n),
                Some(Num::Float(f)) => f64_to_i64(f.trunc()),
                _ => None,
            });
            idx.map_or(TermEvaluationResult::Invalid, |n| function_index(array, n))
        }
        "match" | "search" => {
            let full = name == "match";
            let s = args.first().and_then(term_as_str);
            let re = args.get(1).and_then(term_as_str);
            match (s, re) {
                (Some(s), Some(re)) => {
                    TermEvaluationResult::Bool(regex_matches(cache, re, full, s))
                }
                _ => TermEvaluationResult::Bool(false),
            }
        }
        other => {
            trace!("eval_function: unknown function {other:?}");
            TermEvaluationResult::Invalid
        }
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
    /// An array/object literal operand, e.g. `[1,2]` or `{"a":1}` in `?@==[1,2]`.
    Literal(Value),
    Bool(bool),
    Null,
    /// Multiple results from a non-singular query (e.g. `@.*`, `@..key`).
    /// Per RFC 9535, comparisons succeed if ANY element satisfies the condition.
    NodeList(Vec<ValueRef<'j, S>>),
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
    fn ord_cmp_matches(&self, s: &Self, pred: fn(Ordering) -> bool) -> bool {
        match (self, s) {
            (TermEvaluationResult::NodeList(list), _) => list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).ord_cmp_matches(s, pred)),
            (_, TermEvaluationResult::NodeList(list)) => list
                .iter()
                .any(|v| self.ord_cmp_matches(&TermEvaluationResult::Value(v.clone()), pred)),
            _ => match self.cmp(s) {
                CmpResult::Ord(o) => pred(o),
                CmpResult::NotComparable => false,
            },
        }
    }

    fn gt(&self, s: &Self) -> bool {
        self.ord_cmp_matches(s, Ordering::is_gt)
    }

    fn ge(&self, s: &Self) -> bool {
        self.ord_cmp_matches(s, Ordering::is_ge)
    }

    fn lt(&self, s: &Self) -> bool {
        self.ord_cmp_matches(s, Ordering::is_lt)
    }

    fn le(&self, s: &Self) -> bool {
        self.ord_cmp_matches(s, Ordering::is_le)
    }

    fn eq(&self, s: &Self) -> bool {
        match (self, s) {
            (TermEvaluationResult::NodeList(list), _) => list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).eq(s)),
            (_, TermEvaluationResult::NodeList(list)) => list
                .iter()
                .any(|v| self.eq(&TermEvaluationResult::Value(v.clone()))),
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => v1 == v2,
            // Structured literal operands deep-compare against the document value
            // (any `SelectValue`) via the cross-type `is_equal`. Like the existing
            // `Value == Value` path above, this is type-strict for numbers: nested
            // integers do not match doubles (e.g. `@ == [1]` does not match `[1.0]`),
            // unlike scalar `==` / `in` which coerce. Kept consistent with `is_equal`.
            (TermEvaluationResult::Value(v), TermEvaluationResult::Literal(l)) => {
                is_equal(v.as_ref(), l)
            }
            (TermEvaluationResult::Literal(l), TermEvaluationResult::Value(v)) => {
                is_equal(l, v.as_ref())
            }
            (TermEvaluationResult::Literal(l1), TermEvaluationResult::Literal(l2)) => {
                is_equal(l1, l2)
            }
            (_, _) => match self.cmp(s) {
                CmpResult::Ord(o) => o.is_eq(),
                CmpResult::NotComparable => false,
            },
        }
    }

    fn ne(&self, s: &Self) -> bool {
        !self.eq(s)
    }

    /// Numeric view of this term (integer or double), `None` if not a number.
    fn as_number(&self) -> Option<Num> {
        match self {
            TermEvaluationResult::Integer(n) => Some(Num::Int(*n)),
            TermEvaluationResult::Float(f) => Some(Num::Float(*f)),
            TermEvaluationResult::Value(v) => value_as_number(v.as_ref()),
            TermEvaluationResult::Literal(l) => value_as_number(l),
            TermEvaluationResult::Str(_)
            | TermEvaluationResult::String(_)
            | TermEvaluationResult::Bool(_)
            | TermEvaluationResult::Null
            | TermEvaluationResult::NodeList(_)
            | TermEvaluationResult::Invalid => None,
        }
    }

    /// Equality between this term and an arbitrary `SelectValue`, used by membership
    /// (`in`/`nin`). Numbers coerce across integer/float (matching `==`); strings,
    /// booleans, null and structured values use deep (`is_equal`) equality.
    fn equals_value<V: SelectValue>(&self, other: &V) -> bool {
        // Numbers compare by value (int/float coerce), matching `==`.
        if let (Some(a), Some(b)) = (self.as_number(), value_as_number(other)) {
            return numbers_equal(a, b);
        }
        match self {
            TermEvaluationResult::Value(v) => is_equal(v.as_ref(), other),
            TermEvaluationResult::Literal(l) => is_equal(l, other),
            TermEvaluationResult::NodeList(list) => {
                list.iter().any(|v| is_equal(v.as_ref(), other))
            }
            TermEvaluationResult::Str(s) => other.as_str() == Some(*s),
            TermEvaluationResult::String(s) => other.as_str() == Some(s.as_str()),
            TermEvaluationResult::Bool(b) => other.get_bool() == Some(*b),
            TermEvaluationResult::Null => other.get_type() == SelectValueType::Null,
            // self is numeric but `other` is not a number -> not equal
            TermEvaluationResult::Integer(_) | TermEvaluationResult::Float(_) => false,
            TermEvaluationResult::Invalid => false,
        }
    }

    /// Membership: true if `self` deep-equals any element of `arr`. `arr` must be an
    /// array value, an array literal, or a nodelist; anything else yields false.
    fn member_of(&self, arr: &Self) -> bool {
        match arr {
            TermEvaluationResult::Value(v) if v.as_ref().get_type() == SelectValueType::Array => v
                .as_ref()
                .values()
                .is_some_and(|mut it| it.any(|e| self.equals_value(e.as_ref()))),
            TermEvaluationResult::Literal(Value::Array(items)) => {
                items.iter().any(|it| self.equals_value(it))
            }
            TermEvaluationResult::NodeList(list) => {
                list.iter().any(|v| self.equals_value(v.as_ref()))
            }
            TermEvaluationResult::Value(_)
            | TermEvaluationResult::Literal(_)
            | TermEvaluationResult::Integer(_)
            | TermEvaluationResult::Float(_)
            | TermEvaluationResult::Str(_)
            | TermEvaluationResult::String(_)
            | TermEvaluationResult::Bool(_)
            | TermEvaluationResult::Null
            | TermEvaluationResult::Invalid => false,
        }
    }

    /// Set relation between two arrays (`subsetof`/`anyof`/`noneof`). Folds
    /// `value_in_array(element, rhs)` over the elements of the array-shaped `self`:
    /// `require_all` ⇒ every element must be in `rhs` (empty `self` ⇒ true);
    /// otherwise ⇒ any element is in `rhs` (empty `self` ⇒ false). A non-array `self`
    /// yields false (so `subsetof`/`anyof` are false and `noneof` is true).
    ///
    /// A multi-result (nodelist) left operand is handled any-of per node, matching
    /// `==`/`<`/`in`: the relation holds if any single matched node — itself array-shaped
    /// — satisfies it. (Without this, the nodelist itself would be taken as the left array
    /// and its nodes as elements, so array-valued nodes would never match.)
    fn set_relate(&self, rhs: &Self, require_all: bool) -> bool {
        if let TermEvaluationResult::NodeList(list) = self {
            return list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).set_relate(rhs, require_all));
        }
        fn combine(require_all: bool, mut it: impl Iterator<Item = bool>) -> bool {
            if require_all {
                it.all(|m| m)
            } else {
                it.any(|m| m)
            }
        }
        match self {
            TermEvaluationResult::Value(v) if v.as_ref().get_type() == SelectValueType::Array => {
                v.as_ref().values().is_some_and(|it| {
                    combine(require_all, it.map(|e| value_in_array(e.as_ref(), rhs)))
                })
            }
            TermEvaluationResult::Literal(Value::Array(items)) => {
                combine(require_all, items.iter().map(|e| value_in_array(e, rhs)))
            }
            TermEvaluationResult::NodeList(list) => list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).set_relate(rhs, require_all)),
            _ => false,
        }
    }

    /// `arr1 subsetof arr2`: every element of `self` is a member of `rhs`.
    fn subset_of(&self, rhs: &Self) -> bool {
        self.set_relate(rhs, true)
    }

    /// `arr1 anyof arr2`: `self` and `rhs` have a non-empty intersection.
    fn any_of(&self, rhs: &Self) -> bool {
        self.set_relate(rhs, false)
    }

    /// Length of `self` as a sized sequence — array element count or string char count.
    /// `None` for anything else (numbers, bools, null, objects). Used by the `sizeof`/
    /// `empty` operators; a multi-result (nodelist) left operand is handled any-of by the
    /// callers, so it never reaches here.
    fn seq_length(&self) -> Option<usize> {
        fn arr_or_str_len<V: SelectValue>(v: &V) -> Option<usize> {
            match v.get_type() {
                SelectValueType::String => v.as_str().map(|s| s.chars().count()),
                SelectValueType::Array => v.len(),
                _ => None,
            }
        }
        match self {
            TermEvaluationResult::Str(s) => Some(s.chars().count()),
            TermEvaluationResult::String(s) => Some(s.chars().count()),
            TermEvaluationResult::Value(v) => arr_or_str_len(v.as_ref()),
            TermEvaluationResult::Literal(l) => arr_or_str_len(l),
            _ => None,
        }
    }

    /// Boolean view of this term, `None` if not a boolean.
    fn as_bool(&self) -> Option<bool> {
        match self {
            TermEvaluationResult::Bool(b) => Some(*b),
            TermEvaluationResult::Literal(Value::Bool(b)) => Some(*b),
            TermEvaluationResult::Value(v) if v.as_ref().get_type() == SelectValueType::Bool => {
                v.as_ref().get_bool()
            }
            _ => None,
        }
    }

    /// `left sizeof right`: true if `self` is an array/string whose length equals the
    /// integer value of `right` (a fractional `right` is truncated toward zero). A
    /// non-numeric `right` or non-array/string `self` yields false.
    fn size_of(&self, rhs: &Self) -> bool {
        // Any-of over a multi-result left operand, matching `==`/`<`/`in`.
        if let TermEvaluationResult::NodeList(list) = self {
            return list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).size_of(rhs));
        }
        let target: i64 = match rhs.as_number() {
            Some(Num::Int(n)) => n,
            Some(Num::Float(f)) => match f64_to_i64(f.trunc()) {
                Some(n) => n,
                None => return false,
            },
            None => return false,
        };
        target >= 0 && self.seq_length().is_some_and(|len| len as i64 == target)
    }

    /// `left empty right`: `right` is a boolean — `true` matches an empty array/string,
    /// `false` a non-empty one. A non-boolean `right` or non-array/string `self` yields
    /// false.
    fn empty_check(&self, rhs: &Self) -> bool {
        // Any-of over a multi-result left operand, matching `==`/`<`/`in`.
        if let TermEvaluationResult::NodeList(list) = self {
            return list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).empty_check(rhs));
        }
        let (Some(len), Some(want_empty)) = (self.seq_length(), rhs.as_bool()) else {
            return false;
        };
        (len == 0) == want_empty
    }

    fn re_is_match(cache: &mut RegexCache, regex: &str, s: &str) -> bool {
        // Substring match, shared (and cached) with `search()`.
        regex_matches(cache, regex, false, s)
    }

    fn re_match(&self, s: &Self, cache: &mut RegexCache) -> bool {
        match (self, s) {
            (TermEvaluationResult::Value(v), TermEvaluationResult::Str(regex)) => {
                match v.get_type() {
                    SelectValueType::String => v
                        .as_str()
                        .is_some_and(|s| Self::re_is_match(cache, regex, s)),
                    _ => false,
                }
            }
            (TermEvaluationResult::Value(v1), TermEvaluationResult::Value(v2)) => {
                match (v1.get_type(), v2.get_type()) {
                    (SelectValueType::String, SelectValueType::String) => v1
                        .as_str()
                        .zip(v2.as_str())
                        .is_some_and(|(s1, s2)| Self::re_is_match(cache, s2, s1)),
                    (_, _) => false,
                }
            }
            (_, _) => false,
        }
    }

    fn re(&self, s: &Self, cache: &mut RegexCache) -> bool {
        match (self, s) {
            (TermEvaluationResult::NodeList(list), _) => list
                .iter()
                .any(|v| TermEvaluationResult::Value(v.clone()).re(s, cache)),
            (_, TermEvaluationResult::NodeList(list)) => list
                .iter()
                .any(|v| self.re(&TermEvaluationResult::Value(v.clone()), cache)),
            _ => self.re_match(s, cache),
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
    pub res: ValueRef<'i, S>,
    pub path_tracker: Option<UPT>,
}

#[derive(Debug)]
struct PathCalculatorData<'i, S: SelectValue, UPT: UserPathTracker> {
    results: Vec<CalculationResult<'i, S, UPT>>,
    root: ValueRef<'i, S>,
    /// Per-query compiled-regex cache (see `RegexCache`), threaded as `&mut` through
    /// filter evaluation. Dropped with this struct when the query ends.
    regex_cache: RegexCache,
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

    fn results_to_term<'j, S: SelectValue>(
        mut results: Vec<CalculationResult<'j, S, UPTG::PT>>,
    ) -> TermEvaluationResult<'static, 'j, S> {
        match results.len() {
            0 => TermEvaluationResult::Invalid,
            1 => results
                .pop()
                .map(|r| TermEvaluationResult::Value(r.res))
                .unwrap_or(TermEvaluationResult::Invalid),
            _ => TermEvaluationResult::NodeList(results.into_iter().map(|r| r.res).collect()),
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
            Rule::array_literal | Rule::object_literal => {
                TermEvaluationResult::Literal(build_literal(term))
            }
            Rule::function_call => {
                let mut inner = term.into_inner();
                let name = inner.next().map_or("", |p| p.as_str());
                let mut args = Vec::new();
                for arg in inner {
                    args.push(self.evaluate_single_term(arg, json.clone(), calc_data));
                }
                eval_function(name, args, &mut calc_data.regex_cache)
            }
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
                        regex_cache: HashMap::new(),
                    };
                    self.calc_internal(term.into_inner(), json, None, &mut calc_data);
                    Self::results_to_term(calc_data.results)
                }
                None => TermEvaluationResult::Value(json),
            },
            Rule::from_root => match term.into_inner().next() {
                Some(term) => {
                    let mut new_calc_data = PathCalculatorData {
                        results: Vec::new(),
                        root: calc_data.root.clone(),
                        regex_cache: HashMap::new(),
                    };
                    self.calc_internal(
                        term.into_inner(),
                        calc_data.root.clone(),
                        None,
                        &mut new_calc_data,
                    );
                    Self::results_to_term(new_calc_data.results)
                }
                None => TermEvaluationResult::Value(calc_data.root.clone()),
            },
            _ => {
                trace!("evaluate_single_term: unhandled rule {:?}", term.as_rule());
                TermEvaluationResult::Invalid
            }
        }
    }

    /// Evaluate an arithmetic expression: `arith_term ((+|-) arith_term)*` (left-assoc).
    fn evaluate_arith_expr<'j: 'i, S: SelectValue>(
        &self,
        expr: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        let mut inner = expr.into_inner();
        let Some(first) = inner.next() else {
            return TermEvaluationResult::Invalid;
        };
        let mut acc = self.evaluate_arith_term(first, json.clone(), calc_data);
        while let Some(op) = inner.next() {
            let Some(rhs) = inner.next() else {
                return TermEvaluationResult::Invalid;
            };
            let rhs = self.evaluate_arith_term(rhs, json.clone(), calc_data);
            acc = arith_binop(op.as_rule(), &acc, &rhs);
        }
        acc
    }

    /// Evaluate an arithmetic term: `arith_factor ((*|/|%) arith_factor)*` (left-assoc).
    fn evaluate_arith_term<'j: 'i, S: SelectValue>(
        &self,
        term: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        let mut inner = term.into_inner();
        let Some(first) = inner.next() else {
            return TermEvaluationResult::Invalid;
        };
        let mut acc = self.evaluate_arith_factor(first, json.clone(), calc_data);
        while let Some(op) = inner.next() {
            let Some(rhs) = inner.next() else {
                return TermEvaluationResult::Invalid;
            };
            let rhs = self.evaluate_arith_factor(rhs, json.clone(), calc_data);
            acc = arith_binop(op.as_rule(), &acc, &rhs);
        }
        acc
    }

    /// Evaluate an arithmetic factor: an optional unary `+`/`-` applied to a primary
    /// (a parenthesized expression or a term).
    fn evaluate_arith_factor<'j: 'i, S: SelectValue>(
        &self,
        factor: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        let mut inner = factor.into_inner();
        let Some(first) = inner.next() else {
            return TermEvaluationResult::Invalid;
        };
        match first.as_rule() {
            Rule::neg | Rule::pos => {
                let operator = first.as_rule();
                let Some(operand) = inner.next() else {
                    return TermEvaluationResult::Invalid;
                };
                let v = self.evaluate_arith_operand(operand, json, calc_data);
                arith_unary(operator, v)
            }
            _ => self.evaluate_arith_operand(first, json, calc_data),
        }
    }

    /// Evaluate an arithmetic primary: a parenthesized sub-expression or a plain term.
    fn evaluate_arith_operand<'j: 'i, S: SelectValue>(
        &self,
        operand: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> TermEvaluationResult<'i, 'j, S> {
        match operand.as_rule() {
            Rule::arith_expr => self.evaluate_arith_expr(operand, json, calc_data),
            _ => self.evaluate_single_term(operand, json, calc_data),
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
        let term1_val = self.evaluate_arith_expr(term1, json.clone(), calc_data);
        trace!("evaluate_single_filter term1_val {:?}", &term1_val);
        if let Some(op) = curr.next() {
            trace!("evaluate_single_filter op {:?}", &op);
            let Some(term2) = curr.next() else {
                trace!("evaluate_single_filter: missing second term");
                return false;
            };
            trace!("evaluate_single_filter term2 {:?}", &term2);
            let term2_val = self.evaluate_arith_expr(term2, json, calc_data);
            trace!("evaluate_single_filter term2_val {:?}", &term2_val);
            match op.as_rule() {
                Rule::gt => term1_val.gt(&term2_val),
                Rule::ge => term1_val.ge(&term2_val),
                Rule::lt => term1_val.lt(&term2_val),
                Rule::le => term1_val.le(&term2_val),
                Rule::eq => term1_val.eq(&term2_val),
                Rule::ne => term1_val.ne(&term2_val),
                Rule::re => term1_val.re(&term2_val, &mut calc_data.regex_cache),
                Rule::in_op => term1_val.member_of(&term2_val),
                // `nin` is the strict negation of `in`: a non-array / absent RHS makes
                // `in` false, so `nin` is true.
                Rule::nin_op => !term1_val.member_of(&term2_val),
                Rule::subsetof_op => term1_val.subset_of(&term2_val),
                Rule::anyof_op => term1_val.any_of(&term2_val),
                // `noneof` = empty intersection = strict negation of `anyof`.
                Rule::noneof_op => !term1_val.any_of(&term2_val),
                Rule::size_op => term1_val.size_of(&term2_val),
                Rule::empty_op => term1_val.empty_check(&term2_val),
                _ => {
                    trace!(
                        "evaluate_single_filter: unknown comparison op {:?}",
                        op.as_rule()
                    );
                    false
                }
            }
        } else {
            // A bare term is a test: a boolean result (e.g. `match(...)`) uses its
            // value; any other present value is truthy (existence), `Invalid` is false.
            match term1_val {
                TermEvaluationResult::Bool(b) => b,
                other => !matches!(other, TermEvaluationResult::Invalid),
            }
        }
    }

    /// Evaluate a single filter operand: a comparison/existence test (`single_filter`),
    /// a parenthesized sub-filter (`filter`), or a negated operand (`negation`).
    fn evaluate_filter_operand<'j: 'i, S: SelectValue>(
        &self,
        operand: Pair<'i, Rule>,
        json: ValueRef<'j, S>,
        calc_data: &mut PathCalculatorData<'j, S, UPTG::PT>,
    ) -> bool {
        match operand.as_rule() {
            Rule::single_filter => self.evaluate_single_filter(operand, json, calc_data),
            Rule::filter => self.evaluate_filter(operand.into_inner(), json, calc_data),
            Rule::negation => match operand.into_inner().next() {
                Some(inner) => !self.evaluate_filter_operand(inner, json, calc_data),
                None => {
                    trace!("evaluate_filter_operand: negation without operand");
                    false
                }
            },
            other => {
                trace!("evaluate_filter_operand: unexpected rule {other:?}");
                false
            }
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
        let mut first_result = self.evaluate_filter_operand(first_filter, json.clone(), calc_data);
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
                    first_result =
                        self.evaluate_filter_operand(second_filter, json.clone(), calc_data);
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
                        }
                        // Per RFC 9535 s2.3.5.2: "The filter selector works
                        // with arrays and objects exclusively. [...] Applied
                        // to a primitive value, it selects nothing."
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
            regex_cache: HashMap::new(),
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
            // SAFETY: Calculator must be built with a path tracker (e.g. `create_with_generator`);
            // each result should therefore carry `path_tracker` like `calc_once_paths`.
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
