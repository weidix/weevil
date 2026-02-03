use super::{HtmlTree, NodeId, NodeKind};

impl HtmlTree {
    /// Returns the HTML contents of a node's children.
    pub fn html(&self, id: NodeId) -> String {
        self.inner_html(id)
    }

    /// Returns the HTML contents of a node's children.
    pub fn inner_html(&self, id: NodeId) -> String {
        let mut output = String::new();
        match self.node(id).kind() {
            NodeKind::Document | NodeKind::Element(_) => {
                for child in self.children(id) {
                    write_node(self, child, &mut output);
                }
            }
            _ => {}
        }
        output
    }

    /// Returns the HTML for a node and its descendants.
    pub fn outer_html(&self, id: NodeId) -> String {
        let mut output = String::new();
        write_node(self, id, &mut output);
        output
    }

    /// Returns the concatenated text content under a node.
    pub fn text(&self, id: NodeId) -> String {
        self.text_content(id)
    }
}

fn write_node(tree: &HtmlTree, id: NodeId, output: &mut String) {
    match tree.node(id).kind() {
        NodeKind::Document => {
            for child in tree.children(id) {
                write_node(tree, child, output);
            }
        }
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => {
            output.push_str("<!DOCTYPE");
            let name_str = name.as_ref();
            if !name_str.is_empty() {
                output.push(' ');
                output.push_str(name_str);
            }
            let public_str = public_id.as_ref();
            let system_str = system_id.as_ref();
            if !public_str.is_empty() || !system_str.is_empty() {
                if !public_str.is_empty() {
                    output.push_str(" PUBLIC \"");
                    escape_attr(public_str, output);
                    output.push('"');
                    if !system_str.is_empty() {
                        output.push(' ');
                        output.push('"');
                        escape_attr(system_str, output);
                        output.push('"');
                    }
                } else {
                    output.push_str(" SYSTEM \"");
                    escape_attr(system_str, output);
                    output.push('"');
                }
            }
            output.push('>');
        }
        NodeKind::Text(text) => escape_text(text.as_ref(), output),
        NodeKind::Comment(text) => {
            output.push_str("<!--");
            output.push_str(text.as_ref());
            output.push_str("-->");
        }
        NodeKind::Element(data) => {
            output.push('<');
            write_qual_name(data.name.prefix.as_ref(), data.name.local.as_ref(), output);
            for attr in &data.attrs {
                output.push(' ');
                write_qual_name(attr.name.prefix.as_ref(), attr.name.local.as_ref(), output);
                output.push_str("=\"");
                escape_attr(attr.value.as_ref(), output);
                output.push('"');
            }
            output.push('>');
            if !is_void_element(data.name.local.as_ref()) {
                for child in tree.children(id) {
                    write_node(tree, child, output);
                }
                output.push_str("</");
                write_qual_name(data.name.prefix.as_ref(), data.name.local.as_ref(), output);
                output.push('>');
            }
        }
        NodeKind::ProcessingInstruction { target, data } => {
            output.push_str("<?");
            output.push_str(target.as_ref());
            let data_str = data.as_ref();
            if !data_str.is_empty() {
                output.push(' ');
                output.push_str(data_str);
            }
            output.push_str("?>");
        }
    }
}

fn write_qual_name(prefix: Option<&html5ever::Prefix>, local: &str, output: &mut String) {
    if let Some(prefix) = prefix {
        output.push_str(prefix.as_ref());
        output.push(':');
    }
    output.push_str(local);
}

fn escape_text(input: &str, output: &mut String) {
    for ch in input.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(ch),
        }
    }
}

fn escape_attr(input: &str, output: &mut String) {
    for ch in input.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(ch),
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}
