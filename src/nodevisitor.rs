use bitflags::bitflags;
use jsonpath_lib::parser::{NodeVisitor, ParseToken};
use jsonpath_lib::Parser;

use std::fmt::Display;
use std::fmt::Formatter;
use std::os::raw::c_int;

pub enum StaticPathElement {
    ArrayIndex(f64),
    ObjectKey(String),
    Root,
}

impl Display for StaticPathElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use StaticPathElement::*;
        match self {
            ArrayIndex(num) => write!(f, "[{}]", num),
            ObjectKey(key) => write!(f, "[\"{}\"]", key),
            Root => write!(f, "$"),
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq)]
pub enum VisitStatus {
    NotValid,
    PartialValid,
    Valid,
}

bitflags! {
    pub struct PathInfoFlags: c_int {
        // cast to `c_int` is not unnecessary since `c_int` might not always be `i32`
        //  (some esoteric systems define it as an `i16`, for example)
        //  (see https://doc.rust-lang.org/std/os/raw/type.c_int.html)
        #[allow(clippy::unnecessary_cast)]
        const NONE = 0 as c_int;
        #[allow(clippy::unnecessary_cast)]
        const SINGLE = 1 as c_int;
        #[allow(clippy::unnecessary_cast)]
        const DEFINED_ORDER = 2 as c_int;
    }
}

pub struct StaticPathParser<'a> {
    pub valid: VisitStatus,
    last_token: Option<ParseToken<'a>>,
    pub static_path_elements: Vec<StaticPathElement>,
}

impl<'a> StaticPathParser<'a> {
    ///
    /// Checks if path is static & valid
    ///
    pub fn check(input: &'a str) -> Result<Self, String> {
        let node = Parser::compile(input)?;
        let mut visitor = StaticPathParser {
            valid: VisitStatus::PartialValid,
            last_token: None,
            static_path_elements: vec![],
        };
        visitor.visit(&node);
        Ok(visitor)
    }

    pub fn get_path_info(path: &'a str) -> Result<PathInfoFlags, String> {
        let parser = Self::check(path)?;
        match parser.valid {
            // Currently we do not detect some SINGLE, such as, $.a[1:2]
            // Currently we do not detect some DEFINED_ORDER which are not SINGLE, such as, $.a[1:3] or $.a.b[3,1,2,0].c
            VisitStatus::Valid => Ok(PathInfoFlags::SINGLE | PathInfoFlags::DEFINED_ORDER),
            _ => Ok(PathInfoFlags::NONE),
        }
    }
}

impl<'a> NodeVisitor<'a> for StaticPathParser<'a> {
    fn visit_token(&mut self, token: &ParseToken<'a>) {
        if self.valid != VisitStatus::NotValid {
            //eprintln!("visit token: {:?} -> {:?}", self.last_token, token);
            self.valid = match (&self.last_token, token) {
                (None, ParseToken::Absolute) => {
                    self.static_path_elements.push(StaticPathElement::Root);
                    VisitStatus::Valid
                }

                (Some(ParseToken::Absolute), ParseToken::In)
                | (Some(ParseToken::Absolute), ParseToken::Array)
                | (Some(ParseToken::Array), ParseToken::Key(_))
                | (Some(ParseToken::Array), ParseToken::KeyString(_))
                | (Some(ParseToken::Key(_)), ParseToken::In)
                | (Some(ParseToken::KeyString(_)), ParseToken::In)
                | (Some(ParseToken::Key(_)), ParseToken::Array)
                | (Some(ParseToken::KeyString(_)), ParseToken::Array)
                | (Some(ParseToken::ArrayEof), ParseToken::Array)
                | (Some(ParseToken::Array), ParseToken::Number(_))
                | (Some(ParseToken::ArrayEof), ParseToken::In) => VisitStatus::PartialValid,

                (Some(ParseToken::Number(num)), ParseToken::ArrayEof) => {
                    self.static_path_elements
                        .push(StaticPathElement::ArrayIndex(*num));
                    VisitStatus::Valid
                }

                (Some(ParseToken::In), ParseToken::Key(key))
                | (Some(ParseToken::Key(key)), ParseToken::ArrayEof) => {
                    self.static_path_elements
                        .push(StaticPathElement::ObjectKey((*key).to_string()));
                    VisitStatus::Valid
                }

                (Some(ParseToken::In), ParseToken::KeyString(key))
                | (Some(ParseToken::KeyString(key)), ParseToken::ArrayEof) => {
                    self.static_path_elements
                        .push(StaticPathElement::ObjectKey(key.to_string()));
                    VisitStatus::Valid
                }

                _ => VisitStatus::NotValid,
            };
            self.last_token = Some(token.clone());
        }
    }
}
