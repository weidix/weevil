//! XPath parsing for weevil core.

use std::fmt;

use xee_xpath_ast::{Namespaces, ParserError, VariableNames, ast};

/// Wrapper around a parsed XPath expression.
#[derive(Debug, Clone)]
pub struct XPath(pub(crate) ast::XPath);

impl XPath {
    /// Parses an XPath expression using default namespaces and variable names.
    pub fn parse(input: &str) -> Result<Self, XPathError> {
        let namespaces = Namespaces::default();
        let variable_names = VariableNames::default();
        ast::XPath::parse(input, &namespaces, &variable_names)
            .map(Self)
            .map_err(|err| XPathError::new(err, input))
    }
}

/// Error returned when parsing XPath expressions.
#[derive(Debug, Clone)]
pub struct XPathError(Box<XPathErrorInner>);

#[derive(Debug, Clone)]
struct XPathErrorInner {
    kind: ParserError,
    span: ast::Span,
    snippet: String,
}

impl XPathError {
    fn new(kind: ParserError, input: &str) -> Self {
        let span = kind.span();
        let snippet = snippet_for_span(input, span);
        Self(Box::new(XPathErrorInner {
            kind,
            span,
            snippet,
        }))
    }

    /// Returns the underlying parser error kind.
    pub fn kind(&self) -> &ParserError {
        &self.0.kind
    }

    /// Returns the span where the error occurred.
    pub fn span(&self) -> ast::Span {
        self.0.span
    }

    /// Returns a snippet of the input at the error span.
    pub fn snippet(&self) -> &str {
        &self.0.snippet
    }
}

impl fmt::Display for XPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = self.0.span.start;
        let end = self.0.span.end;
        let snippet = &self.0.snippet;

        match &self.0.kind {
            ParserError::ExpectedFound { .. } => {
                write!(f, "Unexpected token at {start}..{end}: {snippet:?}")
            }
            ParserError::UnknownPrefix { prefix, .. } => write!(
                f,
                "Unknown prefix {prefix:?} at {start}..{end}: {snippet:?}"
            ),
            ParserError::Reserved { name, .. } => {
                write!(f, "Reserved name {name:?} at {start}..{end}: {snippet:?}")
            }
            ParserError::ArityOverflow { .. } => {
                write!(f, "Function arity overflow at {start}..{end}: {snippet:?}")
            }
            ParserError::UnknownType { name, .. } => {
                write!(f, "Unknown type {name:?} at {start}..{end}: {snippet:?}")
            }
            ParserError::IllegalFunctionInPattern { name, .. } => write!(
                f,
                "Illegal function in pattern {name:?} at {start}..{end}: {snippet:?}"
            ),
        }
    }
}

impl std::error::Error for XPathError {}

fn snippet_for_span(input: &str, span: ast::Span) -> String {
    let len = input.len();
    let start = span.start.min(len);
    let end = span.end.min(len);
    input.get(start..end).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xpath_simple() {
        assert!(XPath::parse("child::foo").is_ok());
    }

    #[test]
    fn xpath_unknown_prefix_error_message() {
        let err = XPath::parse("foo:bar").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("foo"));
    }
}
