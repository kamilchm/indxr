# Codebase Index: indxr

> Generated: 2026-03-23 09:15:10 UTC | Files: 23 | Lines: 4636
> Languages: Rust (23)

## Directory Structure

```
indxr/
  src/
    cache/
      fingerprint.rs
      mod.rs
    cli.rs
    error.rs
    languages.rs
    main.rs
    model/
      declarations.rs
      mod.rs
    output/
      markdown.rs
      mod.rs
      yaml.rs
    parser/
      mod.rs
      queries/
        c.rs
        cpp.rs
        go.rs
        java.rs
        javascript.rs
        mod.rs
        python.rs
        rust.rs
        typescript.rs
      tree_sitter_parser.rs
    walker/
      mod.rs
```

---

## src/cache/fingerprint.rs

**Language:** Rust | **Size:** 464 B | **Lines:** 12

**Imports:**
- `xxhash_rust::xxh3::xxh3_64`

**Declarations:**

`pub fn compute_hash(content: &[u8]) -> u64`
> Fast non-cryptographic hash of file content for change detection.

`pub fn metadata_matches(cached_mtime: u64, cached_size: u64, current_mtime: u64, current_size: u64) -> bool`
> Quick change check using mtime + size. Returns true if the file appears unchanged based on metadata alone.

---

## src/cache/mod.rs

**Language:** Rust | **Size:** 3.5 KB | **Lines:** 128

**Imports:**
- `std::collections::HashMap`
- `std::fs`
- `std::path::{Path, PathBuf}`
- `anyhow::Result`
- `serde::{Deserialize, Serialize}`
- `crate::model::FileIndex`
- `self::fingerprint::{compute_hash, metadata_matches}`

**Declarations:**

`pub mod fingerprint`

`const CACHE_VERSION: u32 = 1`

`const CACHE_FILENAME: &str = "cache.bin"`

`struct CacheStore`
> Fields: `version: u32`, `entries: HashMap<PathBuf, CacheEntry>`

`struct CacheEntry`
> Fields: `mtime: u64`, `size: u64`, `content_hash: u64`, `file_index: FileIndex`

`pub struct Cache`
> Fields: `store: CacheStore`, `cache_dir: PathBuf`, `dirty: bool`

**`impl Cache`**
  `pub fn load(cache_dir: &Path) -> Self`
  > Load cache from disk, or create empty if not found / incompatible.

  `pub fn disabled() -> Self`
  > Create a no-op cache that never hits and never saves.

  `fn empty_store() -> CacheStore`

  `pub fn get(&self, relative_path: &Path, size: u64, mtime: u64) -> Option<FileIndex>`
  > Try to get a cached FileIndex for a file. Returns Some if the file hasn't changed (based on mtime + size).

  `pub fn insert(&mut self, relative_path: &Path, size: u64, mtime: u64, content: &[u8], file_index: FileIndex)`
  > Insert or update a cache entry for a file.

  `pub fn prune(&mut self, existing_paths: &[PathBuf])`
  > Remove entries for files that no longer exist.

  `pub fn save(&self) -> Result<()>`
  > Save cache to disk if it has been modified.

  `pub fn len(&self) -> usize`


---

## src/cli.rs

**Language:** Rust | **Size:** 1.6 KB | **Lines:** 68

**Imports:**
- `std::path::PathBuf`
- `clap::Parser`
- `crate::model::DetailLevel`

**Declarations:**

`pub struct Cli`
> Fields: `path: PathBuf`, `output: Option<PathBuf>`, `format: OutputFormat`, `detail: DetailLevel`, `max_depth: Option<usize>`, `max_file_size: u64`, `languages: Option<Vec<String>>`, `exclude: Option<Vec<String>>`, `no_gitignore: bool`, `no_cache: bool`, `cache_dir: PathBuf`, `quiet: bool`, `stats: bool`

`pub enum OutputFormat`
> Variants: `Markdown`, `Json`, `Yaml`

---

## src/error.rs

**Language:** Rust | **Size:** 324 B | **Lines:** 14

**Imports:**
- `thiserror::Error`

**Declarations:**

`pub enum IndxrError`
> Variants: `Io`, `Parse`, `UnsupportedLanguage`

---

## src/languages.rs

**Language:** Rust | **Size:** 1.9 KB | **Lines:** 66

**Imports:**
- `std::fmt`
- `std::path::Path`
- `serde::{Deserialize, Serialize}`

**Declarations:**

`pub enum Language`
> Variants: `Rust`, `Python`, `TypeScript`, `JavaScript`, `Go`, `Java`, `C`, `Cpp`

