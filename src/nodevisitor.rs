use bitflags::bitflags;
use jsonpath_lib::parser::{Node, NodeVisitor, ParseToken};
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
#[derive(Debug, PartialEq)]
pub enum VisitStatus {
    NotValid,
    PartialValid,
    Valid,
}

bitflags! {
    #[allow(clippy::unnecessary_cast)]
    pub struct PathInfoFlags: c_int {
        const INVALID = 1 as c_int;
        const STATIC = 2 as c_int;
        const DEFINED_ORDER = 4 as c_int;
    }
}

pub struct StaticPathParser<'a> {
    pub valid: VisitStatus,
    last_token: Option<ParseToken<'a>>,
    pub static_path_elements: Vec<StaticPathElement>,
}

pub struct JSONPathHandle<'a> {
    path: String,
    pub parser: Option<StaticPathParser<'a>>,
    node: Option<Node<'a>>,
}
impl<'a> JSONPathHandle<'a> {
    pub fn new(path: &'a str) -> Self {
        Self {
            path: String::from(path),
            parser: None,
            node: None,
        }
    }

    pub fn parse(&'a mut self) -> Result<&'a Self, String> {
        let node = Parser::compile(self.path.as_str())?;
        let mut parser = StaticPathParser {
            valid: VisitStatus::PartialValid,
            last_token: None,
            static_path_elements: vec![],
        };
        parser.visit(&node);
        self.node = Some(node);
        self.parser = Some(parser);
        Ok(self)
    }

    pub fn get_path_info_flags(&self) -> PathInfoFlags {
        self.parser
            .as_ref()
            .map_or_else(|| PathInfoFlags::INVALID, |p| p.get_path_info_flags())
    }
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

    pub fn get_path_info_flags(&self) -> PathInfoFlags {
        if self.valid == VisitStatus::Valid {
            PathInfoFlags::STATIC | PathInfoFlags::DEFINED_ORDER
        } else {
            PathInfoFlags::INVALID
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
