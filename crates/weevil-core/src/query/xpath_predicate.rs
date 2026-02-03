use crate::html::{HtmlTree, NodeId, NodeKind};
use xee_xpath_ast::FN_NAMESPACE;
use xee_xpath_ast::ast;
use xot::xmlname::NameStrInfo;

use super::xpath_exec::{eval_path_expr, expr_single_kind, primary_expr_kind, xpath_unsupported};
use super::{QueryExecError, QueryExecFeature};

pub(super) fn apply_predicates(
    tree: &HtmlTree,
    nodes: Vec<NodeId>,
    predicates: &[ast::ExprS],
) -> Result<Vec<NodeId>, QueryExecError> {
    let mut current = nodes;
    for predicate in predicates {
        if let Some(index) = predicate_index(predicate)? {
            if index == 0 || index > current.len() {
                current.clear();
                continue;
            }
            current = vec![current[index - 1]];
            continue;
        }

        let mut filtered = Vec::new();
        for node_id in current {
            if predicate_matches(tree, node_id, predicate)? {
                filtered.push(node_id);
            }
        }
        current = filtered;
    }
    Ok(current)
}

fn predicate_matches(
    tree: &HtmlTree,
    node_id: NodeId,
    predicate: &ast::ExprS,
) -> Result<bool, QueryExecError> {
    let exprs = &predicate.value.0;
    if exprs.len() != 1 {
        let count = exprs.len();
        return Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!("expected a single predicate expression, got {count}"),
        ));
    }

    match &exprs[0].value {
        ast::ExprSingle::Path(path) => eval_predicate_path(tree, node_id, path),
        ast::ExprSingle::Binary(binary) => eval_predicate_binary(tree, node_id, binary),
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!(
                "predicate expression {} is not supported",
                expr_single_kind(other)
            ),
        )),
    }
}

fn eval_predicate_path(
    tree: &HtmlTree,
    node_id: NodeId,
    path: &ast::PathExpr,
) -> Result<bool, QueryExecError> {
    if let Some(call) = function_call_from_path(path) {
        return eval_function_call(tree, node_id, call);
    }

    let value = value_from_path_expr(tree, node_id, path)?;
    Ok(value.as_bool())
}

fn function_call_from_path(path: &ast::PathExpr) -> Option<&ast::FunctionCall> {
    if path.steps.len() != 1 {
        return None;
    }
    let ast::StepExpr::PrimaryExpr(primary) = &path.steps[0].value else {
        return None;
    };
    match &primary.value {
        ast::PrimaryExpr::FunctionCall(call) => Some(call),
        _ => None,
    }
}

fn eval_predicate_binary(
    tree: &HtmlTree,
    node_id: NodeId,
    binary: &ast::BinaryExpr,
) -> Result<bool, QueryExecError> {
    use ast::BinaryOperator;

    let left = value_from_path_expr(tree, node_id, &binary.left)?;
    let right = value_from_path_expr(tree, node_id, &binary.right)?;

    match binary.operator {
        BinaryOperator::And => Ok(left.as_bool() && right.as_bool()),
        BinaryOperator::Or => Ok(left.as_bool() || right.as_bool()),
        BinaryOperator::ValueEq | BinaryOperator::GenEq => {
            Ok(left.as_string(tree) == right.as_string(tree))
        }
        BinaryOperator::ValueNe | BinaryOperator::GenNe => {
            Ok(left.as_string(tree) != right.as_string(tree))
        }
        BinaryOperator::ValueLt
        | BinaryOperator::ValueLe
        | BinaryOperator::ValueGt
        | BinaryOperator::ValueGe
        | BinaryOperator::GenLt
        | BinaryOperator::GenLe
        | BinaryOperator::GenGt
        | BinaryOperator::GenGe => eval_numeric_comparison(tree, &left, &right, binary.operator),
        _ => Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!("binary operator {:?} is not supported", binary.operator),
        )),
    }
}

