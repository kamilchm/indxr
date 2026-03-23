use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, RelKind, Relationship, Visibility};

use super::DeclExtractor;

pub struct TypeScriptExtractor;

impl DeclExtractor for TypeScriptExtractor {
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
                    | "abstract_class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "lexical_declaration" => {
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
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(mut decl) = extract_class(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "interface_declaration" => {
            if let Some(mut decl) = extract_interface(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "type_alias_declaration" => {
            if let Some(mut decl) = extract_type_alias(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "enum_declaration" => {
            if let Some(mut decl) = extract_enum(node, source) {
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
        _ => {}
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    if let Some(sibling) = prev
        && sibling.kind() == "comment"
    {
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

    // Also check if the node's parent is an export_statement and the
    // doc comment is before the export_statement.
    if let Some(parent) = node.parent()
        && parent.kind() == "export_statement"
    {
        prev = parent.prev_sibling();
        if let Some(sibling) = prev
            && sibling.kind() == "comment"
        {
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
    }

    None
}

/// Get the raw doc comment text (including /** */) for metadata checks.
fn get_raw_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    if let Some(sibling) = prev
        && sibling.kind() == "comment"
    {
        let text = node_text(sibling, source);
        if text.starts_with("/**") {
            return Some(text.to_string());
        }
        return None;
    }

    if let Some(parent) = node.parent()
        && parent.kind() == "export_statement"
    {
        prev = parent.prev_sibling();
        if let Some(sibling) = prev
            && sibling.kind() == "comment"
        {
            let text = node_text(sibling, source);
            if text.starts_with("/**") {
                return Some(text.to_string());
            }
            return None;
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

    // Check for "extends Foo"
    if let Some(pos) = sig.find("extends ") {
        let after = &sig[pos + 8..];
        let target = after
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('{')
            .trim_end_matches(',');
        if !target.is_empty() {
            relationships.push(Relationship {
                kind: RelKind::Extends,
                target: target.to_string(),
            });
        }
    }

    // Check for "implements Bar, Baz"
    if let Some(pos) = sig.find("implements ") {
        let after = &sig[pos + 11..];
        // Everything until '{' or end
        let end = after.find('{').unwrap_or(after.len());
        let implements_str = after[..end].trim();
        for target in implements_str.split(',') {
            let target = target.trim();
            if !target.is_empty() {
                relationships.push(Relationship {
                    kind: RelKind::Implements,
                    target: target.to_string(),
                });
            }
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
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let is_async = signature.contains("async");
    let is_test = is_test_name(&name);
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(
        DeclKind::Function,
        name,
        signature,
        Visibility::Private,
        line,
    );
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
                "public_field_definition" => {
                    if let Some(field) = extract_class_field(child, source) {
                        children.push(field);
                    }
                }
                _ => {}
            }
        }
    }

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );
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
    let visibility = extract_member_visibility(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let is_async = signature.contains("async");
    let is_test = is_test_name(&name);
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Method, name, signature, visibility, line);
    decl.doc_comment = doc_comment;
    decl.is_async = is_async;
    decl.is_test = is_test;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_member_visibility(node, source);
    let line = node.start_position().row + 1;

    let type_text = node
        .child_by_field_name("type")
        .map(|t| node_text(t, source))
        .unwrap_or("");

    // type_annotation includes the ": " prefix, strip it
    let type_text = type_text.trim_start_matches(':').trim();

    let signature = if type_text.is_empty() {
        name.clone()
    } else {
        format!("{}: {}", name, type_text)
    };

    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Field, name, signature, visibility, line);
    decl.body_lines = body_lines;
    Some(decl)
}

/// Check for accessibility_modifier (public/private/protected) on class members.
fn extract_member_visibility(node: Node<'_>, source: &str) -> Visibility {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i)
            && child.kind() == "accessibility_modifier"
        {
            let text = node_text(child, source);
            match text {
                "public" => return Visibility::Public,
                "private" | "protected" => return Visibility::Private,
                _ => {}
            }
        }
    }
    // Default for class members: Public (they are accessible by default in JS/TS)
    Visibility::Public
}

fn extract_interface(node: Node<'_>, source: &str) -> Option<Declaration> {
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
                "method_signature" | "function_signature" => {
                    if let Some(method) = extract_interface_method(child, source) {
                        children.push(method);
                    }
                }
                "property_signature" => {
                    if let Some(field) = extract_property_signature(child, source) {
                        children.push(field);
                    }
                }
                _ => {}
            }
        }
    }

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    // Interfaces can extend other interfaces
    let mut relationships = Vec::new();
    if let Some(pos) = signature.find("extends ") {
        let after = &signature[pos + 8..];
        let end = after.find('{').unwrap_or(after.len());
        let extends_str = after[..end].trim();
        for target in extends_str.split(',') {
            let target = target.trim();
            if !target.is_empty() {
                relationships.push(Relationship {
                    kind: RelKind::Extends,
                    target: target.to_string(),
                });
            }
        }
    }

    let mut decl = Declaration::new(DeclKind::Trait, name, signature, Visibility::Private, line);
    decl.doc_comment = doc_comment;
    decl.children = children;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    decl.relationships = relationships;
    Some(decl)
}

fn extract_interface_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim();
    let signature = text
        .trim_end_matches(';')
        .trim_end_matches(',')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let is_async = signature.contains("async");
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Method, name, signature, Visibility::Public, line);
    decl.is_async = is_async;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_property_signature(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim();
    let signature = text
        .trim_end_matches(';')
        .trim_end_matches(',')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Field, name, signature, Visibility::Public, line);
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_type_alias(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;
    let signature = extract_signature(node, source);

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(
        DeclKind::TypeAlias,
        name,
        signature,
        Visibility::Private,
        line,
    );
    decl.doc_comment = doc_comment;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut variants = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            // enum_assignment is the node type for enum members in tree-sitter-typescript
            if (child.kind() == "enum_assignment" || child.kind() == "property_identifier")
                && let Some(variant) = extract_enum_member(child, source)
            {
                variants.push(variant);
            }
        }
    }

