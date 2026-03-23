use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::DeclExtractor;

pub struct JavaExtractor;

impl DeclExtractor for JavaExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        for i in 0..root.child_count() {
            let Some(child) = root.child(i) else {
                continue;
            };
            match child.kind() {
                "import_declaration" => {
                    if let Some(import) = extract_import(child, source) {
                        imports.push(import);
                    }
                }
                "class_declaration" => {
                    if let Some(decl) = extract_class(child, source) {
                        declarations.push(decl);
                    }
                }
                "interface_declaration" => {
                    if let Some(decl) = extract_interface(child, source) {
                        declarations.push(decl);
                    }
                }
                "enum_declaration" => {
                    if let Some(decl) = extract_enum(child, source) {
                        declarations.push(decl);
                    }
                }
                _ => {}
            }
        }

        (imports, declarations)
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_modifiers_text(node: Node<'_>, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "modifiers" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn extract_visibility(node: Node<'_>, source: &str) -> Visibility {
    if let Some(mods) = extract_modifiers_text(node, source) {
        if mods.contains("public") {
            return Visibility::Public;
        }
        if mods.contains("private") || mods.contains("protected") {
            return Visibility::Private;
        }
    }
    Visibility::Private
}

#[allow(dead_code)]
fn has_modifier(node: Node<'_>, source: &str, keyword: &str) -> bool {
    if let Some(mods) = extract_modifiers_text(node, source) {
        mods.contains(keyword)
    } else {
        false
    }
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "block_comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("/**") {
                    let cleaned = text
                        .trim_start_matches("/**")
                        .trim_end_matches("*/")
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    return Some(cleaned);
                }
                return None;
            }
            "line_comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("//") {
                    // Regular line comment, not Javadoc — skip
                }
                return None;
            }
            "marker_annotation" | "annotation" => {
                // Skip annotations like @Override, @Deprecated
                prev = sibling.prev_sibling();
                continue;
            }
            _ => return None,
        }
    }

    None
}

fn extract_signature(node: Node<'_>, source: &str) -> String {
    let text = node_text(node, source);
    let end = text
        .find('{')
        .or_else(|| text.find(';'))
        .unwrap_or(text.len());
    let sig = text[..end].trim();
    sig.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_import(node: Node<'_>, source: &str) -> Option<Import> {
    let text = node_text(node, source).trim();
    let clean = text.trim_end_matches(';').trim();
    let clean = clean.strip_prefix("import ").unwrap_or(clean);
    let clean = clean.strip_prefix("static ").unwrap_or(clean);
    Some(Import {
        text: clean.to_string(),
    })
}

fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut children = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            match child.kind() {
                "method_declaration" => {
                    if let Some(decl) = extract_method(child, source) {
                        children.push(decl);
                    }
                }
                "constructor_declaration" => {
                    if let Some(decl) = extract_constructor(child, source) {
                        children.push(decl);
                    }
                }
                "field_declaration" => {
                    if let Some(decl) = extract_field(child, source) {
                        children.push(decl);
                    }
                }
                "class_declaration" => {
                    if let Some(decl) = extract_class(child, source) {
                        children.push(decl);
                    }
                }
                "enum_declaration" => {
                    if let Some(decl) = extract_enum(child, source) {
                        children.push(decl);
                    }
                }
                "interface_declaration" => {
                    if let Some(decl) = extract_interface(child, source) {
                        children.push(decl);
                    }
                }
                _ => {}
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Struct,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children,
    })
}

fn extract_interface(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            if child.kind() == "method_declaration" {
                if let Some(decl) = extract_method(child, source) {
                    methods.push(decl);
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Trait,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: methods,
    })
}

fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut variants = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            if child.kind() == "enum_constant" {
                if let Some(variant) = extract_enum_constant(child, source) {
                    variants.push(variant);
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Enum,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: variants,
    })
}

fn extract_enum_constant(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim().trim_end_matches(',');
    let signature = text.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Variant,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;

    let mut sig_parts = Vec::new();
    if let Some(mods) = extract_modifiers_text(node, source) {
        sig_parts.push(mods);
    }
    if let Some(ret_type) = node.child_by_field_name("type") {
        sig_parts.push(node_text(ret_type, source).to_string());
    }
    sig_parts.push(name.clone());
    if let Some(params) = node.child_by_field_name("parameters") {
        sig_parts.push(node_text(params, source).to_string());
    }
    let signature = sig_parts.join(" ");
    let signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Method,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_constructor(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;

    let mut sig_parts = Vec::new();
    if let Some(mods) = extract_modifiers_text(node, source) {
        sig_parts.push(mods);
    }
    sig_parts.push(name.clone());
    if let Some(params) = node.child_by_field_name("parameters") {
        sig_parts.push(node_text(params, source).to_string());
    }
    let signature = sig_parts.join(" ");
    let signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Method,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let type_node = node.child_by_field_name("type");
    let type_text = type_node
        .map(|t| node_text(t, source))
        .unwrap_or("");

    // The field name is in the declarator child
    let declarator = node.child_by_field_name("declarator")?;
    let name_node = declarator.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();

    let visibility = extract_visibility(node, source);
    let line = node.start_position().row + 1;

    let mut sig_parts = Vec::new();
    if let Some(mods) = extract_modifiers_text(node, source) {
        sig_parts.push(mods);
    }
    sig_parts.push(type_text.to_string());
    sig_parts.push(name.clone());
    let signature = sig_parts.join(" ");
    let signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Field,
        name,
        signature,
        visibility,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}
