use crate::html::{HtmlTree, NodeId, NodeKind};
use crate::xpath::XPath;
use rustc_hash::FxHashSet;
use xee_xpath_ast::ast;
use xee_xpath_ast::{FN_NAMESPACE, Name};
use xot::xmlname::NameStrInfo;

use super::xpath_predicate::apply_predicates;
use super::{QueryExecError, QueryExecFeature};

const XPATH_HINT: &str = "Supported XPath: a single path expression; axes child, descendant, descendant-or-self, self, parent, ancestor, ancestor-or-self, following-sibling, preceding-sibling; name and kind node tests (document, element, text, comment, processing-instruction); predicates with a single integer literal [n] (1-based), attribute existence [@id], string comparisons on @attr/text()/., boolean and/or, contains()/starts-with() with string args; fn:root() only.";

pub(super) fn xpath_unsupported(
    feature: QueryExecFeature,
    detail: impl Into<String>,
) -> QueryExecError {
    QueryExecError::unsupported(feature, detail, Some(XPATH_HINT))
}

pub(crate) fn find_xpath_in(
    xpath: &XPath,
    tree: &HtmlTree,
    start: NodeId,
) -> Result<Vec<NodeId>, QueryExecError> {
    let expr = &xpath.0.0;
    let exprs = &expr.value.0;
    if exprs.len() != 1 {
        let count = exprs.len();
        return Err(xpath_unsupported(
            QueryExecFeature::XPathExpression,
            format!("expected a single expression, got {count}"),
        ));
    }

    match &exprs[0].value {
        ast::ExprSingle::Path(path) => eval_path_expr(tree, path, vec![start]),
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathExpression,
            format!(
                "only path expressions are supported, got {}",
                expr_single_kind(other)
            ),
        )),
    }
}