    let raw_doc = get_raw_doc_comment(node, source);
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Enum, name, signature, Visibility::Private, line);
    decl.doc_comment = doc_comment;
    decl.children = variants;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_enum_member(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        // If it's a bare property_identifier, the node itself is the name
        if node.kind() == "property_identifier" {
            Some(node)
        } else {
            None
        }
    })?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim().trim_end_matches(',');
    let signature = text.split_whitespace().collect::<Vec<_>>().join(" ");

    let body_lines = Some(
        node.end_position()
            .row
            .saturating_sub(node.start_position().row),
    );

    let mut decl = Declaration::new(DeclKind::Variant, name, signature, Visibility::Public, line);
    decl.body_lines = body_lines;
    Some(decl)
}

fn extract_lexical_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    // Determine if this is const, let, or var
    let mut decl_keyword = "const";
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let text = node_text(child, source);
            if text == "const" || text == "let" || text == "var" {
                decl_keyword = if text == "const" { "const" } else { "let" };
                break;
            }
        }
    }

    // Only extract top-level const declarations
    if decl_keyword != "const" {
        return declarations;
    }

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "variable_declarator"
            && let Some(decl) = extract_variable_declarator(child, node, source)
        {
            declarations.push(decl);
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
    let is_deprecated = raw_doc.as_ref().is_some_and(|d| d.contains("@deprecated"));
    let body_lines = Some(
        parent
            .end_position()
            .row
            .saturating_sub(parent.start_position().row),
    );

    let mut decl = Declaration::new(
        DeclKind::Constant,
        name,
        signature,
        Visibility::Private,
        line,
    );
    decl.doc_comment = doc_comment;
    decl.is_deprecated = is_deprecated;
    decl.body_lines = body_lines;
    Some(decl)
}
