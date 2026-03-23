use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::DeclExtractor;

pub struct CppExtractor;

impl DeclExtractor for CppExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        extract_top_level(root, source, &mut imports, &mut declarations);

        (imports, declarations)
    }
}

fn extract_top_level(
    root: Node<'_>,
    source: &str,
    imports: &mut Vec<Import>,
    declarations: &mut Vec<Declaration>,
) {
    for i in 0..root.child_count() {
        let Some(child) = root.child(i) else {
            continue;
        };
        match child.kind() {
            "preproc_include" => {
                if let Some(import) = extract_include(child, source) {
                    imports.push(import);
                }
            }
            "using_declaration" => {
                if let Some(import) = extract_using(child, source) {
                    imports.push(import);
                }
            }
            "function_definition" => {
                if let Some(decl) = extract_function_definition(child, source) {
                    declarations.push(decl);
                }
            }
            "declaration" => {
                if let Some(decl) = extract_declaration(child, source) {
                    declarations.push(decl);
                }
            }
            "class_specifier" => {
                if let Some(decl) = extract_class(child, source, Visibility::Private) {
                    declarations.push(decl);
                }
            }
            "struct_specifier" => {
                if let Some(decl) = extract_class(child, source, Visibility::Public) {
                    declarations.push(decl);
                }
            }
            "enum_specifier" => {
                if let Some(decl) = extract_enum(child, source) {
                    declarations.push(decl);
                }
            }
            "namespace_definition" => {
                if let Some(decl) = extract_namespace(child, source) {
                    declarations.push(decl);
                }
            }
            "template_declaration" => {
                if let Some(decl) = extract_template(child, source) {
                    declarations.push(decl);
                }
            }
            "type_definition" => {
                if let Some(decl) = extract_typedef(child, source) {
                    declarations.push(decl);
                }
            }
            "preproc_def" => {
                if let Some(decl) = extract_preproc_def(child, source) {
                    declarations.push(decl);
                }
            }
            _ => {}
        }
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_visibility_from_text(node: Node<'_>, source: &str) -> Visibility {
    let text = node_text(node, source);
    if text.contains("static") {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
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
                    comments.push(cleaned);
                    break;
                } else if text.starts_with("/*") {
                    let cleaned = text
                        .trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string();
                    comments.push(cleaned);
                    break;
                } else if let Some(doc) = text.strip_prefix("///") {
                    comments.push(doc.trim().to_string());
                } else if let Some(doc) = text.strip_prefix("//") {
                    comments.push(doc.trim().to_string());
                } else {
                    break;
                }
            }
            _ => break,
        }
        prev = sibling.prev_sibling();
    }

    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join(" "))
    }
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

fn extract_include(node: Node<'_>, source: &str) -> Option<Import> {
    let path_node = node.child_by_field_name("path")?;
    let text = node_text(path_node, source).to_string();
    Some(Import { text })
}

fn extract_using(node: Node<'_>, source: &str) -> Option<Import> {
    let text = node_text(node, source).trim();
    let clean = text.trim_end_matches(';').trim();
    let clean = clean.strip_prefix("using ").unwrap_or(clean);
    Some(Import {
        text: clean.to_string(),
    })
}

/// Extract function name from a function_declarator by traversing nested declarators.
fn extract_function_name<'a>(declarator: Node<'_>, source: &'a str) -> Option<&'a str> {
    if declarator.kind() == "function_declarator" {
        if let Some(inner) = declarator.child_by_field_name("declarator") {
            if inner.kind() == "identifier" || inner.kind() == "qualified_identifier"
                || inner.kind() == "destructor_name" || inner.kind() == "operator_name"
                || inner.kind() == "field_identifier"
            {
                return Some(node_text(inner, source));
            }
            return extract_function_name(inner, source);
        }
    }
    if declarator.kind() == "pointer_declarator" || declarator.kind() == "reference_declarator" {
        if let Some(inner) = declarator.child_by_field_name("declarator") {
            return extract_function_name(inner, source);
        }
    }
    if declarator.kind() == "identifier" || declarator.kind() == "qualified_identifier"
        || declarator.kind() == "destructor_name" || declarator.kind() == "operator_name"
        || declarator.kind() == "field_identifier"
    {
        return Some(node_text(declarator, source));
    }
    None
}