fn eval_numeric_comparison(
    tree: &HtmlTree,
    left: &Value,
    right: &Value,
    operator: ast::BinaryOperator,
) -> Result<bool, QueryExecError> {
    let left_value = left.as_number(tree)?;
    let right_value = right.as_number(tree)?;

    let result = match operator {
        ast::BinaryOperator::ValueLt | ast::BinaryOperator::GenLt => left_value < right_value,
        ast::BinaryOperator::ValueLe | ast::BinaryOperator::GenLe => left_value <= right_value,
        ast::BinaryOperator::ValueGt | ast::BinaryOperator::GenGt => left_value > right_value,
        ast::BinaryOperator::ValueGe | ast::BinaryOperator::GenGe => left_value >= right_value,
        _ => {
            return Err(xpath_unsupported(
                QueryExecFeature::XPathPredicate,
                format!("numeric operator {:?} is not supported", operator),
            ));
        }
    };

    Ok(result)
}

fn eval_function_call(
    tree: &HtmlTree,
    node_id: NodeId,
    call: &ast::FunctionCall,
) -> Result<bool, QueryExecError> {
    let name = &call.name.value;
    let local = name.local_name();
    let namespace = name.namespace();
    if !namespace.is_empty() && namespace != FN_NAMESPACE {
        let qualified = format_function_name(name);
        return Err(xpath_unsupported(
            QueryExecFeature::XPathFunction,
            format!("function {qualified} is not supported"),
        ));
    }

    match local {
        "contains" => eval_contains(tree, node_id, call),
        "starts-with" => eval_starts_with(tree, node_id, call),
        _ => {
            let qualified = format_function_name(name);
            Err(xpath_unsupported(
                QueryExecFeature::XPathFunction,
                format!("function {qualified} is not supported"),
            ))
        }
    }
}

fn eval_contains(
    tree: &HtmlTree,
    node_id: NodeId,
    call: &ast::FunctionCall,
) -> Result<bool, QueryExecError> {
    if call.arguments.len() != 2 {
        let count = call.arguments.len();
        return Err(xpath_unsupported(
            QueryExecFeature::XPathFunction,
            format!("contains() expects 2 arguments, got {count}"),
        ));
    }

    let haystack = expr_single_to_string(tree, node_id, &call.arguments[0].value)?;
    let needle = expr_single_to_string(tree, node_id, &call.arguments[1].value)?;
    Ok(haystack.contains(&needle))
}

fn eval_starts_with(
    tree: &HtmlTree,
    node_id: NodeId,
    call: &ast::FunctionCall,
) -> Result<bool, QueryExecError> {
    if call.arguments.len() != 2 {
        let count = call.arguments.len();
        return Err(xpath_unsupported(
            QueryExecFeature::XPathFunction,
            format!("starts-with() expects 2 arguments, got {count}"),
        ));
    }

    let haystack = expr_single_to_string(tree, node_id, &call.arguments[0].value)?;
    let needle = expr_single_to_string(tree, node_id, &call.arguments[1].value)?;
    Ok(haystack.starts_with(&needle))
}

fn expr_single_to_string(
    tree: &HtmlTree,
    node_id: NodeId,
    expr: &ast::ExprSingle,
) -> Result<String, QueryExecError> {
    match expr {
        ast::ExprSingle::Path(path) => {
            Ok(value_from_path_expr(tree, node_id, path)?.as_string(tree))
        }
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!(
                "predicate expression {} is not supported in function arguments",
                expr_single_kind(other)
            ),
        )),
    }
}

#[derive(Debug)]
enum Value {
    Nodes(Vec<NodeId>),
    String(String),
    Number(f64),
    Attribute(Option<String>),
}

impl Value {
    fn as_bool(&self) -> bool {
        match self {
            Value::Nodes(nodes) => !nodes.is_empty(),
            Value::String(value) => !value.is_empty(),
            Value::Number(value) => *value != 0.0,
            Value::Attribute(value) => value.is_some(),
        }
    }

    fn as_string(&self, tree: &HtmlTree) -> String {
        match self {
            Value::Nodes(nodes) => nodes
                .first()
                .map(|node_id| node_string_value(tree, *node_id))
                .unwrap_or_default(),
            Value::String(value) => value.clone(),
            Value::Number(value) => format!("{value}"),
            Value::Attribute(value) => value.clone().unwrap_or_default(),
        }
    }

