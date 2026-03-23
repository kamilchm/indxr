use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility, Relationship, RelKind};

use super::DeclExtractor;

pub struct JavaScriptExtractor;

impl DeclExtractor for JavaScriptExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        for i in 0..root.child_count() {
            let Some(child) = root.child(i) else {
                continue;
            };
            process_top_level_node(child, source, &mut imports, &mut declarations, false);
        }

        (imports, declarations)
    }
}

fn process_top_level_node(
    node: Node<'_>,
    source: &str,
    imports: &mut Vec<Import>,
    declarations: &mut Vec<Declaration>,
    is_exported: bool,
) {
    match node.kind() {
        "import_statement" => {
            if let Some(import) = extract_import(node, source) {
                imports.push(import);
            }
        }
        "export_statement" => {
            // An export_statement wraps a declaration; extract the inner declaration
            // and mark it as Public.
            for i in 0..node.child_count() {
                let Some(child) = node.child(i) else {
                    continue;
                };
                match child.kind() {
                    "function_declaration"
                    | "class_declaration"
                    | "lexical_declaration"
                    | "variable_declaration" => {
                        process_top_level_node(child, source, imports, declarations, true);
                    }
                    _ => {}
                }
            }
        }
        "function_declaration" => {
            if let Some(mut decl) = extract_function(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "class_declaration" => {
            if let Some(mut decl) = extract_class(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "lexical_declaration" => {
            for decl in extract_lexical_declaration(node, source) {
                let mut decl = decl;
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "variable_declaration" => {
            for decl in extract_variable_declaration(node, source) {
                let mut decl = decl;
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        _ => {}
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("/**") {
                    let cleaned = text
                        .trim_start_matches("/**")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string();
                    return Some(cleaned);
                }
                return None;
            }
            _ => break,
        }
    }

    // Also check if the node's parent is an export_statement and the
    // doc comment is before the export_statement.
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            prev = parent.prev_sibling();
            while let Some(sibling) = prev {
                match sibling.kind() {
                    "comment" => {
                        let text = node_text(sibling, source);
                        if text.starts_with("/**") {
                            let cleaned = text
                                .trim_start_matches("/**")
                                .trim_end_matches("*/")
                                .trim()
                                .to_string();
                            return Some(cleaned);
                        }
                        return None;
                    }
                    _ => break,
                }
            }
        }
    }

    None
}

/// Get the raw doc comment text (including /** */) for metadata checks.
fn get_raw_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("/**") {
                    return Some(text.to_string());
                }
                return None;
            }
            _ => break,
        }
    }

    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            prev = parent.prev_sibling();
            while let Some(sibling) = prev {
                match sibling.kind() {
                    "comment" => {
                        let text = node_text(sibling, source);
                        if text.starts_with("/**") {
                            return Some(text.to_string());
                        }
                        return None;
                    }
                    _ => break,
                }
            }
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
    Some(Import {
        text: clean.to_string(),
    })
}

fn is_test_name(name: &str) -> bool {
    name == "describe" || name == "it" || name == "test" || name.starts_with("test")
}

fn extract_class_relationships(node: Node<'_>, source: &str) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let sig = extract_signature(node, source);

    // Check for "extends Foo" (JavaScript has no implements)
    if let Some(pos) = sig.find("extends ") {
        let after = &sig[pos + 8..];
        let target = after.split_whitespace().next().unwrap_or("")
            .trim_end_matches('{')
            .trim_end_matches(',');
        if !target.is_empty() {
            relationships.push(Relationship {
                kind: RelKind::Extends,
                target: target.to_string(),
            });
        }
    }

    relationships
}

fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().map_or(false, |d| d.contains("@deprecated"));
    let is_async = signature.contains("async");
    let is_test = is_test_name(&name);
    let body_lines = Some(node.end_position().row.saturating_sub(node.start_position().row));

    let mut decl = Declaration::new(DeclKind::Function, name, signature, Visibility::Private, line);
    decl.doc_comment = doc_comment;
    decl.is_async = is_async;
    decl.is_test = is_test;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
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
                "method_definition" => {
                    if let Some(method) = extract_method(child, source) {
                        children.push(method);
                    }
                }
                "field_definition" => {
                    if let Some(field) = extract_class_field(child, source) {
                        children.push(field);
                    }
                }
                _ => {}
            }
        }
    }

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().map_or(false, |d| d.contains("@deprecated"));
    let body_lines = Some(node.end_position().row.saturating_sub(node.start_position().row));
    let relationships = extract_class_relationships(node, source);

    let mut decl = Declaration::new(DeclKind::Struct, name, signature, Visibility::Private, line);
    decl.doc_comment = doc_comment;
    decl.children = children;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    decl.relationships = relationships;
    Some(decl)
}

fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().map_or(false, |d| d.contains("@deprecated"));
    let is_async = signature.contains("async");
    let is_test = is_test_name(&name);
    let body_lines = Some(node.end_position().row.saturating_sub(node.start_position().row));

    let mut decl = Declaration::new(DeclKind::Method, name, signature, Visibility::Public, line);
    decl.doc_comment = doc_comment;
    decl.is_async = is_async;
    decl.is_test = is_test;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    // In tree-sitter-javascript, field_definition uses "property" for the name
    let name_node = node.child_by_field_name("property")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim();
    let signature = text
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let body_lines = Some(node.end_position().row.saturating_sub(node.start_position().row));

    let mut decl = Declaration::new(DeclKind::Field, name, signature, Visibility::Public, line);
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_lexical_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    // Determine if this is const, let, or var
    let mut is_const = false;
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let text = node_text(child, source);
            if text == "const" {
                is_const = true;
                break;
            }
        }
    }

    // Only extract top-level const declarations
    if !is_const {
        return declarations;
    }

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "variable_declarator" {
            if let Some(decl) = extract_variable_declarator(child, node, source) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_variable_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "variable_declarator" {
            if let Some(decl) = extract_variable_declarator(child, node, source) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_variable_declarator(
    node: Node<'_>,
    parent: Node<'_>,
    source: &str,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(parent, source);
    let line = parent.start_position().row + 1;
    let signature = extract_signature(parent, source);

    let raw_doc = get_raw_doc_comment(parent, source);
    let is_deprecated = raw_doc.as_ref().map_or(false, |d| d.contains("@deprecated"));
    let body_lines = Some(parent.end_position().row.saturating_sub(parent.start_position().row));

    let mut decl = Declaration::new(DeclKind::Constant, name, signature, Visibility::Private, line);
    decl.doc_comment = doc_comment;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}
