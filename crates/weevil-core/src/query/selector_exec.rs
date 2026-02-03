use crate::html::{HtmlTree, NodeId, NodeKind};
use crate::selector::{CssLocalName, CssString, NonTSPseudoClass, PseudoElement, Selector, Simple};
use html5ever::interface::tree_builder::QuirksMode as HtmlQuirksMode;
use html5ever::{Namespace, local_name, ns};
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::{Element, OpaqueElement, matching};

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

pub(crate) fn find_selector(selector: &Selector, tree: &HtmlTree) -> Vec<NodeId> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use selectors::Element;

    fn sample_tree() -> HtmlTree {
        let html = r#"
        <!doctype html>
        <html>
          <body>
            <div id="root" data-role="main" class="wrap">
              <span id="first"></span>
              text
              <span id="second" class="hit"></span>
              <!-- comment -->
            </div>
            <p class="hit"></p>
          </body>
        </html>
        "#;
        HtmlTree::parse(html)
    }

    #[test]
    fn element_navigation_skips_non_elements() {
        let tree = sample_tree();
        let first = tree.index().by_id("first").expect("missing first");
        let second = tree.index().by_id("second").expect("missing second");
        let root_div = tree.index().by_id("root").expect("missing root");
        let html_id = tree.root_element().expect("missing html");

        let first_el = HtmlElement::new(&tree, first);
        let second_el = HtmlElement::new(&tree, second);
        let root_el = HtmlElement::new(&tree, root_div);
        let html_el = HtmlElement::new(&tree, html_id);

        assert_eq!(first_el.next_sibling_element().unwrap().id, second);
        assert_eq!(second_el.prev_sibling_element().unwrap().id, first);
        assert_eq!(root_el.first_element_child().unwrap().id, first);
        assert_eq!(second_el.parent_element().unwrap().id, root_div);
        assert!(html_el.is_root());
        assert!(!root_el.is_root());
        assert!(first_el.is_empty());
        assert!(!root_el.is_empty());
        assert!(html_el.parent_element().is_none());
    }

    #[test]
    fn find_selector_matches_attributes_and_classes() {
        let tree = sample_tree();
        let second = tree.index().by_id("second").expect("missing second");

        let selector =
            Selector::parse("div#root[data-role=\"main\"] > span.hit").expect("selector parse");
        let matches = find_selector(&selector, &tree);
        assert_eq!(matches, vec![second]);
    }
}
