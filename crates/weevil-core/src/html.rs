//! High-performance HTML tree parsing and indexing.

mod error;
mod index;
mod iter;
mod serialize;

#[cfg(test)]
mod tests;

use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, ParseOpts, QualName, parse_document};
use std::borrow::Cow;
use std::cell::{Cell, RefCell, UnsafeCell};

use crate::node::{ElementAttr, SendStrTendril};
pub use crate::node::{ElementData, Node, NodeId, NodeKind};
pub use error::{HtmlParseError, HtmlParseIssue};
pub use index::HtmlIndex;
use index::index_attr;
pub use iter::{Children, Descendants, Subtree};

/// DOM-like HTML tree with fast tag/id/class indexes.
#[derive(Debug)]
pub struct HtmlTree {
    nodes: Vec<Node>,
    document: NodeId,
    quirks_mode: QuirksMode,
    index: HtmlIndex,
}

/// Output returned by HTML parsing when keeping parser issues.
#[derive(Debug)]
pub struct HtmlParseOutput {
    pub tree: HtmlTree,
    pub errors: Vec<HtmlParseIssue>,
}

impl HtmlTree {
    /// Parses UTF-8 HTML into a tree, ignoring parser issues.
    pub fn parse(html: &str) -> Self {
        Self::parse_bytes(html.as_bytes())
    }

    /// Parses UTF-8 HTML from bytes into a tree, ignoring parser issues.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        Self::parse_bytes_with_errors(bytes).tree
    }

    /// Parses UTF-8 HTML and returns an error when the parser reports issues.
    pub fn parse_checked(html: &str) -> Result<Self, HtmlParseError> {
        Self::parse_bytes_checked(html.as_bytes())
    }

    /// Parses UTF-8 HTML from bytes and returns an error when the parser reports issues.
    pub fn parse_bytes_checked(bytes: &[u8]) -> Result<Self, HtmlParseError> {
        let output = Self::parse_bytes_with_errors(bytes);
        if output.errors.is_empty() {
            Ok(output.tree)
        } else {
            Err(HtmlParseError::new(output.errors))
        }
    }

    /// Parses UTF-8 HTML while retaining parser issues.
    pub fn parse_with_errors(html: &str) -> HtmlParseOutput {
        Self::parse_bytes_with_errors(html.as_bytes())
    }

    /// Parses UTF-8 HTML from bytes while retaining parser issues.
    pub fn parse_bytes_with_errors(bytes: &[u8]) -> HtmlParseOutput {
        let sink = HtmlTreeBuilder::with_capacity(bytes.len());
        parse_document(sink, ParseOpts::default())
            .from_utf8()
            .one(bytes)
    }

    /// Returns the document node id.
    pub fn document(&self) -> NodeId {
        self.document
    }

    /// Returns the quirks mode chosen by the parser.
    pub fn quirks_mode(&self) -> QuirksMode {
        self.quirks_mode
    }

    /// Returns a node by id.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// Returns element data for a node id if it is an element.
    pub fn element(&self, id: NodeId) -> Option<&ElementData> {
        self.node(id).kind().as_element()
    }

    /// Returns the attribute value for a node id if it is an element.
    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.element(id)?.attr_value(name)
    }

    /// Returns the concatenated text content under a node.
    pub fn text_content(&self, id: NodeId) -> String {
        let mut text = String::new();
        for node_id in self.subtree(id) {
            if let NodeKind::Text(contents) = self.node(node_id).kind() {
                text.push_str(contents.as_ref());
            }
        }
        text
    }

    /// Returns the index for fast lookups.
    pub fn index(&self) -> &HtmlIndex {
        &self.index
    }

    /// Returns the first element child under the document node.
    pub fn root_element(&self) -> Option<NodeId> {
        let mut current = self.node(self.document).first_child;
        while let Some(node_id) = current {
            if matches!(self.node(node_id).kind(), NodeKind::Element(_)) {
                return Some(node_id);
            }
            current = self.node(node_id).next_sibling;
        }
        None
    }

    /// Iterates over the direct children of a node in document order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children::new(self, id)
    }

    /// Iterates over the descendants of a node in document order.
    pub fn descendants(&self, id: NodeId) -> Descendants<'_> {
        Descendants::new(self, id)
    }

    /// Iterates over a node and its descendants in document order.
    pub fn subtree(&self, id: NodeId) -> Subtree<'_> {
        Subtree::new(self, id)
    }
}

struct HtmlTreeBuilder {
    // UnsafeCell enables &self mutation required by TreeSink with minimal overhead.
    nodes: UnsafeCell<Vec<Node>>,
    index: RefCell<HtmlIndex>,
    parse_errors: RefCell<Vec<HtmlParseIssue>>,
    document: NodeId,
    quirks_mode: Cell<QuirksMode>,
}