**`impl Language`**
  `pub fn detect(path: &Path) -> Option<Self>`

  `pub fn name(&self) -> &str`

  `pub fn from_name(name: &str) -> Option<Self>`


**`impl fmt::Display for Language`**
  `fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`


---

## src/main.rs

**Language:** Rust | **Size:** 6.2 KB | **Lines:** 223

**Imports:**
- `std::collections::HashMap`
- `std::fs`
- `std::time::Instant`
- `anyhow::Result`
- `clap::Parser`
- `rayon::prelude::*`
- `crate::cache::Cache`
- `crate::cli::{Cli, OutputFormat}`
- `crate::languages::Language`
- `crate::model::{CodebaseIndex, IndexStats}`
- `crate::output::OutputFormatter`
- `crate::output::markdown::MarkdownFormatter`
- `crate::output::yaml::YamlFormatter`
- `crate::parser::ParserRegistry`

**Declarations:**

`mod cache`

`mod cli`

`mod error`

`mod languages`

`mod model`

`mod output`

`mod parser`

`mod walker`

`fn main() -> Result<()>`

---

## src/model/declarations.rs

**Language:** Rust | **Size:** 1.7 KB | **Lines:** 67

**Imports:**
- `std::fmt`
- `serde::{Deserialize, Serialize}`

**Declarations:**

`pub struct Declaration`
> Fields: `kind: DeclKind`, `name: String`, `signature: String`, `visibility: Visibility`, `line: usize`, `doc_comment: Option<String>`, `children: Vec<Declaration>`

`pub enum DeclKind`
> Variants: `Function`, `Struct`, `Enum`, `Trait`, `Impl`, `Constant`, `Static`, `TypeAlias`, `Module`, `Class`, `Field`, `Variant`, `Method`

`pub enum Visibility`
> Variants: `Public`, `PublicCrate`, `Private`

**`impl fmt::Display for Visibility`**
  `fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`


**`impl fmt::Display for DeclKind`**
  `fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`


---

## src/model/mod.rs

**Language:** Rust | **Size:** 1.1 KB | **Lines:** 56

**Imports:**
- `std::collections::HashMap`
- `std::path::PathBuf`
- `serde::{Deserialize, Serialize}`
- `crate::languages::Language`
- `self::declarations::Declaration`

**Declarations:**

`pub mod declarations`

`pub enum DetailLevel`
> Variants: `Summary`, `Signatures`, `Full`

`pub struct CodebaseIndex`
> Fields: `root: PathBuf`, `root_name: String`, `generated_at: String`, `files: Vec<FileIndex>`, `tree: Vec<TreeEntry>`, `stats: IndexStats`

`pub struct FileIndex`
> Fields: `path: PathBuf`, `language: Language`, `size: u64`, `lines: usize`, `imports: Vec<Import>`, `declarations: Vec<Declaration>`

`pub struct Import`
> Fields: `text: String`

`pub struct TreeEntry`
> Fields: `path: String`, `is_dir: bool`, `depth: usize`

`pub struct IndexStats`
> Fields: `total_files: usize`, `total_lines: usize`, `languages: HashMap<String, usize>`, `duration_ms: u64`

---

## src/output/markdown.rs

**Language:** Rust | **Size:** 5.9 KB | **Lines:** 195

**Imports:**
- `std::fmt::Write`
- `anyhow::Result`
- `crate::model::CodebaseIndex`
- `crate::model::DetailLevel`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::OutputFormatter`

**Declarations:**

`pub struct MarkdownFormatter`

**`impl OutputFormatter for MarkdownFormatter`**
  `fn format(&self, index: &CodebaseIndex, detail: DetailLevel) -> Result<String>`


`fn format_declaration( out: &mut String, decl: &Declaration, depth: usize, detail: DetailLevel, ) -> std::fmt::Result`

`fn format_size(bytes: u64) -> String`

---

## src/output/mod.rs

**Language:** Rust | **Size:** 233 B | **Lines:** 11

**Imports:**
- `anyhow::Result`
- `crate::model::CodebaseIndex`
- `crate::model::DetailLevel`

**Declarations:**

`pub mod markdown`

`pub mod yaml`

`pub trait OutputFormatter`
  `fn format(&self, index: &CodebaseIndex, detail: DetailLevel) -> Result<String>`


---

## src/output/yaml.rs

**Language:** Rust | **Size:** 319 B | **Lines:** 14

**Imports:**
- `anyhow::Result`
- `crate::model::CodebaseIndex`
- `crate::model::DetailLevel`
- `super::OutputFormatter`

**Declarations:**

`pub struct YamlFormatter`

**`impl OutputFormatter for YamlFormatter`**
  `fn format(&self, index: &CodebaseIndex, _detail: DetailLevel) -> Result<String>`


---

## src/parser/mod.rs

**Language:** Rust | **Size:** 1.2 KB | **Lines:** 49

**Imports:**
- `std::path::Path`
- `anyhow::Result`
- `crate::languages::Language`
- `crate::model::FileIndex`

**Declarations:**

`pub mod queries`

`pub mod tree_sitter_parser`

`pub trait LanguageParser: Send + Sync`
  `fn language(&self) -> Language`

  `fn parse_file(&self, path: &Path, content: &str) -> Result<FileIndex>`


`pub struct ParserRegistry`
> Fields: `parsers: Vec<Box<dyn LanguageParser>>`

**`impl ParserRegistry`**
  `pub fn new() -> Self`

  `pub fn get_parser(&self, language: &Language) -> Option<&dyn LanguageParser>`


---

## src/parser/queries/c.rs

**Language:** Rust | **Size:** 13.6 KB | **Lines:** 422

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct CExtractor`