pub(super) fn eval_path_expr(
    tree: &HtmlTree,
    path: &ast::PathExpr,
    mut context: Vec<NodeId>,
) -> Result<Vec<NodeId>, QueryExecError> {
    for step in &path.steps {
        context = match &step.value {
            ast::StepExpr::AxisStep(axis_step) => eval_axis_step(tree, axis_step, &context)?,
            ast::StepExpr::PrimaryExpr(primary) => eval_primary_step(tree, primary, &context)?,
            ast::StepExpr::PostfixExpr { .. } => {
                return Err(xpath_unsupported(
                    QueryExecFeature::XPathPostfix,
                    "postfix steps are not supported",
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
            let qualified = format_function_name(name);
            Err(xpath_unsupported(
                QueryExecFeature::XPathFunction,
                format!("function {qualified} is not supported"),
            ))
        }
        ast::PrimaryExpr::ContextItem => Ok(context.to_vec()),
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathPrimaryExpr,
            format!(
                "primary expression {} is not supported",
                primary_expr_kind(other)
            ),
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

        let filtered = apply_predicates(tree, matches, &axis_step.predicates)?;
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
        ast::Axis::Attribute
        | ast::Axis::Namespace
        | ast::Axis::Following
        | ast::Axis::Preceding => Err(xpath_unsupported(
            QueryExecFeature::XPathAxis,
            format!("axis {} is not supported", axis_label(axis)),
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
        ast::KindTest::Attribute(_)
        | ast::KindTest::SchemaElement(_)
        | ast::KindTest::SchemaAttribute(_) => Err(xpath_unsupported(
            QueryExecFeature::XPathKindTest,
            format!("kind test {} is not supported", kind_test_label(kind_test)),
        )),
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
        ast::KindTest::NamespaceNode => Err(xpath_unsupported(
            QueryExecFeature::XPathKindTest,
            format!("kind test {} is not supported", kind_test_label(kind_test)),
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

pub(super) fn expr_single_kind(expr: &ast::ExprSingle) -> &'static str {
    match expr {
        ast::ExprSingle::Path(_) => "path expression",
        ast::ExprSingle::Apply(_) => "apply expression",
        ast::ExprSingle::Let(_) => "let expression",
        ast::ExprSingle::If(_) => "if expression",
        ast::ExprSingle::Binary(_) => "binary expression",
        ast::ExprSingle::For(_) => "for expression",
        ast::ExprSingle::Quantified(_) => "quantified expression",
    }
}

pub(super) fn primary_expr_kind(expr: &ast::PrimaryExpr) -> &'static str {
    match expr {
        ast::PrimaryExpr::Literal(_) => "literal",
        ast::PrimaryExpr::VarRef(_) => "variable reference",
        ast::PrimaryExpr::Expr(_) => "parenthesized expression",
        ast::PrimaryExpr::ContextItem => "context item",
        ast::PrimaryExpr::FunctionCall(_) => "function call",
        ast::PrimaryExpr::NamedFunctionRef(_) => "named function reference",
        ast::PrimaryExpr::InlineFunction(_) => "inline function",
        ast::PrimaryExpr::MapConstructor(_) => "map constructor",
        ast::PrimaryExpr::ArrayConstructor(_) => "array constructor",
        ast::PrimaryExpr::UnaryLookup(_) => "unary lookup",
    }
}

fn axis_label(axis: &ast::Axis) -> &'static str {
    match axis {
        ast::Axis::Ancestor => "ancestor",
        ast::Axis::AncestorOrSelf => "ancestor-or-self",
        ast::Axis::Attribute => "attribute",
        ast::Axis::Child => "child",
        ast::Axis::Descendant => "descendant",
        ast::Axis::DescendantOrSelf => "descendant-or-self",
        ast::Axis::Following => "following",
        ast::Axis::FollowingSibling => "following-sibling",
        ast::Axis::Namespace => "namespace",
        ast::Axis::Parent => "parent",
        ast::Axis::Preceding => "preceding",
        ast::Axis::PrecedingSibling => "preceding-sibling",
        ast::Axis::Self_ => "self",
    }
}

fn kind_test_label(kind_test: &ast::KindTest) -> &'static str {
    match kind_test {
        ast::KindTest::Any => "any",
        ast::KindTest::Document(_) => "document",
        ast::KindTest::Element(_) => "element",
        ast::KindTest::Attribute(_) => "attribute",
        ast::KindTest::SchemaElement(_) => "schema-element",
        ast::KindTest::SchemaAttribute(_) => "schema-attribute",
        ast::KindTest::PI(_) => "processing-instruction",
        ast::KindTest::Comment => "comment",
        ast::KindTest::Text => "text",
        ast::KindTest::NamespaceNode => "namespace-node",
    }
}

fn format_function_name(name: &Name) -> String {
    let local = name.local_name();
    let namespace = name.namespace();
    if namespace.is_empty() {
        local.to_string()
    } else {
        format!("{namespace}:{local}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::HtmlTree;
    use crate::xpath::XPath;

    fn sample_tree() -> HtmlTree {
        let html = r#"
        <html>
          <body>
            <div id="outer">
              <span id="first"></span>
              text
              <span id="second"></span>
              <em id="third"></em>
            </div>
          </body>
        </html>
        "#;
        HtmlTree::parse(html)
    }

    fn predicate_tree() -> HtmlTree {
        let html =
            r#"<div id="root"><span id="first">Hello</span><span class="title">World</span></div>"#;
        HtmlTree::parse(html)
    }

    #[test]
    fn find_xpath_handles_axes_and_predicates() {
        let tree = sample_tree();
        let first = tree.index().by_id("first").expect("missing first");
        let second = tree.index().by_id("second").expect("missing second");
        let third = tree.index().by_id("third").expect("missing third");
        let outer = tree.index().by_id("outer").expect("missing outer");

        let xpath = XPath::parse("//span[1]").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//span/following-sibling::span").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![second]);

        let xpath = XPath::parse("//span/preceding-sibling::span").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//em/ancestor::div").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![outer]);

        let xpath = XPath::parse("//em/parent::div").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![outer]);

        let xpath = XPath::parse("//em/self::em").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![third]);

        let xpath = XPath::parse("//div/descendant::em").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![third]);
    }

    #[test]
    fn find_xpath_rejects_unsupported_axis() {
        let tree = sample_tree();
        let xpath = XPath::parse("//span/attribute::id").unwrap();
        let err = find_xpath_in(&xpath, &tree, tree.document()).unwrap_err();
        assert_eq!(err.feature(), QueryExecFeature::XPathAxis);
        assert!(err.detail().contains("attribute"));
        assert!(err.hint().is_some());
    }

    #[test]
    fn find_xpath_zero_predicate_is_empty() {
        let tree = sample_tree();
        let xpath = XPath::parse("//span[0]").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn find_xpath_supports_attribute_and_text_predicates() {
        let tree = predicate_tree();
        let first = tree.index().by_id("first").expect("missing first");
        let root = tree.index().by_id("root").expect("missing root");
        let title = tree.index().by_class("title").expect("missing title")[0];

        let xpath = XPath::parse("//span[@id]").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//span[contains(@class, 'title')]").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![title]);

        let xpath = XPath::parse("//span[text()='Hello']").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//div[contains(., 'World')]").unwrap();
        let matches = find_xpath_in(&xpath, &tree, tree.document()).unwrap();
        assert_eq!(matches, vec![root]);
    }
}
