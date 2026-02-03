//! Unified selector interface for CSS and XPath.

use std::fmt;

use crate::html::{HtmlTree, NodeId, NodeKind};
use crate::selector::{
    CssLocalName, CssString, NonTSPseudoClass, PseudoElement, Selector, SelectorErrorKind, Simple,
};
use crate::xpath::{XPath, XPathError};
use html5ever::interface::tree_builder::QuirksMode as HtmlQuirksMode;
use html5ever::{Namespace, local_name, ns};
use rustc_hash::FxHashSet;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::{Element, OpaqueElement, matching};
use xee_xpath_ast::FN_NAMESPACE;
use xee_xpath_ast::ast;
use xot::xmlname::NameStrInfo;

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

    /// Executes the query against a parsed HTML tree.
    pub fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        match self {
            Query::Css(selector) => selector.find(tree),
            Query::XPath(xpath) => xpath.find(tree),
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

/// Unified query execution interface for CSS selectors and XPath expressions.
pub trait QueryExecutor {
    /// Returns all nodes matching the query.
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError>;
}

/// Error returned when executing a query against a tree.
#[derive(Debug, Clone)]
pub enum QueryExecError {
    Unsupported(&'static str),
}

impl fmt::Display for QueryExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryExecError::Unsupported(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for QueryExecError {}

#[derive(Clone, Copy, Debug)]
struct HtmlElement<'a> {
    tree: &'a HtmlTree,
    id: NodeId,
}

impl<'a> HtmlElement<'a> {
    fn new(tree: &'a HtmlTree, id: NodeId) -> Self {
        Self { tree, id }
    }

    fn node(&self) -> &crate::html::Node {
        self.tree.node(self.id)
    }

    fn element_data(&self) -> &crate::html::ElementData {
        match self.node().kind() {
            NodeKind::Element(data) => data,
            _ => unreachable!("HtmlElement must wrap element nodes"),
        }
    }
}

impl Element for HtmlElement<'_> {
    type Impl = Simple;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.node())
    }

    fn parent_element(&self) -> Option<Self> {
        let parent = self.node().parent()?;
        match self.tree.node(parent).kind() {
            NodeKind::Element(_) => Some(Self::new(self.tree, parent)),
            _ => None,
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let mut current = self.node().prev_sibling();
        while let Some(node_id) = current {
            if matches!(self.tree.node(node_id).kind(), NodeKind::Element(_)) {
                return Some(Self::new(self.tree, node_id));
            }
            current = self.tree.node(node_id).prev_sibling();
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let mut current = self.node().next_sibling();
        while let Some(node_id) = current {
            if matches!(self.tree.node(node_id).kind(), NodeKind::Element(_)) {
                return Some(Self::new(self.tree, node_id));
            }
            current = self.tree.node(node_id).next_sibling();
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        for child in self.tree.children(self.id) {
            if matches!(self.tree.node(child).kind(), NodeKind::Element(_)) {
                return Some(Self::new(self.tree, child));
            }
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        self.element_data().name.ns == ns!(html)
    }

    fn has_local_name(&self, name: &CssLocalName) -> bool {
        self.element_data().name.local == name.0
    }

    fn has_namespace(&self, namespace: &Namespace) -> bool {
        &self.element_data().name.ns == namespace
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let left = &self.element_data().name;
        let right = &other.element_data().name;
        left.local == right.local && left.ns == right.ns
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&Namespace>,
        local_name: &CssLocalName,
        operation: &AttrSelectorOperation<&CssString>,
    ) -> bool {
        self.element_data().attrs.iter().any(|attr| {
            let namespace_matches = match ns {
                NamespaceConstraint::Any => true,
                NamespaceConstraint::Specific(url) => **url == attr.name.ns,
            };
            namespace_matches
                && local_name.0 == attr.name.local
                && operation.eval_str(attr.value.as_ref())
        })
    }

    fn match_non_ts_pseudo_class(
        &self,
        _pc: &NonTSPseudoClass,
        _context: &mut matching::MatchingContext<'_, Self::Impl>,
    ) -> bool {
        false
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut matching::MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: matching::ElementSelectorFlags) {}

    fn is_link(&self) -> bool {
        self.element_data().name.local == local_name!("link")
    }

    fn is_html_slot_element(&self) -> bool {
        self.element_data().name.local == local_name!("slot")
    }

    fn has_id(&self, id: &CssLocalName, case_sensitivity: CaseSensitivity) -> bool {
        match self.element_data().attr_value("id") {
            Some(val) => case_sensitivity.eq(val.as_bytes(), id.0.as_ref().as_bytes()),
            None => false,
        }
    }

    fn has_class(&self, name: &CssLocalName, case_sensitivity: CaseSensitivity) -> bool {
        let Some(value) = self.element_data().attr_value("class") else {
            return false;
        };

        value
            .split_ascii_whitespace()
            .any(|class| case_sensitivity.eq(class.as_bytes(), name.0.as_ref().as_bytes()))
    }

    fn has_custom_state(&self, _name: &CssLocalName) -> bool {
        false
    }

    fn imported_part(&self, _name: &CssLocalName) -> Option<CssLocalName> {
        None
    }

    fn is_part(&self, _name: &CssLocalName) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        !self
            .tree
            .children(self.id)
            .any(|child| match self.tree.node(child).kind() {
                NodeKind::Element(_) => true,
                NodeKind::Text(text) => !text.is_empty(),
                _ => false,
            })
    }

    fn is_root(&self) -> bool {
        let Some(parent) = self.node().parent() else {
            return false;
        };
        matches!(self.tree.node(parent).kind(), NodeKind::Document)
    }

    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        false
    }
}

fn selector_quirks_mode(mode: HtmlQuirksMode) -> QuirksMode {
    match mode {
        HtmlQuirksMode::NoQuirks => QuirksMode::NoQuirks,
        HtmlQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        HtmlQuirksMode::Quirks => QuirksMode::Quirks,
    }
}

fn traverse_preorder<F>(tree: &HtmlTree, start: NodeId, mut visit: F)
where
    F: FnMut(NodeId),
{
    let mut stack = vec![start];
    while let Some(node_id) = stack.pop() {
        visit(node_id);
        let mut children: Vec<NodeId> = tree.children(node_id).collect();
        children.reverse();
        stack.extend(children);
    }
}

fn find_css(selector: &Selector, tree: &HtmlTree) -> Vec<NodeId> {
    let mut caches = SelectorCaches::default();
    let quirks = selector_quirks_mode(tree.quirks_mode());
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        quirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    context.scope_element = tree
        .root_element()
        .map(|id| HtmlElement::new(tree, id).opaque());

    let mut matches = Vec::new();
    traverse_preorder(tree, tree.document(), |node_id| {
        if matches!(tree.node(node_id).kind(), NodeKind::Element(_)) {
            let element = HtmlElement::new(tree, node_id);
            if matching::matches_selector_list(selector.selector_list(), &element, &mut context) {
                matches.push(node_id);
            }
        }
    });
    matches
}

fn find_xpath(xpath: &XPath, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
    let expr = &xpath.0.0;
    let exprs = &expr.value.0;
    if exprs.len() != 1 {
        return Err(QueryExecError::Unsupported(
            "xpath execution expects a single expression",
        ));
    }

    match &exprs[0].value {
        ast::ExprSingle::Path(path) => eval_path_expr(tree, path, vec![tree.document()]),
        _ => Err(QueryExecError::Unsupported(
            "xpath execution only supports path expressions",
        )),
    }
}

fn eval_path_expr(
    tree: &HtmlTree,
    path: &ast::PathExpr,
    mut context: Vec<NodeId>,
) -> Result<Vec<NodeId>, QueryExecError> {
    for step in &path.steps {
        context = match &step.value {
            ast::StepExpr::AxisStep(axis_step) => eval_axis_step(tree, axis_step, &context)?,
            ast::StepExpr::PrimaryExpr(primary) => eval_primary_step(tree, primary, &context)?,
            ast::StepExpr::PostfixExpr { .. } => {
                return Err(QueryExecError::Unsupported(
                    "xpath postfix steps are not supported",
                ));
            }
        };
    }

    Ok(context)
}

fn eval_primary_step(
    tree: &HtmlTree,
    primary: &ast::PrimaryExprS,
    context: &[NodeId],
) -> Result<Vec<NodeId>, QueryExecError> {
    match &primary.value {
        ast::PrimaryExpr::FunctionCall(call) => {
            let name = &call.name.value;
            if name.local_name() == "root" && name.namespace() == FN_NAMESPACE {
                return Ok(vec![tree.document()]);
            }
            Err(QueryExecError::Unsupported(
                "xpath function calls are not supported",
            ))
        }
        ast::PrimaryExpr::ContextItem => Ok(context.to_vec()),
        _ => Err(QueryExecError::Unsupported(
            "xpath primary expressions are not supported",
        )),
    }
}

fn eval_axis_step(
    tree: &HtmlTree,
    axis_step: &ast::AxisStep,
    context: &[NodeId],
) -> Result<Vec<NodeId>, QueryExecError> {
    let mut results = Vec::new();
    let mut seen = FxHashSet::default();

    for &node_id in context {
        let candidates = axis_nodes(tree, node_id, &axis_step.axis)?;
        let mut matches = Vec::new();
        for candidate in candidates {
            if matches_node_test(tree, candidate, &axis_step.node_test)? {
                matches.push(candidate);
            }
        }

        let filtered = apply_predicates(matches, &axis_step.predicates)?;
        for node in filtered {
            if seen.insert(node) {
                results.push(node);
            }
        }
    }

    Ok(results)
}

fn axis_nodes(
    tree: &HtmlTree,
    context: NodeId,
    axis: &ast::Axis,
) -> Result<Vec<NodeId>, QueryExecError> {
    match axis {
        ast::Axis::Child => Ok(tree.children(context).collect()),
        ast::Axis::Descendant => Ok(descendants(tree, context, false)),
        ast::Axis::DescendantOrSelf => Ok(descendants(tree, context, true)),
        ast::Axis::Self_ => Ok(vec![context]),
        ast::Axis::Parent => Ok(tree.node(context).parent().into_iter().collect()),
        ast::Axis::Ancestor => Ok(ancestors(tree, context, false)),
        ast::Axis::AncestorOrSelf => Ok(ancestors(tree, context, true)),
        ast::Axis::FollowingSibling => Ok(following_siblings(tree, context)),
        ast::Axis::PrecedingSibling => Ok(preceding_siblings(tree, context)),
        ast::Axis::Attribute => Err(QueryExecError::Unsupported(
            "xpath attribute axis is not supported",
        )),
        ast::Axis::Namespace => Err(QueryExecError::Unsupported(
            "xpath namespace axis is not supported",
        )),
        ast::Axis::Following => Err(QueryExecError::Unsupported(
            "xpath following axis is not supported",
        )),
        ast::Axis::Preceding => Err(QueryExecError::Unsupported(
            "xpath preceding axis is not supported",
        )),
    }
}

fn descendants(tree: &HtmlTree, start: NodeId, include_self: bool) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    let mut stack = vec![start];
    let mut first = true;
    while let Some(node_id) = stack.pop() {
        if include_self || !first {
            nodes.push(node_id);
        }
        first = false;
        let mut children: Vec<NodeId> = tree.children(node_id).collect();
        children.reverse();
        stack.extend(children);
    }
    nodes
}

