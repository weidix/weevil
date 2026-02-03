use crate::node::NodeId;

use super::HtmlTree;

/// Iterator over child nodes.
pub struct Children<'a> {
    tree: &'a HtmlTree,
    next: Option<NodeId>,
}

impl<'a> Children<'a> {
    pub(crate) fn new(tree: &'a HtmlTree, parent: NodeId) -> Self {
        Self {
            tree,
            next: tree.node(parent).first_child,
        }
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.tree.node(current).next_sibling;
        Some(current)
    }
}

/// Iterator over the descendants of a node in document order.
pub struct Descendants<'a> {
    tree: &'a HtmlTree,
    stack: Vec<NodeId>,
}

impl<'a> Descendants<'a> {
    pub(crate) fn new(tree: &'a HtmlTree, parent: NodeId) -> Self {
        let mut stack = Vec::new();
        push_children(tree, parent, &mut stack);
        Self { tree, stack }
    }
}

impl<'a> Iterator for Descendants<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        push_children(self.tree, current, &mut self.stack);
        Some(current)
    }
}

/// Iterator over a node and its descendants in document order.
pub struct Subtree<'a> {
    tree: &'a HtmlTree,
    stack: Vec<NodeId>,
}

impl<'a> Subtree<'a> {
    pub(crate) fn new(tree: &'a HtmlTree, root: NodeId) -> Self {
        Self {
            tree,
            stack: vec![root],
        }
    }
}

impl<'a> Iterator for Subtree<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        push_children(self.tree, current, &mut self.stack);
        Some(current)
    }
}

fn push_children(tree: &HtmlTree, parent: NodeId, stack: &mut Vec<NodeId>) {
    let mut children: Vec<NodeId> = tree.children(parent).collect();
    children.reverse();
    stack.extend(children);
}