    fn as_number(&self, tree: &HtmlTree) -> Result<f64, QueryExecError> {
        let value = match self {
            Value::Number(value) => return Ok(*value),
            Value::String(value) => value.clone(),
            Value::Attribute(value) => value.clone().unwrap_or_default(),
            Value::Nodes(nodes) => nodes
                .first()
                .map(|node_id| node_string_value(tree, *node_id))
                .unwrap_or_default(),
        };

        value.parse::<f64>().map_err(|_| {
            xpath_unsupported(
                QueryExecFeature::XPathPredicate,
                format!("value {value:?} is not a number"),
            )
        })
    }
}

fn value_from_path_expr(
    tree: &HtmlTree,
    node_id: NodeId,
    path: &ast::PathExpr,
) -> Result<Value, QueryExecError> {
    if path.steps.len() == 1 {
        match &path.steps[0].value {
            ast::StepExpr::PrimaryExpr(primary) => value_from_primary_expr(tree, node_id, primary),
            ast::StepExpr::AxisStep(axis_step) => {
                if matches!(axis_step.axis, ast::Axis::Attribute) {
                    return attribute_value(tree, node_id, &axis_step.node_test)
                        .map(Value::Attribute);
                }
                let nodes = eval_path_expr(tree, path, vec![node_id])?;
                Ok(Value::Nodes(nodes))
            }
            ast::StepExpr::PostfixExpr { .. } => Err(xpath_unsupported(
                QueryExecFeature::XPathPostfix,
                "postfix expressions are not supported in predicates",
            )),
        }
    } else {
        if path_has_attribute_axis(path) {
            return Err(xpath_unsupported(
                QueryExecFeature::XPathAxis,
                "attribute axis in predicate expressions is not supported",
            ));
        }
        let nodes = eval_path_expr(tree, path, vec![node_id])?;
        Ok(Value::Nodes(nodes))
    }
}

fn value_from_primary_expr(
    tree: &HtmlTree,
    node_id: NodeId,
    primary: &ast::PrimaryExprS,
) -> Result<Value, QueryExecError> {
    match &primary.value {
        ast::PrimaryExpr::Literal(literal) => value_from_literal(literal),
        ast::PrimaryExpr::ContextItem => Ok(Value::Nodes(vec![node_id])),
        ast::PrimaryExpr::Expr(expr) => value_from_expr(tree, node_id, expr),
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!(
                "primary expression {} is not supported in predicates",
                primary_expr_kind(other)
            ),
        )),
    }
}

fn value_from_expr(
    tree: &HtmlTree,
    node_id: NodeId,
    expr: &ast::ExprOrEmptyS,
) -> Result<Value, QueryExecError> {
    let Some(expr) = &expr.value else {
        return Ok(Value::String(String::new()));
    };
    let exprs = &expr.0;
    if exprs.len() != 1 {
        let count = exprs.len();
        return Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!("expected a single parenthesized expression, got {count}"),
        ));
    }

    match &exprs[0].value {
        ast::ExprSingle::Path(path) => value_from_path_expr(tree, node_id, path),
        ast::ExprSingle::Binary(binary) => {
            let matches = eval_predicate_binary(tree, node_id, binary)?;
            Ok(Value::Number(if matches { 1.0 } else { 0.0 }))
        }
        other => Err(xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!(
                "parenthesized expression {} is not supported in predicates",
                expr_single_kind(other)
            ),
        )),
    }
}

fn value_from_literal(literal: &ast::Literal) -> Result<Value, QueryExecError> {
    match literal {
        ast::Literal::String(value) => Ok(Value::String(value.clone())),
        ast::Literal::Integer(value) => {
            let number = value.to_f64();
            if !number.is_finite() {
                let rendered_value = format!("{value}");
                return Err(xpath_unsupported(
                    QueryExecFeature::XPathPredicate,
                    format!("integer literal {rendered_value} is out of range"),
                ));
            }
            Ok(Value::Number(number))
        }
        ast::Literal::Decimal(value) => {
            let rendered_value = value.to_string();
            let number = rendered_value.parse::<f64>().map_err(|_| {
                xpath_unsupported(
                    QueryExecFeature::XPathPredicate,
                    format!("decimal literal {rendered_value} is out of range"),
                )
            })?;
            Ok(Value::Number(number))
        }
        ast::Literal::Double(value) => Ok(Value::Number(value.into_inner())),
    }
}

