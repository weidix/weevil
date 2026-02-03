//! Unified selector interface for CSS and XPath.

use std::fmt;

use crate::html::{HtmlTree, NodeId, NodeKind};
use crate::selector::Selector;
use crate::xpath::XPath;

mod selector_exec;
mod xpath_exec;
mod xpath_predicate;

use selector_exec::find_selector_in;
use xpath_exec::find_xpath_in;

/// Query execution context, either the whole tree or a subtree.
pub trait QueryContext {
    /// Returns the tree for this context.
    fn tree(&self) -> &HtmlTree;

    /// Returns the starting node id for this context.
    fn start(&self) -> NodeId;

    /// Returns the selector scope element for this context, if any.
    fn scope_element(&self) -> Option<NodeId>;
}

impl QueryContext for &HtmlTree {
    fn tree(&self) -> &HtmlTree {
        self
    }

    fn start(&self) -> NodeId {
        self.document()
    }

    fn scope_element(&self) -> Option<NodeId> {
        self.root_element()
    }
}

impl QueryContext for (&HtmlTree, NodeId) {
    fn tree(&self) -> &HtmlTree {
        self.0
    }

    fn start(&self) -> NodeId {
        self.1
    }

    fn scope_element(&self) -> Option<NodeId> {
        match self.0.node(self.1).kind() {
            NodeKind::Element(_) => Some(self.1),
            NodeKind::Document => self.0.root_element(),
            _ => None,
        }
    }
}

/// Unified query execution interface for CSS selectors and XPath expressions.
pub trait Query {
    /// Returns all nodes matching the query.
    fn find<C>(&self, context: C) -> Result<Vec<NodeId>, QueryExecError>
    where
        C: QueryContext;

    /// Returns the first node matching the query, if any.
    fn find_first<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Ok(self.find(context)?.into_iter().next())
    }

    /// Alias for `find_first`.
    fn select_one<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
    }

    /// Alias for `find_first`.
    fn first_match<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
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

impl Query for Selector {
    fn find<C>(&self, context: C) -> Result<Vec<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Ok(find_selector_in(
            self,
            context.tree(),
            context.start(),
            context.scope_element(),
        ))
    }
}

impl Query for XPath {
    fn find<C>(&self, context: C) -> Result<Vec<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        find_xpath_in(self, context.tree(), context.start())
    }
}

impl Selector {
    /// Returns all matching nodes.
    pub fn find<C>(&self, context: C) -> Result<Vec<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Query::find(self, context)
    }

    /// Returns the first matching node, if any.
    pub fn find_first<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Query::find_first(self, context)
    }

    /// Alias for `find_first`.
    pub fn select_one<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
    }

    /// Alias for `find_first`.
    pub fn first_match<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
    }
}

impl XPath {
    /// Returns all matching nodes.
    pub fn find<C>(&self, context: C) -> Result<Vec<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Query::find(self, context)
    }

    /// Returns the first matching node, if any.
    pub fn find_first<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        Query::find_first(self, context)
    }

    /// Alias for `find_first`.
    pub fn select_one<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
    }

    /// Alias for `find_first`.
    pub fn first_match<C>(&self, context: C) -> Result<Option<NodeId>, QueryExecError>
    where
        C: QueryContext,
    {
        self.find_first(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_selector_and_xpath() {
        let html = r#"<div id="hero" class="a b"><span class="b"></span><span></span></div>"#;
        let tree = HtmlTree::parse(html);
        let hero = tree.index().by_id("hero").expect("missing hero id");
        let spans = tree.index().by_tag("span").expect("missing span nodes");
        let first_span = spans[0];

        let selector = Selector::parse("div#hero > span.b").unwrap();
        let matches = selector.find(&tree).unwrap();
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

    #[test]
    fn query_context_limits_scope() {
        let html = r#"
        <div id="outer">
          <span id="inner"></span>
        </div>
        <span id="outside"></span>
        "#;
        let tree = HtmlTree::parse(html);
        let outer = tree.index().by_id("outer").expect("missing outer");
        let inner = tree.index().by_id("inner").expect("missing inner");
        let outside = tree.index().by_id("outside").expect("missing outside");

        let selector = Selector::parse("span").unwrap();
        let matches = selector.find(&tree).unwrap();
        assert_eq!(matches, vec![inner, outside]);

        let matches = selector.find((&tree, outer)).unwrap();
        assert_eq!(matches, vec![inner]);

        let xpath = XPath::parse("descendant::span").unwrap();
        let matches = xpath.find((&tree, outer)).unwrap();
        assert_eq!(matches, vec![inner]);
    }
}