fn extract_function_definition(node: Node<'_>, source: &str) -> Option<Declaration> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_function_name(declarator, source)?.to_string();
    let visibility = extract_visibility_from_text(node, source);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    Some(Declaration {
        kind: DeclKind::Function,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_declaration(node: Node<'_>, source: &str) -> Option<Declaration> {
    let declarator = node.child_by_field_name("declarator")?;

    if has_function_declarator(declarator) {
        let name = extract_function_name_from_decl(declarator, source)?.to_string();
        let visibility = extract_visibility_from_text(node, source);
        let doc_comment = extract_doc_comment(node, source);
        let signature = extract_signature(node, source);
        let line = node.start_position().row + 1;

        Some(Declaration {
            kind: DeclKind::Function,
            name,
            signature,
            visibility,
            line,
            doc_comment,
            children: Vec::new(),
        })
    } else {
        let name = extract_var_name(declarator, source)?.to_string();
        let visibility = extract_visibility_from_text(node, source);
        let doc_comment = extract_doc_comment(node, source);
        let signature = extract_signature(node, source);
        let line = node.start_position().row + 1;

        Some(Declaration {
            kind: DeclKind::Static,
            name,
            signature,
            visibility,
            line,
            doc_comment,
            children: Vec::new(),
        })
    }
}

fn has_function_declarator(node: Node<'_>) -> bool {
    if node.kind() == "function_declarator" {
        return true;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if has_function_declarator(child) {
                return true;
            }
        }
    }
    false
}

fn extract_function_name_from_decl<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    if node.kind() == "function_declarator" {
        return extract_function_name(node, source);
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(name) = extract_function_name_from_decl(child, source) {
                return Some(name);
            }
        }
    }
    None
}

fn extract_var_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    if node.kind() == "init_declarator" {
        if let Some(inner) = node.child_by_field_name("declarator") {
            return extract_var_name(inner, source);
        }
    }
    if node.kind() == "identifier" || node.kind() == "qualified_identifier" {
        return Some(node_text(node, source));
    }
    if node.kind() == "pointer_declarator" || node.kind() == "reference_declarator" {
        if let Some(inner) = node.child_by_field_name("declarator") {
            return extract_var_name(inner, source);
        }
    }
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(node_text(name_node, source));
    }
    None
}

fn extract_class(
    node: Node<'_>,
    source: &str,
    default_visibility: Visibility,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let is_struct = matches!(default_visibility, Visibility::Public);
    let kind = DeclKind::Struct;

    let mut children = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut current_visibility = default_visibility.clone();

        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            match child.kind() {
                "access_specifier" => {
                    let spec_text = node_text(child, source);
                    if spec_text.contains("public") {
                        current_visibility = Visibility::Public;
                    } else if spec_text.contains("private") {
                        current_visibility = Visibility::Private;
                    } else if spec_text.contains("protected") {
                        current_visibility = Visibility::Private;
                    }
                }
                "function_definition" => {
                    if let Some(mut decl) = extract_function_definition(child, source) {
                        decl.kind = DeclKind::Method;
                        decl.visibility = current_visibility.clone();
                        children.push(decl);
                    }
                }
                "declaration" => {
                    if let Some(mut decl) = extract_class_member_declaration(child, source) {
                        decl.visibility = current_visibility.clone();
                        children.push(decl);
                    }
                }
                "field_declaration" => {
                    if let Some(mut decl) = extract_class_field(child, source) {
                        decl.visibility = current_visibility.clone();
                        children.push(decl);
                    }
                }
                "class_specifier" => {
                    if let Some(decl) = extract_class(child, source, Visibility::Private) {
                        children.push(decl);
                    }
                }
                "struct_specifier" => {
                    if let Some(decl) = extract_class(child, source, Visibility::Public) {
                        children.push(decl);
                    }
                }
                "enum_specifier" => {
                    if let Some(decl) = extract_enum(child, source) {
                        children.push(decl);
                    }
                }
                "template_declaration" => {
                    if let Some(mut decl) = extract_template(child, source) {
                        decl.visibility = current_visibility.clone();
                        children.push(decl);
                    }
                }
                _ => {}
            }
        }
    }

    let _ = is_struct;

    Some(Declaration {
        kind,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children,
    })
}

fn extract_class_member_declaration(node: Node<'_>, source: &str) -> Option<Declaration> {
    // Inside a class body, a declaration can be a method declaration or a field
    let declarator = node.child_by_field_name("declarator")?;

    if has_function_declarator(declarator) {
        let name = extract_function_name_from_decl(declarator, source)?.to_string();
        let doc_comment = extract_doc_comment(node, source);
        let signature = extract_signature(node, source);
        let line = node.start_position().row + 1;

        Some(Declaration {
            kind: DeclKind::Method,
            name,
            signature,
            visibility: Visibility::Public,
            line,
            doc_comment,
            children: Vec::new(),
        })
    } else {
        let name = extract_var_name(declarator, source)?.to_string();
        let line = node.start_position().row + 1;
        let signature = extract_signature(node, source);

        Some(Declaration {
            kind: DeclKind::Field,
            name,
            signature,
            visibility: Visibility::Public,
            line,
            doc_comment: None,
            children: Vec::new(),
        })
    }
}

fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let declarator = node.child_by_field_name("declarator")?;

    // Check if it is a function declaration (method prototype)
    if has_function_declarator(declarator) {
        let name = extract_function_name_from_decl(declarator, source)?.to_string();
        let doc_comment = extract_doc_comment(node, source);
        let signature = extract_signature(node, source);
        let line = node.start_position().row + 1;

        return Some(Declaration {
            kind: DeclKind::Method,
            name,
            signature,
            visibility: Visibility::Public,
            line,
            doc_comment,
            children: Vec::new(),
        });
    }

    let name = extract_var_name(declarator, source)?.to_string();
    let line = node.start_position().row + 1;

    let type_text = node
        .child_by_field_name("type")
        .map(|t| node_text(t, source))
        .unwrap_or("");

    let signature = format!("{} {}", type_text, name);
    let signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Field,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
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
            if let Some(child) = body.child(i) {
                if child.kind() == "enumerator" {
                    if let Some(variant) = extract_enumerator(child, source) {
                        variants.push(variant);
                    }
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Enum,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: variants,
    })
}

fn extract_enumerator(node: Node<'_>, source: &str) -> Option<Declaration> {
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

fn extract_namespace(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut child_imports = Vec::new();
    let mut child_decls = Vec::new();

    if let Some(body) = node.child_by_field_name("body") {
        extract_top_level(body, source, &mut child_imports, &mut child_decls);
    }

    Some(Declaration {
        kind: DeclKind::Module,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: child_decls,
    })
}

fn extract_template(node: Node<'_>, source: &str) -> Option<Declaration> {
    // A template_declaration wraps another declaration (class, function, etc.)
    // Find the inner declaration
    let inner = node.child_by_field_name("declaration")?;
    let template_params = node.child_by_field_name("parameters");
    let template_prefix = if let Some(params) = template_params {
        format!("template{} ", node_text(params, source))
    } else {
        "template ".to_string()
    };

    match inner.kind() {
        "function_definition" => {
            let mut decl = extract_function_definition(inner, source)?;
            decl.signature = format!("{}{}", template_prefix, decl.signature);
            decl.signature = decl.signature.split_whitespace().collect::<Vec<_>>().join(" ");
            decl.doc_comment = extract_doc_comment(node, source).or(decl.doc_comment);
            decl.line = node.start_position().row + 1;
            Some(decl)
        }
        "declaration" => {
            let mut decl = extract_declaration(inner, source)?;
            decl.signature = format!("{}{}", template_prefix, decl.signature);
            decl.signature = decl.signature.split_whitespace().collect::<Vec<_>>().join(" ");
            decl.doc_comment = extract_doc_comment(node, source).or(decl.doc_comment);
            decl.line = node.start_position().row + 1;
            Some(decl)
        }
        "class_specifier" => {
            let mut decl = extract_class(inner, source, Visibility::Private)?;
            decl.signature = format!("{}{}", template_prefix, decl.signature);
            decl.signature = decl.signature.split_whitespace().collect::<Vec<_>>().join(" ");
            decl.doc_comment = extract_doc_comment(node, source).or(decl.doc_comment);
            decl.line = node.start_position().row + 1;
            Some(decl)
        }
        "struct_specifier" => {
            let mut decl = extract_class(inner, source, Visibility::Public)?;
            decl.signature = format!("{}{}", template_prefix, decl.signature);
            decl.signature = decl.signature.split_whitespace().collect::<Vec<_>>().join(" ");
            decl.doc_comment = extract_doc_comment(node, source).or(decl.doc_comment);
            decl.line = node.start_position().row + 1;
            Some(decl)
        }
        _ => {
            // Fallback: extract the whole template as a generic declaration
            let doc_comment = extract_doc_comment(node, source);
            let signature = extract_signature(node, source);
            let line = node.start_position().row + 1;

            // Try to get a name from the inner node
            let name = inner
                .child_by_field_name("name")
                .map(|n| node_text(n, source).to_string())
                .unwrap_or_else(|| node_text(inner, source).split_whitespace().next().unwrap_or("unknown").to_string());

            Some(Declaration {
                kind: DeclKind::Function,
                name,
                signature,
                visibility: Visibility::Public,
                line,
                doc_comment,
                children: Vec::new(),
            })
        }
    }
}

fn extract_typedef(node: Node<'_>, source: &str) -> Option<Declaration> {
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;
    let signature = extract_signature(node, source);

    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_var_name(declarator, source)
        .unwrap_or_else(|| node_text(declarator, source))
        .to_string();

    Some(Declaration {
        kind: DeclKind::TypeAlias,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_preproc_def(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let doc_comment = extract_doc_comment(node, source);
    let signature = node_text(node, source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Constant,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: Vec::new(),
    })
}