fn path_has_attribute_axis(path: &ast::PathExpr) -> bool {
    path.steps.iter().any(|step| {
        matches!(
            step.value,
            ast::StepExpr::AxisStep(ast::AxisStep {
                axis: ast::Axis::Attribute,
                ..
            })
        )
    })
}

fn attribute_value(
    tree: &HtmlTree,
    node_id: NodeId,
    node_test: &ast::NodeTest,
) -> Result<Option<String>, QueryExecError> {
    let element = match tree.node(node_id).kind() {
        NodeKind::Element(data) => data,
        _ => return Ok(None),
    };

    match node_test {
        ast::NodeTest::NameTest(name_test) => match name_test {
            ast::NameTest::Name(name) => Ok(attribute_value_by_name(
                element,
                name.value.local_name(),
                name.value.namespace(),
            )),
            ast::NameTest::LocalName(local) => Ok(element.attr_value(local).map(str::to_string)),
            ast::NameTest::Star => {
                if element.attrs.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(String::new()))
                }
            }
            ast::NameTest::Namespace(namespace) => {
                for attr in &element.attrs {
                    if attr.name.ns.as_ref() == namespace {
                        return Ok(Some(attr.value.to_string()));
                    }
                }
                Ok(None)
            }
        },
        ast::NodeTest::KindTest(kind_test) => match kind_test {
            ast::KindTest::Attribute(test) => match test {
                None => {
                    if element.attrs.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(String::new()))
                    }
                }
                Some(attribute_test) => match &attribute_test.name_or_wildcard {
                    ast::NameOrWildcard::Name(name) => Ok(attribute_value_by_name(
                        element,
                        name.local_name(),
                        name.namespace(),
                    )),
                    ast::NameOrWildcard::Wildcard => {
                        if element.attrs.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(String::new()))
                        }
                    }
                },
            },
            other => Err(xpath_unsupported(
                QueryExecFeature::XPathNodeTest,
                format!(
                    "node test {} is not supported for attributes",
                    node_test_label(other)
                ),
            )),
        },
    }
}

fn attribute_value_by_name(
    element: &crate::html::ElementData,
    local: &str,
    namespace: &str,
) -> Option<String> {
    let normalized = crate::node::normalize_tag_name(local);
    for attr in &element.attrs {
        if attr.name.local.as_ref() == normalized.as_ref()
            && (namespace.is_empty() || attr.name.ns.as_ref() == namespace)
        {
            return Some(attr.value.to_string());
        }
    }
    None
}

fn node_string_value(tree: &HtmlTree, node_id: NodeId) -> String {
    match tree.node(node_id).kind() {
        NodeKind::Text(text) => text.to_string(),
        NodeKind::Comment(text) => text.to_string(),
        NodeKind::ProcessingInstruction { data, .. } => data.to_string(),
        NodeKind::Element(_) | NodeKind::Document => tree.text_content(node_id),
        NodeKind::Doctype { .. } => String::new(),
    }
}

fn node_test_label(kind_test: &ast::KindTest) -> &'static str {
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

fn format_function_name(name: &xee_xpath_ast::Name) -> String {
    let local = name.local_name();
    let namespace = name.namespace();
    if namespace.is_empty() {
        local.to_string()
    } else {
        format!("{namespace}:{local}")
    }
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

    let rendered_value = format!("{value}");
    let index = u64::try_from(value).map_err(|_| {
        xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!("predicate index {rendered_value} is out of range"),
        )
    })?;
    let index = usize::try_from(index).map_err(|_| {
        xpath_unsupported(
            QueryExecFeature::XPathPredicate,
            format!("predicate index {rendered_value} is out of range"),
        )
    })?;
    Ok(Some(index))
}
