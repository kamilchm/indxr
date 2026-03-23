use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::languages::Language;
use crate::model::declarations::{DeclKind, Declaration, Visibility};
use crate::model::{FileIndex, Import};

use super::LanguageParser;

pub struct RegexParser {
    language: Language,
}

impl RegexParser {
    pub fn new(language: Language) -> Self {
        Self { language }
    }
}

impl LanguageParser for RegexParser {
    fn language(&self) -> Language {
        self.language.clone()
    }

    fn parse_file(&self, path: &Path, content: &str) -> Result<FileIndex> {
        let lines = content.lines().count();
        let (imports, declarations) = match self.language {
            Language::Shell => parse_shell(content),
            Language::Toml => parse_toml(path, content),
            Language::Yaml => parse_yaml(path, content),
            Language::Json => parse_json(path, content),
            Language::Sql => parse_sql(content),
            Language::Markdown => parse_markdown(content),
            Language::Protobuf => parse_protobuf(content),
            Language::GraphQL => parse_graphql(content),
            _ => (Vec::new(), Vec::new()),
        };

        Ok(FileIndex {
            path: path.to_path_buf(),
            language: self.language.clone(),
            size: content.len() as u64,
            lines,
            imports,
            declarations,
        })
    }
}

// ---------------------------------------------------------------------------
// Shell parser
// ---------------------------------------------------------------------------

