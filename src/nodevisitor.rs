use jsonpath_lib::parser::{NodeVisitor, ParseToken};
use jsonpath_lib::Parser;

#[derive(Debug, PartialEq)]
enum VisitStatus {
    NotValid,
    PartialValid,
    Valid,
}

pub struct NodeVisitorImpl {
    valid: VisitStatus,
    last_token: Option<ParseToken>,
}

impl NodeVisitorImpl {
    ///
    /// Checks if path is static & valid
    /// 
    pub fn check(input: &str) -> Result<bool, String> {
        let node = Parser::compile(input)?;
        let mut visitor = NodeVisitorImpl {
            valid: VisitStatus::PartialValid,
            last_token: None,
        };
        visitor.visit(&node);
        Ok(visitor.valid == VisitStatus::Valid)
    }
}

impl NodeVisitor for NodeVisitorImpl {
    fn visit_token(&mut self, token: &ParseToken) {
        if self.valid != VisitStatus::NotValid {
            self.valid = match (&self.last_token, token) {
                (None, ParseToken::Absolute) => VisitStatus::PartialValid,
                (Some(ParseToken::Absolute), ParseToken::In) => VisitStatus::PartialValid,
                (Some(ParseToken::In), ParseToken::Key(_)) => VisitStatus::Valid,
                (Some(ParseToken::Key(_)), ParseToken::In) => VisitStatus::PartialValid,
                (Some(ParseToken::Key(_)), ParseToken::Array) => VisitStatus::PartialValid,
                (Some(ParseToken::Array), ParseToken::Number(_)) => VisitStatus::PartialValid,
                (Some(ParseToken::Number(_)), ParseToken::ArrayEof) => VisitStatus::PartialValid,
                (Some(ParseToken::ArrayEof), ParseToken::In) => VisitStatus::PartialValid,
                _ => VisitStatus::NotValid,
            };
            self.last_token = Some(token.clone());
        }
    }
}
