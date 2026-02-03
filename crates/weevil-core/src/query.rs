//! Unified selector interface for CSS and XPath.

use std::fmt;

use crate::html::{HtmlTree, NodeId};
use crate::selector::{Selector, SelectorError};
use crate::xpath::{XPath, XPathError};

mod selector_exec;
mod xpath_exec;

use selector_exec::find_selector;
use xpath_exec::find_xpath;

/// Supported query languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Selector,
    XPath,
}

/// Unified query AST for CSS selectors and XPath expressions.
#[derive(Debug, Clone)]
pub enum Query {
    Selector(Selector),
    XPath(XPath),
}

impl Query {
    /// Parses a query using the requested language.
    pub fn parse(input: &str, kind: QueryKind) -> Result<Self, QueryError> {
        match kind {
            QueryKind::Selector => Selector::parse(input)
                .map(Query::Selector)
                .map_err(QueryError::Selector),
            QueryKind::XPath => XPath::parse(input)
                .map(Query::XPath)
                .map_err(QueryError::XPath),
        }
    }

    /// Executes the query against a parsed HTML tree.
    pub fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        match self {
            Query::Selector(selector) => selector.find(tree),
            Query::XPath(xpath) => xpath.find(tree),
        }
    }

    /// Returns the first matching node, if any.
    pub fn find_first(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        QueryExecutor::find_first(self, tree)
    }

    /// Alias for `find_first`.
    pub fn select_one(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }

    /// Alias for `find_first`.
    pub fn first_match(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }
}

/// Unified parse error for CSS selectors and XPath expressions.
#[derive(Debug, Clone)]
pub enum QueryError {
    Selector(SelectorError),
    XPath(XPathError),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Selector(err) => write!(f, "{err}"),
            QueryError::XPath(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for QueryError {}

/// Unified query execution interface for CSS selectors and XPath expressions.
pub trait QueryExecutor {
    /// Returns all nodes matching the query.
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError>;

    /// Returns the first node matching the query, if any.
    fn find_first(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        Ok(self.find(tree)?.into_iter().next())
    }
}

/// Error returned when executing a query against a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryExecFeature {
    XPathExpression,
    XPathAxis,
    XPathNodeTest,
    XPathKindTest,
    XPathPredicate,
    XPathPrimaryExpr,
    XPathFunction,
    XPathPostfix,
}

impl fmt::Display for QueryExecFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            QueryExecFeature::XPathExpression => "XPath expression",
            QueryExecFeature::XPathAxis => "XPath axis",
            QueryExecFeature::XPathNodeTest => "XPath node test",
            QueryExecFeature::XPathKindTest => "XPath kind test",
            QueryExecFeature::XPathPredicate => "XPath predicate",
            QueryExecFeature::XPathPrimaryExpr => "XPath primary expression",
            QueryExecFeature::XPathFunction => "XPath function",
            QueryExecFeature::XPathPostfix => "XPath postfix expression",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Clone)]
pub struct QueryExecError {
    feature: QueryExecFeature,
    detail: String,
    hint: Option<&'static str>,
}

impl QueryExecError {
    pub(crate) fn unsupported(
        feature: QueryExecFeature,
        detail: impl Into<String>,
        hint: Option<&'static str>,
    ) -> Self {
        Self {
            feature,
            detail: detail.into(),
            hint,
        }
    }

    /// Returns the unsupported feature category.
    pub fn feature(&self) -> QueryExecFeature {
        self.feature
    }

    /// Returns the detailed error message.
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Returns a hint describing the supported subset, if available.
    pub fn hint(&self) -> Option<&'static str> {
        self.hint
    }
}

impl fmt::Display for QueryExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Unsupported {feature}: {detail}",
            feature = self.feature,
            detail = self.detail
        )?;
        if let Some(hint) = self.hint {
            write!(f, "\nHint: {hint}")?;
        }
        Ok(())
    }
}

impl std::error::Error for QueryExecError {}

impl QueryExecutor for Selector {
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        Ok(find_selector(self, tree))
    }
}

impl QueryExecutor for XPath {
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        find_xpath(self, tree)
    }
}

impl QueryExecutor for Query {
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        Query::find(self, tree)
    }
}

impl Selector {
    /// Returns the first matching node, if any.
    pub fn find_first(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        QueryExecutor::find_first(self, tree)
    }

    /// Alias for `find_first`.
    pub fn select_one(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }

    /// Alias for `find_first`.
    pub fn first_match(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }
}

impl XPath {
    /// Returns the first matching node, if any.
    pub fn find_first(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        QueryExecutor::find_first(self, tree)
    }

    /// Alias for `find_first`.
    pub fn select_one(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }

    /// Alias for `find_first`.
    pub fn first_match(&self, tree: &HtmlTree) -> Result<Option<NodeId>, QueryExecError> {
        self.find_first(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssparser::ToCss;

    #[test]
    fn parse_query_selector() {
        let query = Query::parse("h1.title", QueryKind::Selector).unwrap();
        match query {
            Query::Selector(selector) => {
                assert_eq!(selector.to_css_string(), "h1.title");
            }
            Query::XPath(_) => panic!("expected selector query"),
        }
    }

    #[test]
    fn parse_query_xpath() {
        let query = Query::parse("/foo", QueryKind::XPath).unwrap();
        match query {
            Query::XPath(_) => {}
            Query::Selector(_) => panic!("expected xpath query"),
        }
    }

    #[test]
    fn parse_query_xpath_error() {
        let err = Query::parse("foo:bar", QueryKind::XPath).unwrap_err();
        match err {
            QueryError::XPath(inner) => {
                assert!(inner.to_string().contains("foo"));
            }
            QueryError::Selector(_) => panic!("expected xpath error"),
        }
    }

    #[test]
    fn query_find_selector_and_xpath() {
        let html = r#"<div id="hero" class="a b"><span class="b"></span><span></span></div>"#;
        let tree = HtmlTree::parse(html);
        let hero = tree.index().by_id("hero").expect("missing hero id");
        let spans = tree.index().by_tag("span").expect("missing span nodes");
        let first_span = spans[0];

        let selector = Selector::parse("div#hero > span.b").unwrap();
        let matches = selector.find(&tree).unwrap();
        assert_eq!(matches, vec![first_span]);

        let query = Query::Selector(selector);
        let matches = query.find(&tree).unwrap();
        assert_eq!(matches, vec![first_span]);

        let xpath = XPath::parse("/html/body/div/span[1]").unwrap();
        let matches = xpath.find(&tree).unwrap();
        assert_eq!(matches, vec![first_span]);

        let xpath = XPath::parse("//span").unwrap();
        let matches = xpath.find(&tree).unwrap();
        assert_eq!(matches, spans.to_vec());

        let xpath = XPath::parse("//div").unwrap();
        let matches = xpath.find(&tree).unwrap();
        assert_eq!(matches, vec![hero]);
    }

    #[test]
    fn query_find_first_and_aliases() {
        let html = r#"<div><span id="a"></span><span id="b"></span></div>"#;
        let tree = HtmlTree::parse(html);
        let first = tree.index().by_id("a").expect("missing first span");

        let selector = Selector::parse("span").unwrap();
        assert_eq!(selector.find_first(&tree).unwrap(), Some(first));
        assert_eq!(selector.select_one(&tree).unwrap(), Some(first));
        assert_eq!(selector.first_match(&tree).unwrap(), Some(first));
    }
}