impl HtmlTreeBuilder {
    fn with_capacity(byte_len: usize) -> Self {
        let estimated_nodes = (byte_len / 32).max(16);
        let mut nodes = Vec::with_capacity(estimated_nodes);
        let document = NodeId::new(0);
        nodes.push(Node::new(NodeKind::Document));
        Self {
            nodes: UnsafeCell::new(nodes),
            index: RefCell::new(HtmlIndex::with_capacity(estimated_nodes)),
            parse_errors: RefCell::new(Vec::new()),
            document,
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }

    #[inline]
    fn nodes(&self) -> &Vec<Node> {
        unsafe { &*self.nodes.get() }
    }

    #[inline]
    fn with_nodes_mut<R>(&self, f: impl FnOnce(&mut Vec<Node>) -> R) -> R {
        // Safety: TreeSink requires &self methods; UnsafeCell provides interior mutability.
        let nodes = unsafe { &mut *self.nodes.get() };
        f(nodes)
    }

    fn push_node(&self, kind: NodeKind) -> NodeId {
        self.with_nodes_mut(|nodes| {
            let id = NodeId::new(nodes.len());
            nodes.push(Node::new(kind));
            id
        })
    }

    fn index_element(&self, id: NodeId) {
        let nodes = self.nodes();
        let node = &nodes[id.index()];
        let element = match &node.kind {
            NodeKind::Element(data) => data,
            _ => return,
        };

        let mut index = self.index.borrow_mut();
        index.insert_tag(element.name.local.clone(), id);
        for attr in &element.attrs {
            index_attr(&mut index, id, attr);
        }
    }
}

impl TreeSink for HtmlTreeBuilder {
    type Handle = NodeId;
    type Output = HtmlParseOutput;
    type ElemName<'a>
        = &'a QualName
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        HtmlParseOutput {
            tree: HtmlTree {
                nodes: self.nodes.into_inner(),
                document: self.document,
                quirks_mode: self.quirks_mode.get(),
                index: self.index.into_inner(),
            },
            errors: self.parse_errors.into_inner(),
        }
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        self.parse_errors
            .borrow_mut()
            .push(HtmlParseIssue::new(msg.into_owned()));
    }