fn parse_shell(content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let mut imports = Vec::new();
    let mut declarations = Vec::new();

    let re_func1 = Regex::new(r"^function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)").unwrap();
    let re_func2 = Regex::new(r"^function\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s*\{|\s*$)").unwrap();
    let re_func3 = Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)\s*\{?").unwrap();
    let re_export = Regex::new(r"^export\s+([A-Za-z_][A-Za-z0-9_]*)=(.*)").unwrap();
    let re_alias = Regex::new(r"^alias\s+([A-Za-z_][A-Za-z0-9_\-]*)=").unwrap();
    let re_source = Regex::new(r"^(?:source|\.\s+)\s+(.+)$").unwrap();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Source / dot-import
        if let Some(caps) = re_source.captures(trimmed) {
            imports.push(Import {
                text: caps[1].trim().trim_matches('"').trim_matches('\'').to_string(),
            });
            continue;
        }

        // function name() { ... }
        if let Some(caps) = re_func1.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::ShellFunction,
                name.clone(),
                format!("function {}()", name),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // function name { ... }
        if let Some(caps) = re_func2.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::ShellFunction,
                name.clone(),
                format!("function {}", name),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // name() { ... }  — but not things like echo() which are unlikely, still parse them
        if let Some(caps) = re_func3.captures(trimmed) {
            let name = caps[1].to_string();
            // Skip common keywords that look like function calls
            if matches!(
                name.as_str(),
                "if" | "for" | "while" | "until" | "case" | "do" | "then" | "else" | "elif"
                    | "fi" | "done" | "esac" | "export" | "alias" | "source" | "echo"
                    | "return" | "exit" | "set" | "unset" | "local" | "readonly"
            ) {
                continue;
            }
            declarations.push(Declaration::new(
                DeclKind::ShellFunction,
                name.clone(),
                format!("{}()", name),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // export VAR=value
        if let Some(caps) = re_export.captures(trimmed) {
            let name = caps[1].to_string();
            let value = caps[2].to_string();
            declarations.push(Declaration::new(
                DeclKind::Constant,
                name.clone(),
                format!("export {}={}", name, truncate_value(&value, 60)),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // alias name=...
        if let Some(caps) = re_alias.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::Constant,
                name.clone(),
                format!("alias {}", name),
                Visibility::Public,
                line_num + 1,
            ));
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// TOML parser
// ---------------------------------------------------------------------------

fn parse_toml(path: &Path, content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let mut imports = Vec::new();
    let mut declarations = Vec::new();

    let re_section = Regex::new(r"^\[([^\]]+)\]").unwrap();
    let re_array_section = Regex::new(r"^\[\[([^\]]+)\]\]").unwrap();
    let re_kv = Regex::new(r#"^([A-Za-z_][A-Za-z0-9_\-]*)\s*="#).unwrap();

    let is_cargo = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "Cargo.toml")
        .unwrap_or(false);

    let mut current_section: Option<(String, usize)> = None; // (name, decl_index)
    let mut in_dependencies = false;

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // [[array.section]]
        if let Some(caps) = re_array_section.captures(trimmed) {
            let name = caps[1].to_string();
            in_dependencies = false;
            let decl = Declaration::new(
                DeclKind::ConfigKey,
                name.clone(),
                format!("[[{}]]", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            current_section = Some((name, idx));
            continue;
        }

        // [section]
        if let Some(caps) = re_section.captures(trimmed) {
            let name = caps[1].to_string();
            in_dependencies = is_cargo
                && (name == "dependencies"
                    || name == "dev-dependencies"
                    || name == "build-dependencies"
                    || name.starts_with("dependencies.")
                    || name.starts_with("dev-dependencies.")
                    || name.starts_with("build-dependencies."));
            let decl = Declaration::new(
                DeclKind::ConfigKey,
                name.clone(),
                format!("[{}]", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            current_section = Some((name, idx));
            continue;
        }

        // key = value
        if let Some(caps) = re_kv.captures(trimmed) {
            let key = caps[1].to_string();

            if is_cargo && in_dependencies {
                imports.push(Import {
                    text: key.clone(),
                });
            }

            let child = Declaration::new(
                DeclKind::ConfigKey,
                key.clone(),
                trimmed.to_string(),
                Visibility::Private,
                line_num + 1,
            );

            if let Some((_, parent_idx)) = &current_section {
                declarations[*parent_idx].children.push(child);
            } else {
                // Top-level key with no section
                declarations.push(child);
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// YAML parser
// ---------------------------------------------------------------------------

fn parse_yaml(path: &Path, content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let imports = Vec::new();
    let mut declarations = Vec::new();

    let is_docker_compose = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("docker-compose"))
        .unwrap_or(false);

    let re_top_key = Regex::new(r"^([A-Za-z_][A-Za-z0-9_\-]*)\s*:").unwrap();
    let re_child_key = Regex::new(r"^(\s{2,})([A-Za-z_][A-Za-z0-9_\-]*)\s*:").unwrap();

    let mut current_top: Option<usize> = None;
    let mut in_services = false;

    for (line_num, line) in content.lines().enumerate() {
        // Skip comments and blank lines
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Top-level key (no leading whitespace)
        if let Some(caps) = re_top_key.captures(line) {
            let name = caps[1].to_string();
            in_services = is_docker_compose && name == "services";

            let decl = Declaration::new(
                DeclKind::ConfigKey,
                name.clone(),
                format!("{}:", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            current_top = Some(idx);
            continue;
        }

        // Indented child key (services children in docker-compose, or general children)
        if is_docker_compose && in_services {
            if let Some(caps) = re_child_key.captures(line) {
                let indent: &str = &caps[1];
                let name = caps[2].to_string();

                // 2-space indent = direct child of services
                if indent.len() <= 4 {
                    if let Some(parent_idx) = current_top {
                        let child = Declaration::new(
                            DeclKind::ConfigKey,
                            name.clone(),
                            format!("service: {}", name),
                            Visibility::Private,
                            line_num + 1,
                        );
                        declarations[parent_idx].children.push(child);
                    }
                }
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// JSON parser
// ---------------------------------------------------------------------------

fn parse_json(path: &Path, content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let mut imports = Vec::new();
    let mut declarations = Vec::new();

    let is_package_json = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "package.json")
        .unwrap_or(false);

    // Simple line-by-line JSON parsing: detect top-level keys by tracking brace depth.
    let re_key = Regex::new(r#"^\s*"([^"]+)"\s*:"#).unwrap();

    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut current_top: Option<(String, usize)> = None;
    let mut in_deps = false;

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Count brace/bracket depth changes before this line's key
        // We need the depth at the start of this line, so count chars on previous lines.
        // Actually, let's track depth as we go.

        if let Some(caps) = re_key.captures(trimmed) {
            let key = caps[1].to_string();

            if brace_depth == 1 && bracket_depth == 0 {
                // Top-level key
                in_deps = is_package_json
                    && (key == "dependencies"
                        || key == "devDependencies"
                        || key == "peerDependencies"
                        || key == "optionalDependencies");

                let decl = Declaration::new(
                    DeclKind::ConfigKey,
                    key.clone(),
                    trimmed.trim_end_matches(',').to_string(),
                    Visibility::Public,
                    line_num + 1,
                );
                let idx = declarations.len();
                declarations.push(decl);
                current_top = Some((key, idx));
            } else if brace_depth == 2 && bracket_depth == 0 {
                // Depth-2 key: child of current top-level
                if is_package_json && in_deps {
                    imports.push(Import {
                        text: key.clone(),
                    });
                }

                let child = Declaration::new(
                    DeclKind::ConfigKey,
                    key.clone(),
                    trimmed.trim_end_matches(',').to_string(),
                    Visibility::Private,
                    line_num + 1,
                );

                if let Some((_, parent_idx)) = &current_top {
                    declarations[*parent_idx].children.push(child);
                }
            }
        }

        // Update depth tracking after processing the line.
        // Skip over string contents to avoid counting braces inside strings.
        let mut chars = trimmed.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if brace_depth <= 1 {
                        in_deps = false;
                    }
                }
                '[' => bracket_depth += 1,
                ']' => bracket_depth -= 1,
                '"' => {
                    // Skip until closing quote (handling escaped quotes)
                    let mut escaped = false;
                    for inner in chars.by_ref() {
                        if escaped {
                            escaped = false;
                        } else if inner == '\\' {
                            escaped = true;
                        } else if inner == '"' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// SQL parser
// ---------------------------------------------------------------------------

fn parse_sql(content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let imports = Vec::new();
    let mut declarations = Vec::new();

    let re_create_table =
        Regex::new(r#"(?i)^CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"]?(\w+)[`"]?"#)
            .unwrap();
    let re_create_view =
        Regex::new(r#"(?i)^CREATE\s+(?:OR\s+REPLACE\s+)?(?:MATERIALIZED\s+)?VIEW\s+[`"]?(\w+)[`"]?"#)
            .unwrap();
    let re_create_index =
        Regex::new(r#"(?i)^CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"]?(\w+)[`"]?"#)
            .unwrap();
    let re_create_func =
        Regex::new(r#"(?i)^CREATE\s+(?:OR\s+REPLACE\s+)?(?:FUNCTION|PROCEDURE)\s+[`"]?(\w+)[`"]?"#)
            .unwrap();
    let re_create_type =
        Regex::new(r#"(?i)^CREATE\s+TYPE\s+[`"]?(\w+)[`"]?"#).unwrap();
    let re_column =
        Regex::new(r"(?i)^\s+(\w+)\s+(SERIAL|BIGSERIAL|SMALLSERIAL|INTEGER|INT|BIGINT|SMALLINT|TEXT|VARCHAR|CHAR|BOOLEAN|BOOL|FLOAT|DOUBLE|REAL|DECIMAL|NUMERIC|DATE|TIME|TIMESTAMP|TIMESTAMPTZ|UUID|JSONB?|BYTEA|BLOB|CLOB|XML|ARRAY)")
            .unwrap();

    let mut current_table: Option<usize> = None;
    let mut in_create_block = false;

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty()
            || trimmed.starts_with("--")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("*")
        {
            continue;
        }

        // CREATE TABLE
        if let Some(caps) = re_create_table.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::TableDef,
                name.clone(),
                format!("CREATE TABLE {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            current_table = Some(idx);
            in_create_block = true;
            continue;
        }

        // CREATE VIEW
        if let Some(caps) = re_create_view.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::TableDef,
                name.clone(),
                format!("CREATE VIEW {}", name),
                Visibility::Public,
                line_num + 1,
            ));
            current_table = None;
            in_create_block = false;
            continue;
        }

        // CREATE INDEX
        if let Some(caps) = re_create_index.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::ConfigKey,
                name.clone(),
                format!("CREATE INDEX {}", name),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // CREATE FUNCTION / PROCEDURE
        if let Some(caps) = re_create_func.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::Function,
                name.clone(),
                trimmed.to_string(),
                Visibility::Public,
                line_num + 1,
            ));
            current_table = None;
            in_create_block = false;
            continue;
        }

        // CREATE TYPE
        if let Some(caps) = re_create_type.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::SchemaType,
                name.clone(),
                format!("CREATE TYPE {}", name),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // Column definitions inside CREATE TABLE
        if in_create_block {
            if trimmed.starts_with(')') || trimmed.starts_with(");") {
                in_create_block = false;
                current_table = None;
                continue;
            }

            if let Some(caps) = re_column.captures(line) {
                let col_name = caps[1].to_string();
                let col_type = caps[2].to_string();

                // Skip SQL keywords that look like column names
                let upper = col_name.to_uppercase();
                if matches!(
                    upper.as_str(),
                    "PRIMARY" | "FOREIGN" | "UNIQUE" | "CHECK" | "CONSTRAINT" | "INDEX" | "KEY"
                ) {
                    continue;
                }

                let child = Declaration::new(
                    DeclKind::Field,
                    col_name.clone(),
                    format!("{} {}", col_name, col_type),
                    Visibility::Private,
                    line_num + 1,
                );

                if let Some(parent_idx) = current_table {
                    declarations[parent_idx].children.push(child);
                }
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// Markdown parser
// ---------------------------------------------------------------------------

fn parse_markdown(content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let imports: Vec<Import> = Vec::new();
    let mut declarations: Vec<Declaration> = Vec::new();

    let re_heading = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();

    // Stack of (heading_level, index_in_declarations) for building hierarchy.
    // We only parent H2+ under the nearest higher-level heading.
    let mut heading_stack: Vec<(usize, usize)> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(caps) = re_heading.captures(line) {
            let level = caps[1].len();
            let text = caps[2].trim().to_string();

            let decl = Declaration::new(
                DeclKind::Heading,
                text.clone(),
                format!("{} {}", &caps[1], text),
                Visibility::Public,
                line_num + 1,
            );

            // Pop stack entries that are at the same or deeper level
            while heading_stack
                .last()
                .map(|(l, _)| *l >= level)
                .unwrap_or(false)
            {
                heading_stack.pop();
            }

            if let Some((_, parent_idx)) = heading_stack.last() {
                // This heading is a child of the last heading at a shallower level.
                declarations[*parent_idx].children.push(decl);
                // We need the index of the child we just pushed, within the parent's children.
                // For deeper nesting, we track the top-level decl index only for direct children.
                // To keep it simple: only top-level (H1) or "no parent" headings go into
                // declarations directly; the rest are children. We don't add to the heading_stack
                // for children since we can't easily index into nested children.
                // Instead, let's use a flat approach: put all headings into declarations
                // and use the stack only for one level of nesting.
                //
                // Actually, let's restructure: only H1 at top level, H2+ as children of
                // the immediately preceding higher-level heading already in declarations.
                // For simplicity, we only handle one level of parent-child.
            } else {
                // No parent — this is a top-level heading.
                let idx = declarations.len();
                declarations.push(decl);
                heading_stack.push((level, idx));
                continue;
            }

            // For child headings added above, we still want to track them so that
            // even deeper headings can be children. But since children are nested in
            // a Vec<Declaration> and we only have the parent index, we take a simpler approach:
            // we track the parent, and always add sub-headings as direct children of
            // the nearest ancestor that lives in the top-level declarations vec.
            // Don't push child headings onto the stack — they can't be parents in our model.
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// Protobuf parser
// ---------------------------------------------------------------------------

fn parse_protobuf(content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let mut imports = Vec::new();
    let mut declarations = Vec::new();

    let re_syntax = Regex::new(r#"^syntax\s*=\s*"([^"]+)""#).unwrap();
    let re_package = Regex::new(r"^package\s+([\w.]+)\s*;").unwrap();
    let re_import = Regex::new(r#"^import\s+(?:public\s+)?"([^"]+)"\s*;"#).unwrap();
    let re_message = Regex::new(r"^message\s+(\w+)\s*\{?").unwrap();
    let re_service = Regex::new(r"^service\s+(\w+)\s*\{?").unwrap();
    let re_enum = Regex::new(r"^enum\s+(\w+)\s*\{?").unwrap();
    let re_rpc =
        Regex::new(r"^\s*rpc\s+(\w+)\s*\(\s*(?:stream\s+)?(\w+)\s*\)\s*returns\s*\(\s*(?:stream\s+)?(\w+)\s*\)")
            .unwrap();
    let re_field =
        Regex::new(r"^\s+(?:repeated\s+|optional\s+|required\s+|map<[^>]+>\s+)?(\w+(?:\.\w+)*)\s+(\w+)\s*=\s*(\d+)")
            .unwrap();
    let re_enum_variant = Regex::new(r"^\s+(\w+)\s*=\s*(\d+)\s*;").unwrap();

    enum BlockKind {
        Message(usize),
        Service(usize),
        Enum(usize),
    }

    let mut block_stack: Vec<BlockKind> = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut block_start_depth: Vec<i32> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("//") {
            // Still count braces in comments? No, skip.
            continue;
        }

        // Syntax declaration
        if let Some(caps) = re_syntax.captures(trimmed) {
            declarations.push(Declaration::new(
                DeclKind::ConfigKey,
                "syntax".to_string(),
                format!("syntax = \"{}\"", &caps[1]),
                Visibility::Public,
                line_num + 1,
            ));
            continue;
        }

        // Package
        if let Some(caps) = re_package.captures(trimmed) {
            imports.push(Import {
                text: format!("package {}", &caps[1]),
            });
            continue;
        }

        // Import
        if let Some(caps) = re_import.captures(trimmed) {
            imports.push(Import {
                text: caps[1].to_string(),
            });
            continue;
        }

        // Message (only at top level or nested)
        if let Some(caps) = re_message.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::Message,
                name.clone(),
                format!("message {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(BlockKind::Message(idx));
            block_start_depth.push(brace_depth);
            // Count braces on this line
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // Service
        if let Some(caps) = re_service.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::Service,
                name.clone(),
                format!("service {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(BlockKind::Service(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // Enum
        if let Some(caps) = re_enum.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::Enum,
                name.clone(),
                format!("enum {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(BlockKind::Enum(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // RPC inside a service
        if let Some(caps) = re_rpc.captures(trimmed) {
            let name = caps[1].to_string();
            let req = caps[2].to_string();
            let resp = caps[3].to_string();
            let child = Declaration::new(
                DeclKind::RpcMethod,
                name.clone(),
                format!("rpc {}({}) returns ({})", name, req, resp),
                Visibility::Public,
                line_num + 1,
            );
            if let Some(BlockKind::Service(idx)) = block_stack.last() {
                declarations[*idx].children.push(child);
            } else {
                declarations.push(child);
            }
            // Count braces
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // Field inside a message
        if let Some(caps) = re_field.captures(line) {
            let field_type = caps[1].to_string();
            let field_name = caps[2].to_string();

            let child = Declaration::new(
                DeclKind::Field,
                field_name.clone(),
                format!("{} {}", field_type, field_name),
                Visibility::Private,
                line_num + 1,
            );
            if let Some(BlockKind::Message(idx)) = block_stack.last() {
                declarations[*idx].children.push(child);
            }
            // Count braces
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // Enum variant inside an enum
        if let Some(caps) = re_enum_variant.captures(line) {
            let variant_name = caps[1].to_string();
            // Skip if it looks like a field (has a type before it) -- enum variants are just NAME = NUMBER
            let child = Declaration::new(
                DeclKind::Variant,
                variant_name.clone(),
                variant_name.clone(),
                Visibility::Public,
                line_num + 1,
            );
            if let Some(BlockKind::Enum(idx)) = block_stack.last() {
                declarations[*idx].children.push(child);
            }
        }

        // Count braces for all other lines
        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }

        // Pop blocks if brace depth has returned
        while let Some(start_depth) = block_start_depth.last() {
            if brace_depth <= *start_depth {
                block_start_depth.pop();
                block_stack.pop();
            } else {
                break;
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// GraphQL parser
// ---------------------------------------------------------------------------

fn parse_graphql(content: &str) -> (Vec<Import>, Vec<Declaration>) {
    let imports = Vec::new();
    let mut declarations = Vec::new();

    let re_type = Regex::new(r"^type\s+(\w+)(?:\s+implements\s+[\w\s&]+)?\s*\{?").unwrap();
    let re_input = Regex::new(r"^input\s+(\w+)\s*\{?").unwrap();
    let re_interface = Regex::new(r"^interface\s+(\w+)\s*\{?").unwrap();
    let re_enum = Regex::new(r"^enum\s+(\w+)\s*\{?").unwrap();
    let re_schema = Regex::new(r"^schema\s*\{?").unwrap();
    let re_query = Regex::new(r"^(?:query|mutation|subscription)\s+(\w+)").unwrap();
    let re_field = Regex::new(r"^\s+(\w+)(?:\([^)]*\))?\s*:\s*(.+)$").unwrap();

    enum GqlBlock {
        Type(usize),
        Input(usize),
        Interface(usize),
        Enum(usize),
        Schema(usize),
        Other,
    }

    let mut block_stack: Vec<GqlBlock> = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut block_start_depth: Vec<i32> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // schema { ... }
        if re_schema.is_match(trimmed) {
            let decl = Declaration::new(
                DeclKind::ConfigKey,
                "schema".to_string(),
                "schema".to_string(),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(GqlBlock::Schema(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // query/mutation/subscription Name
        if let Some(caps) = re_query.captures(trimmed) {
            let name = caps[1].to_string();
            declarations.push(Declaration::new(
                DeclKind::Function,
                name.clone(),
                trimmed.to_string(),
                Visibility::Public,
                line_num + 1,
            ));
            block_stack.push(GqlBlock::Other);
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // type Name { ... }
        if let Some(caps) = re_type.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::SchemaType,
                name.clone(),
                trimmed.trim_end_matches('{').trim().to_string(),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(GqlBlock::Type(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // input Name { ... }
        if let Some(caps) = re_input.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::SchemaType,
                name.clone(),
                format!("input {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(GqlBlock::Input(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // interface Name { ... }
        if let Some(caps) = re_interface.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::Interface,
                name.clone(),
                format!("interface {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(GqlBlock::Interface(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // enum Name { ... }
        if let Some(caps) = re_enum.captures(trimmed) {
            let name = caps[1].to_string();
            let decl = Declaration::new(
                DeclKind::Enum,
                name.clone(),
                format!("enum {}", name),
                Visibility::Public,
                line_num + 1,
            );
            let idx = declarations.len();
            declarations.push(decl);
            block_stack.push(GqlBlock::Enum(idx));
            block_start_depth.push(brace_depth);
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            continue;
        }

        // Fields inside type/input/interface blocks
        if let Some(caps) = re_field.captures(line) {
            let field_name = caps[1].to_string();
            let field_type = caps[2].trim().trim_end_matches(',').to_string();

            // Skip GraphQL keywords
            if matches!(
                field_name.as_str(),
                "query" | "mutation" | "subscription" | "type" | "input" | "interface" | "enum"
                    | "schema" | "extend" | "directive" | "scalar" | "union" | "fragment"
            ) {
                // count braces and continue
                for ch in trimmed.chars() {
                    match ch {
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }
                continue;
            }

            let child = Declaration::new(
                DeclKind::Field,
                field_name.clone(),
                format!("{}: {}", field_name, field_type),
                Visibility::Public,
                line_num + 1,
            );

            match block_stack.last() {
                Some(GqlBlock::Type(idx))
                | Some(GqlBlock::Input(idx))
                | Some(GqlBlock::Interface(idx)) => {
                    declarations[*idx].children.push(child);
                }
                Some(GqlBlock::Schema(idx)) => {
                    declarations[*idx].children.push(child);
                }
                _ => {
                    declarations.push(child);
                }
            }
        } else if brace_depth > 0 {
            // Inside an enum block, lines might just be variant names
            if let Some(GqlBlock::Enum(idx)) = block_stack.last() {
                let variant = trimmed.trim_end_matches(',').trim();
                if !variant.is_empty()
                    && !variant.starts_with('{')
                    && !variant.starts_with('}')
                    && !variant.starts_with('#')
                    && variant.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                {
                    let child = Declaration::new(
                        DeclKind::Variant,
                        variant.to_string(),
                        variant.to_string(),
                        Visibility::Public,
                        line_num + 1,
                    );
                    declarations[*idx].children.push(child);
                }
            }
        }

        // Count braces
        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }

        // Pop blocks
        while let Some(start_depth) = block_start_depth.last() {
            if brace_depth <= *start_depth {
                block_start_depth.pop();
                block_stack.pop();
            } else {
                break;
            }
        }
    }

    (imports, declarations)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_value(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_parser(lang: Language) -> RegexParser {
        RegexParser::new(lang)
    }

    #[test]
    fn test_shell_functions() {
        let content = r#"#!/bin/bash
source ./utils.sh
. /etc/profile

function greet() {
    echo "hello"
}

function setup {
    echo "setup"
}

cleanup() {
    echo "done"
}

export PATH="/usr/local/bin:$PATH"
alias ll="ls -la"
"#;
        let parser = make_parser(Language::Shell);
        let result = parser
            .parse_file(Path::new("test.sh"), content)
            .unwrap();
        assert_eq!(result.imports.len(), 2);
        assert_eq!(result.imports[0].text, "./utils.sh");
        assert!(result.declarations.iter().any(|d| d.name == "greet"));
        assert!(result.declarations.iter().any(|d| d.name == "setup"));
        assert!(result.declarations.iter().any(|d| d.name == "cleanup"));
        assert!(result.declarations.iter().any(|d| d.name == "PATH"));
        assert!(result.declarations.iter().any(|d| d.name == "ll"));
    }

    #[test]
    fn test_toml_cargo() {
        let content = r#"[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
serde = "1"
anyhow = "1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
"#;
        let parser = make_parser(Language::Toml);
        let result = parser
            .parse_file(Path::new("Cargo.toml"), content)
            .unwrap();
        // Should have sections and imports for deps
        assert!(result.imports.iter().any(|i| i.text == "serde"));
        assert!(result.imports.iter().any(|i| i.text == "anyhow"));
        assert!(result.imports.iter().any(|i| i.text == "tokio"));
        assert!(result.declarations.iter().any(|d| d.name == "package"));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "dependencies"));
    }

    #[test]
    fn test_sql_create_table() {
        let content = r#"CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email TEXT UNIQUE
);

CREATE INDEX idx_users_email ON users(email);

CREATE FUNCTION get_user(user_id INTEGER) RETURNS users AS $$
BEGIN
    RETURN QUERY SELECT * FROM users WHERE id = user_id;
END;
$$ LANGUAGE plpgsql;

CREATE VIEW active_users AS
SELECT * FROM users WHERE active = true;

CREATE TYPE status AS ENUM ('active', 'inactive');
"#;
        let parser = make_parser(Language::Sql);
        let result = parser.parse_file(Path::new("schema.sql"), content).unwrap();
        let table = result
            .declarations
            .iter()
            .find(|d| d.name == "users" && d.kind == DeclKind::TableDef)
            .unwrap();
        assert!(table.children.iter().any(|c| c.name == "id"));
        assert!(table.children.iter().any(|c| c.name == "name"));
        assert!(table.children.iter().any(|c| c.name == "email"));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "idx_users_email"));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "get_user" && d.kind == DeclKind::Function));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "active_users" && d.kind == DeclKind::TableDef));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "status" && d.kind == DeclKind::SchemaType));
    }

    #[test]
    fn test_markdown_headings() {
        let content = r#"# Introduction

Some text here.

## Getting Started

### Installation

### Configuration

## Usage

# API Reference

## Methods
"#;
        let parser = make_parser(Language::Markdown);
        let result = parser
            .parse_file(Path::new("README.md"), content)
            .unwrap();
        // Two H1 headings at top level
        let top_level: Vec<_> = result
            .declarations
            .iter()
            .filter(|d| d.signature.starts_with("# "))
            .collect();
        assert_eq!(top_level.len(), 2);
        // H2 headings as children of H1
        let intro = result
            .declarations
            .iter()
            .find(|d| d.name == "Introduction")
            .unwrap();
        assert!(intro.children.iter().any(|c| c.name == "Getting Started"));
        assert!(intro.children.iter().any(|c| c.name == "Usage"));
    }

    #[test]
    fn test_protobuf() {
        let content = r#"syntax = "proto3";

package myservice.v1;

import "google/protobuf/timestamp.proto";

message User {
    string id = 1;
    string name = 2;
    string email = 3;
}

enum Status {
    UNKNOWN = 0;
    ACTIVE = 1;
    INACTIVE = 2;
}

service UserService {
    rpc GetUser(GetUserRequest) returns (User);
    rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);
}
"#;
        let parser = make_parser(Language::Protobuf);
        let result = parser
            .parse_file(Path::new("user.proto"), content)
            .unwrap();
        assert!(result.imports.iter().any(|i| i.text.contains("myservice")));
        assert!(result
            .imports
            .iter()
            .any(|i| i.text.contains("timestamp")));
        let msg = result
            .declarations
            .iter()
            .find(|d| d.name == "User" && d.kind == DeclKind::Message)
            .unwrap();
        assert_eq!(msg.children.len(), 3);
        let enm = result
            .declarations
            .iter()
            .find(|d| d.name == "Status" && d.kind == DeclKind::Enum)
            .unwrap();
        assert_eq!(enm.children.len(), 3);
        let svc = result
            .declarations
            .iter()
            .find(|d| d.name == "UserService" && d.kind == DeclKind::Service)
            .unwrap();
        assert_eq!(svc.children.len(), 2);
        assert!(svc.children[0].kind == DeclKind::RpcMethod);
    }

    #[test]
    fn test_graphql() {
        let content = r#"type User {
    id: ID!
    name: String!
    email: String
    posts: [Post!]!
}

input CreateUserInput {
    name: String!
    email: String!
}

interface Node {
    id: ID!
}

enum Role {
    ADMIN
    USER
    GUEST
}

query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
    }
}

schema {
    query: Query
    mutation: Mutation
}
"#;
        let parser = make_parser(Language::GraphQL);
        let result = parser
            .parse_file(Path::new("schema.graphql"), content)
            .unwrap();
        let user_type = result
            .declarations
            .iter()
            .find(|d| d.name == "User" && d.kind == DeclKind::SchemaType)
            .unwrap();
        assert_eq!(user_type.children.len(), 4);
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "CreateUserInput" && d.kind == DeclKind::SchemaType));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "Node" && d.kind == DeclKind::Interface));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "Role" && d.kind == DeclKind::Enum));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "GetUser" && d.kind == DeclKind::Function));
        assert!(result
            .declarations
            .iter()
            .any(|d| d.name == "schema" && d.kind == DeclKind::ConfigKey));
    }
}
