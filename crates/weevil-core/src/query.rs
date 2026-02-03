//! Unified selector interface for CSS and XPath.

use std::fmt;

use crate::selector::{Selector, SelectorErrorKind};
use crate::xpath::{XPath, XPathError};

/// Supported query languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Css,
    XPath,
}

/// Unified query AST for CSS selectors and XPath expressions.
#[derive(Debug, Clone)]
pub enum Query {
    Css(Selector),
    XPath(XPath),
}

impl Query {
    /// Parses a query using the requested language.
    pub fn parse<'a>(input: &'a str, kind: QueryKind) -> Result<Self, QueryError<'a>> {
        match kind {
            QueryKind::Css => Selector::parse(input)
                .map(Query::Css)
                .map_err(QueryError::Css),
            QueryKind::XPath => XPath::parse(input)
                .map(Query::XPath)
                .map_err(QueryError::XPath),
        }
    }
}

/// Unified parse error for CSS selectors and XPath expressions.
#[derive(Debug, Clone)]
pub enum QueryError<'a> {
    Css(SelectorErrorKind<'a>),
    XPath(XPathError),
}

impl fmt::Display for QueryError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Css(err) => write!(f, "{err}"),
            QueryError::XPath(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for QueryError<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use cssparser::ToCss;

    #[test]
    fn parse_query_css() {
        let query = Query::parse("h1.title", QueryKind::Css).unwrap();
        match query {
            Query::Css(selector) => {
                assert_eq!(selector.to_css_string(), "h1.title");
            }
            Query::XPath(_) => panic!("expected css query"),
        }
    }

    #[test]
    fn parse_query_xpath() {
        let query = Query::parse("/foo", QueryKind::XPath).unwrap();
        match query {
            Query::XPath(_) => {}
            Query::Css(_) => panic!("expected xpath query"),
        }
    }

    #[test]
    fn parse_query_xpath_error() {
        let err = Query::parse("foo:bar", QueryKind::XPath).unwrap_err();
        match err {
            QueryError::XPath(inner) => {
                assert!(inner.to_string().contains("foo"));
            }
            QueryError::Css(_) => panic!("expected xpath error"),
        }
    }
}