fn ancestors(tree: &HtmlTree, start: NodeId, include_self: bool) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    let mut current = Some(start);
    let mut first = true;
    while let Some(node_id) = current {
        if include_self || !first {
            nodes.push(node_id);
        }
        first = false;
        current = tree.node(node_id).parent();
    }
    nodes
}

fn following_siblings(tree: &HtmlTree, start: NodeId) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    let mut current = tree.node(start).next_sibling();
    while let Some(node_id) = current {
        nodes.push(node_id);
        current = tree.node(node_id).next_sibling();
    }
    nodes
}

fn preceding_siblings(tree: &HtmlTree, start: NodeId) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    let mut current = tree.node(start).prev_sibling();
    while let Some(node_id) = current {
        nodes.push(node_id);
        current = tree.node(node_id).prev_sibling();
    }
    nodes
}

fn matches_node_test(
    tree: &HtmlTree,
    node_id: NodeId,
    node_test: &ast::NodeTest,
) -> Result<bool, QueryExecError> {
    match node_test {
        ast::NodeTest::KindTest(kind_test) => matches_kind_test(tree, node_id, kind_test),
        ast::NodeTest::NameTest(name_test) => Ok(matches_name_test(tree, node_id, name_test)),
    }
}

fn matches_kind_test(
    tree: &HtmlTree,
    node_id: NodeId,
    kind_test: &ast::KindTest,
) -> Result<bool, QueryExecError> {
    match kind_test {
        ast::KindTest::Any => Ok(true),
        ast::KindTest::Document(_) => Ok(matches!(tree.node(node_id).kind(), NodeKind::Document)),
        ast::KindTest::Element(test) => Ok(match tree.node(node_id).kind() {
            NodeKind::Element(data) => match test {
                None => true,
                Some(element_test) => match &element_test.name_or_wildcard {
                    ast::NameOrWildcard::Wildcard => true,
                    ast::NameOrWildcard::Name(name) => {
                        let local = name.local_name();
                        data.name.local.as_ref().eq_ignore_ascii_case(local)
                            && namespace_matches(data, name.namespace())
                    }
                },
            },
            _ => false,
        }),
        ast::KindTest::Attribute(_) => Err(QueryExecError::Unsupported(
            "xpath attribute node tests are not supported",
        )),
        ast::KindTest::SchemaElement(_) | ast::KindTest::SchemaAttribute(_) => Err(
            QueryExecError::Unsupported("xpath schema node tests are not supported"),
        ),
        ast::KindTest::PI(test) => Ok(match tree.node(node_id).kind() {
            NodeKind::ProcessingInstruction { target, .. } => match test {
                None => true,
                Some(ast::PITest::Name(name) | ast::PITest::StringLiteral(name)) => {
                    target.as_ref() == name
                }
            },
            _ => false,
        }),
        ast::KindTest::Comment => Ok(matches!(tree.node(node_id).kind(), NodeKind::Comment(_))),
        ast::KindTest::Text => Ok(matches!(tree.node(node_id).kind(), NodeKind::Text(_))),
        ast::KindTest::NamespaceNode => Err(QueryExecError::Unsupported(
            "xpath namespace nodes are not supported",
        )),
    }
}