**`impl DeclExtractor for CExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_visibility_from_text(node: Node<'_>, source: &str) -> Visibility`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_include(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function_name<'a>(declarator: Node<'_>, source: &'a str) -> Option<&'a str>`
> Extract function name from a function_declarator by traversing nested declarators. function_definition → declarator (function_declarator) → declarator (identifier)

`fn extract_function_definition(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_declaration(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn has_function_declarator(node: Node<'_>) -> bool`

`fn extract_function_name_from_decl<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str>`

`fn extract_var_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str>`

`fn extract_struct(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_struct_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enumerator(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_typedef(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_preproc_def(node: Node<'_>, source: &str) -> Option<Declaration>`

---

## src/parser/queries/cpp.rs

**Language:** Rust | **Size:** 22.5 KB | **Lines:** 662

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct CppExtractor`

**`impl DeclExtractor for CppExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn extract_top_level( root: Node<'_>, source: &str, imports: &mut Vec<Import>, declarations: &mut Vec<Declaration>, )`

`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_visibility_from_text(node: Node<'_>, source: &str) -> Visibility`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_include(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_using(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function_name<'a>(declarator: Node<'_>, source: &'a str) -> Option<&'a str>`
> Extract function name from a function_declarator by traversing nested declarators.

`fn extract_function_definition(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_declaration(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn has_function_declarator(node: Node<'_>) -> bool`

`fn extract_function_name_from_decl<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str>`

`fn extract_var_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str>`

`fn extract_class( node: Node<'_>, source: &str, default_visibility: Visibility, ) -> Option<Declaration>`

`fn extract_class_member_declaration(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enumerator(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_namespace(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_template(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_typedef(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_preproc_def(node: Node<'_>, source: &str) -> Option<Declaration>`

---

## src/parser/queries/go.rs

**Language:** Rust | **Size:** 13.0 KB | **Lines:** 437

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct GoExtractor`

**`impl DeclExtractor for GoExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_visibility(name: &str) -> Visibility`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_receiver_type(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_type_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_type_spec( node: Node<'_>, source: &str, parent_doc: &Option<String>, ) -> Option<Declaration>`

`fn extract_struct_fields(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_struct_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_interface_methods(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_method_spec(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_const_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_const_spec( node: Node<'_>, source: &str, parent_doc: &Option<String>, ) -> Option<Declaration>`

`fn extract_var_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_var_spec( node: Node<'_>, source: &str, parent_doc: &Option<String>, ) -> Option<Declaration>`

---

## src/parser/queries/java.rs

**Language:** Rust | **Size:** 12.0 KB | **Lines:** 381

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct JavaExtractor`

**`impl DeclExtractor for JavaExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_modifiers_text(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_visibility(node: Node<'_>, source: &str) -> Visibility`

`fn has_modifier(node: Node<'_>, source: &str, keyword: &str) -> bool`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_interface(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum_constant(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_constructor(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_field(node: Node<'_>, source: &str) -> Option<Declaration>`

---

## src/parser/queries/javascript.rs

**Language:** Rust | **Size:** 10.0 KB | **Lines:** 336

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct JavaScriptExtractor`

**`impl DeclExtractor for JavaScriptExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn process_top_level_node( node: Node<'_>, source: &str, imports: &mut Vec<Import>, declarations: &mut Vec<Declaration>, is_exported: bool, )`

`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_lexical_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_variable_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_variable_declarator( node: Node<'_>, parent: Node<'_>, source: &str, ) -> Option<Declaration>`

---

## src/parser/queries/mod.rs

**Language:** Rust | **Size:** 947 B | **Lines:** 29

**Imports:**
- `crate::languages::Language`
- `crate::model::Import`
- `crate::model::declarations::Declaration`

**Declarations:**

`pub mod c`

`pub mod cpp`

`pub mod go`

`pub mod java`

`pub mod javascript`

`pub mod python`

`pub mod rust`

`pub mod typescript`

`pub trait DeclExtractor: Send + Sync`
  `fn extract(&self, root: tree_sitter::Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`pub fn get_extractor(language: &Language) -> Box<dyn DeclExtractor>`

---

## src/parser/queries/python.rs

**Language:** Rust | **Size:** 11.0 KB | **Lines:** 334

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct PythonExtractor`

**`impl DeclExtractor for PythonExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_visibility(name: &str) -> Visibility`

`fn extract_docstring(body: Node<'_>, source: &str) -> Option<String>`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_function_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function(node: Node<'_>, source: &str, kind: DeclKind) -> Option<Declaration>`

`fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn find_inner_definition(decorated: Node<'_>) -> Option<Node<'_>>`

`fn extract_decorators(decorated: Node<'_>, source: &str) -> Vec<String>`

`fn extract_decorated(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_assignment(node: Node<'_>, source: &str) -> Option<Declaration>`

---

## src/parser/queries/rust.rs

**Language:** Rust | **Size:** 12.8 KB | **Lines:** 416

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct RustExtractor`

**`impl DeclExtractor for RustExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_visibility(node: Node<'_>, source: &str) -> Visibility`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function(node: Node<'_>, source: &str, kind: DeclKind) -> Option<Declaration>`

`fn extract_struct(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_variant(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_trait(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_impl(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_const_or_static( node: Node<'_>, source: &str, kind: DeclKind, ) -> Option<Declaration>`

`fn extract_type_alias(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_module(node: Node<'_>, source: &str) -> Option<Declaration>`

---

## src/parser/queries/typescript.rs

**Language:** Rust | **Size:** 16.2 KB | **Lines:** 526

**Imports:**
- `tree_sitter::Node`
- `crate::model::Import`
- `crate::model::declarations::{DeclKind, Declaration, Visibility}`
- `super::DeclExtractor`

**Declarations:**

`pub struct TypeScriptExtractor`

**`impl DeclExtractor for TypeScriptExtractor`**
  `fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>)`


`fn process_top_level_node( node: Node<'_>, source: &str, imports: &mut Vec<Import>, declarations: &mut Vec<Declaration>, is_exported: bool, )`

`fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str`

`fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String>`

`fn extract_signature(node: Node<'_>, source: &str) -> String`

`fn extract_import(node: Node<'_>, source: &str) -> Option<Import>`

`fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_member_visibility(node: Node<'_>, source: &str) -> Visibility`
> Check for accessibility_modifier (public/private/protected) on class members.

`fn extract_interface(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_interface_method(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_property_signature(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_type_alias(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_enum_member(node: Node<'_>, source: &str) -> Option<Declaration>`

`fn extract_lexical_declaration(node: Node<'_>, source: &str) -> Vec<Declaration>`

`fn extract_variable_declarator( node: Node<'_>, parent: Node<'_>, source: &str, ) -> Option<Declaration>`

---

## src/parser/tree_sitter_parser.rs

**Language:** Rust | **Size:** 1.9 KB | **Lines:** 65

**Imports:**
- `std::path::Path`
- `anyhow::Result`
- `crate::languages::Language`
- `crate::model::FileIndex`
- `super::LanguageParser`
- `super::queries`

**Declarations:**

`pub struct TreeSitterParser`
> Fields: `language: Language`

**`impl TreeSitterParser`**
  `pub fn new(language: Language) -> Self`

  `fn get_ts_language(&self) -> tree_sitter::Language`


**`impl LanguageParser for TreeSitterParser`**
  `fn language(&self) -> Language`

  `fn parse_file(&self, path: &Path, content: &str) -> Result<FileIndex>`


---

## src/walker/mod.rs

**Language:** Rust | **Size:** 3.3 KB | **Lines:** 125

**Imports:**
- `std::collections::BTreeSet`
- `std::path::{Path, PathBuf}`
- `anyhow::Result`
- `ignore::WalkBuilder`
- `crate::languages::Language`
- `crate::model::TreeEntry`

**Declarations:**

`pub struct WalkResult`
> Fields: `files: Vec<FileEntry>`, `tree: Vec<TreeEntry>`

`pub struct FileEntry`
> Fields: `path: PathBuf`, `relative_path: PathBuf`, `language: Language`, `size: u64`, `mtime: u64`

`pub fn walk_directory( root: &Path, respect_gitignore: bool, max_file_size: u64, max_depth: Option<usize>, exclude_patterns: &[String], ) -> Result<WalkResult>`

