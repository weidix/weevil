use crate::html::{HtmlTree, NodeId, NodeKind};
use crate::xpath::XPath;
use rustc_hash::FxHashSet;
use xee_xpath_ast::FN_NAMESPACE;
use xee_xpath_ast::ast;
use xot::xmlname::NameStrInfo;

use super::QueryExecError;

pub(crate) fn find_xpath(xpath: &XPath, tree: &HtmlTree) -> Result<Vec<NodeId>, QueryExecError> {
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

    #[test]
    fn find_xpath_handles_axes_and_predicates() {
        let tree = sample_tree();
        let first = tree.index().by_id("first").expect("missing first");
        let second = tree.index().by_id("second").expect("missing second");
        let third = tree.index().by_id("third").expect("missing third");
        let outer = tree.index().by_id("outer").expect("missing outer");

        let xpath = XPath::parse("//span[1]").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//span/following-sibling::span").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![second]);

        let xpath = XPath::parse("//span/preceding-sibling::span").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![first]);

        let xpath = XPath::parse("//em/ancestor::div").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![outer]);

        let xpath = XPath::parse("//em/parent::div").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![outer]);

        let xpath = XPath::parse("//em/self::em").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![third]);

        let xpath = XPath::parse("//div/descendant::em").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert_eq!(matches, vec![third]);
    }

    #[test]
    fn find_xpath_rejects_unsupported_axis() {
        let tree = sample_tree();
        let xpath = XPath::parse("//span/attribute::id").unwrap();
        let err = find_xpath(&xpath, &tree).unwrap_err();
        match err {
            QueryExecError::Unsupported(message) => {
                assert!(message.contains("attribute axis"));
            }
        }
    }

    #[test]
    fn find_xpath_zero_predicate_is_empty() {
        let tree = sample_tree();
        let xpath = XPath::parse("//span[0]").unwrap();
        let matches = find_xpath(&xpath, &tree).unwrap();
        assert!(matches.is_empty());
    }
}