fn matches_name_test(tree: &HtmlTree, node_id: NodeId, name_test: &ast::NameTest) -> bool {
    let NodeKind::Element(data) = tree.node(node_id).kind() else {
        return false;
    };

    match name_test {
        ast::NameTest::Star => true,
        ast::NameTest::Name(name) => {
            data.name
                .local
                .as_ref()
                .eq_ignore_ascii_case(name.value.local_name())
                && namespace_matches(data, name.value.namespace())
        }
        ast::NameTest::LocalName(local) => data.name.local.as_ref().eq_ignore_ascii_case(local),
        ast::NameTest::Namespace(namespace) => data.name.ns.as_ref() == namespace,
    }
}

fn namespace_matches(element: &crate::html::ElementData, namespace: &str) -> bool {
    if namespace.is_empty() {
        return true;
    }
    element.name.ns.as_ref() == namespace
}

fn apply_predicates(
    nodes: Vec<NodeId>,
    predicates: &[ast::ExprS],
) -> Result<Vec<NodeId>, QueryExecError> {
    let mut current = nodes;
    for predicate in predicates {
        let Some(index) = predicate_index(predicate)? else {
            return Err(QueryExecError::Unsupported(
                "xpath predicate expressions are not supported",
            ));
        };
        if index == 0 || index > current.len() {
            current.clear();
            continue;
        }
        current = vec![current[index - 1]];
    }
    Ok(current)
}