    fn get_document(&self) -> Self::Handle {
        self.document
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let nodes = self.nodes();
        match &nodes[target.index()].kind {
            NodeKind::Element(data) => &data.name,
            _ => panic!("not an element"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let template_contents = if flags.template {
            Some(self.push_node(NodeKind::Document))
        } else {
            None
        };

        let data = ElementData {
            name,
            attrs: to_element_attrs(attrs),
            template_contents,
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        };
        let id = self.push_node(NodeKind::Element(data));
        self.index_element(id);
        id
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.push_node(NodeKind::Comment(to_send_tendril(text)))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        self.push_node(NodeKind::ProcessingInstruction {
            target: to_send_tendril(target),
            data: to_send_tendril(data),
        })
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.with_nodes_mut(|nodes| {
            append_child(nodes, *parent, child);
        });
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        self.with_nodes_mut(|nodes| {
            if nodes[element.index()].parent.is_some() {
                append_before_sibling(nodes, *element, child);
            } else {
                append_child(nodes, *prev_element, child);
            }
        });
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.with_nodes_mut(|nodes| {
            let doctype = push_node(
                nodes,
                NodeKind::Doctype {
                    name: to_send_tendril(name),
                    public_id: to_send_tendril(public_id),
                    system_id: to_send_tendril(system_id),
                },
            );
            append_node(nodes, self.document, doctype);
        });
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        self.with_nodes_mut(|nodes| {
            let node = &mut nodes[target.index()];
            let element = match &mut node.kind {
                NodeKind::Element(data) => data,
                _ => panic!("not an element"),
            };

            if attrs.is_empty() {
                return;
            }

            let mut index = self.index.borrow_mut();
            for attr in attrs {
                if element
                    .attrs
                    .iter()
                    .any(|existing| existing.name == attr.name)
                {
                    continue;
                }
                let attr = to_element_attr(attr);
                index_attr(&mut index, *target, &attr);
                element.attrs.push(attr);
            }
        });
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.with_nodes_mut(|nodes| {
            detach_node(nodes, *target);
        });
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.with_nodes_mut(|nodes| {
            let mut child = nodes[node.index()].first_child;
            while let Some(child_id) = child {
                let next = nodes[child_id.index()].next_sibling;
                append_node(nodes, *new_parent, child_id);
                child = next;
            }
        });
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let nodes = self.nodes();
        match &nodes[target.index()].kind {
            NodeKind::Element(data) => data.template_contents.expect("template contents missing"),
            _ => panic!("not a template element"),
        }
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.with_nodes_mut(|nodes| {
            append_before_sibling(nodes, *sibling, child);
        });
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        let nodes = self.nodes();
        match &nodes[handle.index()].kind {
            NodeKind::Element(data) => data.mathml_annotation_xml_integration_point,
            _ => panic!("not an element"),
        }
    }
}

fn push_node(nodes: &mut Vec<Node>, kind: NodeKind) -> NodeId {
    let id = NodeId::new(nodes.len());
    nodes.push(Node::new(kind));
    id
}

fn append_child(nodes: &mut Vec<Node>, parent: NodeId, child: NodeOrText<NodeId>) {
    match child {
        NodeOrText::AppendText(text) => {
            let last_child = nodes[parent.index()].last_child;
            if append_text_to_previous(nodes, last_child, &text) {
                return;
            }
            let text_id = push_node(nodes, NodeKind::Text(to_send_tendril(text)));
            append_node(nodes, parent, text_id);
        }
        NodeOrText::AppendNode(node) => append_node(nodes, parent, node),
    }
}

fn append_before_sibling(nodes: &mut Vec<Node>, sibling: NodeId, child: NodeOrText<NodeId>) {
    match child {
        NodeOrText::AppendText(text) => {
            let prev = nodes[sibling.index()].prev_sibling;
            if append_text_to_previous(nodes, prev, &text) {
                return;
            }
            let text_id = push_node(nodes, NodeKind::Text(to_send_tendril(text)));
            insert_before(nodes, sibling, text_id);
        }
        NodeOrText::AppendNode(node) => insert_before(nodes, sibling, node),
    }
}

fn append_text_to_previous(
    nodes: &mut [Node],
    previous: Option<NodeId>,
    text: &StrTendril,
) -> bool {
    let Some(prev_id) = previous else {
        return false;
    };

    match &mut nodes[prev_id.index()].kind {
        NodeKind::Text(existing) => {
            let incoming = to_send_tendril(text.clone());
            existing.push_tendril(&incoming);
            true
        }
        _ => false,
    }
}

fn to_send_tendril(value: StrTendril) -> SendStrTendril {
    SendStrTendril::from(value.into_send())
}

fn to_element_attr(attr: Attribute) -> ElementAttr {
    ElementAttr {
        name: attr.name,
        value: to_send_tendril(attr.value),
    }
}

fn to_element_attrs(attrs: Vec<Attribute>) -> Vec<ElementAttr> {
    attrs.into_iter().map(to_element_attr).collect()
}

fn append_node(nodes: &mut [Node], parent: NodeId, child: NodeId) {
    detach_node(nodes, child);

    let parent_idx = parent.index();
    let child_idx = child.index();
    let last_child = nodes[parent_idx].last_child;

    nodes[child_idx].parent = Some(parent);
    nodes[child_idx].prev_sibling = last_child;
    nodes[child_idx].next_sibling = None;

    if let Some(last_id) = last_child {
        nodes[last_id.index()].next_sibling = Some(child);
    } else {
        nodes[parent_idx].first_child = Some(child);
    }
    nodes[parent_idx].last_child = Some(child);
}

fn insert_before(nodes: &mut [Node], sibling: NodeId, new_node: NodeId) {
    detach_node(nodes, new_node);

    let sibling_idx = sibling.index();
    let new_idx = new_node.index();
    let parent = nodes[sibling_idx].parent;
    let prev = nodes[sibling_idx].prev_sibling;

    nodes[new_idx].parent = parent;
    nodes[new_idx].next_sibling = Some(sibling);
    nodes[new_idx].prev_sibling = prev;

    if let Some(prev_id) = prev {
        nodes[prev_id.index()].next_sibling = Some(new_node);
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].first_child = Some(new_node);
    }

    nodes[sibling_idx].prev_sibling = Some(new_node);
}

fn detach_node(nodes: &mut [Node], target: NodeId) {
    let idx = target.index();
    let parent = nodes[idx].parent;
    let prev = nodes[idx].prev_sibling;
    let next = nodes[idx].next_sibling;

    nodes[idx].parent = None;
    nodes[idx].prev_sibling = None;
    nodes[idx].next_sibling = None;

    if let Some(next_id) = next {
        nodes[next_id.index()].prev_sibling = prev;
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].last_child = prev;
    }

    if let Some(prev_id) = prev {
        nodes[prev_id.index()].next_sibling = next;
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].first_child = next;
    }
}