fn predicate_index(predicate: &ast::ExprS) -> Result<Option<usize>, QueryExecError> {
    let exprs = &predicate.value.0;
    if exprs.len() != 1 {
        return Ok(None);
    }

    let ast::ExprSingle::Path(path) = &exprs[0].value else {
        return Ok(None);
    };
    if path.steps.len() != 1 {
        return Ok(None);
    }
    let ast::StepExpr::PrimaryExpr(primary) = &path.steps[0].value else {
        return Ok(None);
    };
    let ast::PrimaryExpr::Literal(ast::Literal::Integer(value)) = &primary.value else {
        return Ok(None);
    };

    if value.to_f64() <= 0.0 {
        return Ok(Some(0));
    }

    let index = u64::try_from(value)
        .map_err(|_| QueryExecError::Unsupported("xpath predicate index is out of range"))?;
    let index = usize::try_from(index)
        .map_err(|_| QueryExecError::Unsupported("xpath predicate index is out of range"))?;
    Ok(Some(index))
}

impl QueryExecutor for Selector {
    fn find(&self, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
        Ok(find_css(self, tree))
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

    #[test]
    fn query_find_css_and_xpath() {
        let html = r#"<div id="hero" class="a b"><span class="b"></span><span></span></div>"#;
        let tree = HtmlTree::parse(html);
        let hero = tree.index().by_id("hero").expect("missing hero id");
        let spans = tree.index().by_tag("span").expect("missing span nodes");
        let first_span = spans[0];

        let selector = Selector::parse("div#hero > span.b").unwrap();
        let matches = selector.find(&tree).unwrap();
        assert_eq!(matches, vec![first_span]);

        let query = Query::Css(selector);
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
}
