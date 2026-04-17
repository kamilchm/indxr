use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use globset::GlobBuilder;
use serde::Deserialize;

use crate::diff;
use crate::languages::Language;
use crate::llm::{LlmClient, Message, Role};
use crate::model::declarations::{Declaration, Visibility};
use crate::model::{FileIndex, TreeEntry, WorkspaceIndex};
use crate::parser::ParserRegistry;

use super::page::{self, Contradiction, Frontmatter, PageType, WikiPage};
use super::prompts;
use super::store::WikiStore;

/// Plan for a single wiki page, returned by the planning LLM call.
#[derive(Debug, Deserialize)]
struct PagePlan {
    id: String,
    page_type: PageType,
    title: String,
    source_files: Vec<String>,
}

/// Result of an incremental wiki update.
pub struct UpdateResult {
    pub pages_updated: usize,
    pub pages_removed: usize,
    pub pages_created: usize,
}

/// Result of incremental planning for uncovered files.
#[derive(Debug, Deserialize)]
struct IncrementalPlan {
    #[serde(default)]
    assignments: Vec<FileAssignment>,
    #[serde(default)]
    new_pages: Vec<PagePlan>,
}

#[derive(Debug, Deserialize)]
struct FileAssignment {
    file: String,
    page_id: String,
}

pub struct WikiGenerator<'a> {
    llm: &'a LlmClient,
    workspace: &'a WorkspaceIndex,
    /// Pre-built path→FileIndex lookup for O(1) access.
    file_index: HashMap<String, Vec<(&'a FileIndex, String)>>,
    /// All indexed file paths relative to the workspace root.
    workspace_paths: Vec<String>,
}

impl<'a> WikiGenerator<'a> {
    pub fn new(llm: &'a LlmClient, workspace: &'a WorkspaceIndex) -> Self {
        let file_index = Self::build_file_index(workspace);
        let workspace_paths = Self::collect_workspace_paths(workspace);
        Self {
            llm,
            workspace,
            file_index,
            workspace_paths,
        }
    }

    /// Build a lookup from filename → Vec<(FileIndex, full_path_string)>
    /// for fast path matching.
    fn build_file_index(
        workspace: &'a WorkspaceIndex,
    ) -> HashMap<String, Vec<(&'a FileIndex, String)>> {
        let mut map: HashMap<String, Vec<(&FileIndex, String)>> = HashMap::new();
        for member in &workspace.members {
            for file in &member.index.files {
                let full_path = file.path.to_string_lossy().to_string();
                // Index by full path for exact match
                map.entry(full_path.clone())
                    .or_default()
                    .push((file, full_path.clone()));
                // Also index by filename for suffix matching
                if let Some(name) = file.path.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    if name_str != full_path {
                        map.entry(name_str).or_default().push((file, full_path));
                    }
                }
            }
        }
        map
    }

    fn collect_workspace_paths(workspace: &'a WorkspaceIndex) -> Vec<String> {
        let mut paths: Vec<String> = workspace
            .members
            .iter()
            .flat_map(|member| {
                member
                    .index
                    .files
                    .iter()
                    .map(|file| file.path.to_string_lossy().to_string())
            })
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Full wiki generation from scratch.
    pub async fn generate_full(&self, wiki_dir: &Path, dry_run: bool) -> Result<WikiStore> {
        let git_ref = current_git_ref(&self.workspace.root)?;
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // Stage 1: Plan the wiki structure
        eprintln!("Planning wiki structure...");
        let plans = self.plan_structure().await?;
        eprintln!("Planned {} pages", plans.len());

        if dry_run {
            eprintln!("\n--- Dry run: wiki plan ---");
            for plan in &plans {
                eprintln!(
                    "  [{:?}] {} — {} ({})",
                    plan.page_type,
                    plan.id,
                    plan.title,
                    plan.source_files.len()
                );
                for f in &plan.source_files {
                    eprintln!("    - {}", f);
                }
            }
            return Ok(WikiStore::new(wiki_dir));
        }

        // Build lookup of all page titles for cross-referencing
        let all_pages_ctx: Vec<String> = plans
            .iter()
            .map(|p| format!("[[{}]] — {}", p.id, p.title))
            .collect();
        let all_pages_str = all_pages_ctx.join("\n");

        let mut store = WikiStore::new(wiki_dir);
        store.manifest.generated_at_ref = git_ref.clone();
        store.manifest.generated_at = timestamp.clone();

        // Stage 2: Generate each page (with incremental save)
        let content_plans: Vec<&PagePlan> = plans
            .iter()
            .filter(|p| p.page_type != PageType::Index)
            .collect();
        let total = content_plans.len();
        for (i, plan) in content_plans.iter().enumerate() {
            eprintln!("Generating page {}/{}: {}...", i + 1, total, plan.title);

            let page = self
                .generate_page(plan, &all_pages_str, &git_ref, &timestamp)
                .await?;
            store.upsert_page(page);

            // Incremental save — writes only this page + manifest
            store.save_incremental(&plan.id)?;
        }

        // Stage 3: Generate index page
        eprintln!("Generating cross-reference index...");
        let index_page = self
            .generate_index(&store.pages, &git_ref, &timestamp)
            .await?;
        store.upsert_page(index_page);
        store.save()?;

        Ok(store)
    }

    /// Incremental update: regenerate only wiki pages affected by code changes.
    pub async fn update_affected(
        &self,
        store: &mut WikiStore,
        since_ref: &str,
    ) -> Result<UpdateResult> {
        let root = &self.workspace.root;
        let git_ref = current_git_ref(root)?;
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // 1. Get changed files since the reference
        let changed_paths = diff::get_changed_files(root, since_ref)?;
        if changed_paths.is_empty() {
            eprintln!("No file changes since {}", since_ref);
            return Ok(UpdateResult {
                pages_updated: 0,
                pages_removed: 0,
                pages_created: 0,
            });
        }
        eprintln!(
            "Found {} changed files since {}",
            changed_paths.len(),
            since_ref
        );

        // 2. Build structural diff for context
        let all_files = self.collect_all_file_refs();
        let registry = ParserRegistry::new();
        let mut old_files: HashMap<PathBuf, FileIndex> = HashMap::new();
        for path in &changed_paths {
            if let Ok(Some(old_content)) = diff::get_file_at_ref(root, path, since_ref) {
                if let Some(lang) = Language::detect(path) {
                    if let Some(parser) = registry.get_parser(&lang) {
                        if let Ok(index) = parser.parse_file(path, &old_content) {
                            old_files.insert(path.clone(), index);
                        }
                    }
                }
            }
        }

        let structural_diff = diff::compute_structural_diff(all_files, &old_files, &changed_paths);
        let diff_markdown = diff::format_diff_markdown(&structural_diff);

        // 3. Collect all changed file paths as strings for matching
        let changed_set: HashSet<String> = changed_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        // 4. Find affected pages: any page whose source_files overlap with changed files
        let affected_ids: Vec<String> = store
            .pages
            .iter()
            .filter(|page| {
                page.frontmatter.page_type != PageType::Index
                    && page
                        .frontmatter
                        .source_files
                        .iter()
                        .any(|sf| changed_set.contains(sf))
            })
            .map(|page| page.frontmatter.id.clone())
            .collect();

        if affected_ids.is_empty() {
            eprintln!("No wiki pages are affected by these changes");
            // Still update the ref so we don't re-check the same range
            store.manifest.generated_at_ref = git_ref;
            store.manifest.generated_at = timestamp;
            return Ok(UpdateResult {
                pages_updated: 0,
                pages_removed: 0,
                pages_created: 0,
            });
        }

        eprintln!(
            "Updating {} affected pages: {}",
            affected_ids.len(),
            affected_ids.join(", ")
        );

        // 5. Build cross-reference context from all pages
        let all_pages_str: String = store
            .pages
            .iter()
            .map(|p| format!("[[{}]] — {}", p.frontmatter.id, p.frontmatter.title))
            .collect::<Vec<_>>()
            .join("\n");

        // 6. Regenerate each affected page with update context
        let total = affected_ids.len();
        let mut pages_updated = 0;
        for (i, page_id) in affected_ids.iter().enumerate() {
            let existing_page = store.pages.iter().find(|p| &p.frontmatter.id == page_id);
            let existing_page = match existing_page {
                Some(p) => p.clone(),
                None => continue,
            };

            eprintln!(
                "Updating page {}/{}: {}...",
                i + 1,
                total,
                existing_page.frontmatter.title
            );

            let updated = self
                .update_page(
                    &existing_page,
                    &diff_markdown,
                    &all_pages_str,
                    &git_ref,
                    &timestamp,
                )
                .await?;
            let updated_id = existing_page.frontmatter.id.clone();
            store.upsert_page(updated);
            store.save_incremental(&updated_id)?;
            pages_updated += 1;
        }

        // 7. Detect uncovered changed files and plan new pages
        let mut pages_created = 0;
        {
            let covered_files: HashSet<&str> = store
                .pages
                .iter()
                .flat_map(|p| p.frontmatter.source_files.iter().map(|s| s.as_str()))
                .collect();
            let uncovered: Vec<String> = changed_set
                .iter()
                .filter(|f| !covered_files.contains(f.as_str()))
                .cloned()
                .collect();

            if !uncovered.is_empty() {
                eprintln!(
                    "Found {} uncovered changed files, planning new pages...",
                    uncovered.len()
                );
                match self.plan_incremental(&uncovered, &store.pages).await {
                    Ok(plan) => {
                        // Apply assignments: add files to existing pages
                        for assignment in &plan.assignments {
                            if let Some(page) = store
                                .pages
                                .iter_mut()
                                .find(|p| p.frontmatter.id == assignment.page_id)
                            {
                                if !page.frontmatter.source_files.contains(&assignment.file) {
                                    page.frontmatter.source_files.push(assignment.file.clone());
                                    // Re-generate this page if not already updated
                                    if !affected_ids.contains(&assignment.page_id) {
                                        let existing = page.clone();
                                        eprintln!(
                                            "Updating page (new source file): {}...",
                                            existing.frontmatter.title
                                        );
                                        let updated = self
                                            .update_page(
                                                &existing,
                                                &diff_markdown,
                                                &all_pages_str,
                                                &git_ref,
                                                &timestamp,
                                            )
                                            .await?;
                                        let pid = existing.frontmatter.id.clone();
                                        store.upsert_page(updated);
                                        store.save_incremental(&pid)?;
                                        pages_updated += 1;
                                    }
                                }
                            }
                        }

                        // Generate new pages (capped at 3 by the prompt)
                        for new_plan in plan.new_pages.iter().take(3) {
                            eprintln!("Creating new page: {} ({})...", new_plan.title, new_plan.id);
                            let new_page = self
                                .generate_page(new_plan, &all_pages_str, &git_ref, &timestamp)
                                .await?;
                            let new_id = new_plan.id.clone();
                            store.upsert_page(new_page);
                            store.save_incremental(&new_id)?;
                            pages_created += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: incremental planning failed, skipping new page creation: {}",
                            e
                        );
                    }
                }
            }
        }

        // 8. Remove pages whose source files have all been deleted
        let deleted_set: HashSet<String> = structural_diff
            .files_removed
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let mut pages_removed = 0;
        store.pages.retain(|page| {
            if page.frontmatter.page_type == PageType::Index {
                return true;
            }
            if page.frontmatter.source_files.is_empty() {
                return true;
            }
            let all_deleted = page
                .frontmatter
                .source_files
                .iter()
                .all(|sf| deleted_set.contains(sf));
            if all_deleted {
                eprintln!(
                    "Removing page: {} (all source files deleted)",
                    page.frontmatter.id
                );
                pages_removed += 1;
                false
            } else {
                true
            }
        });

        // 9. Regenerate index page if anything changed
        if pages_updated > 0 || pages_removed > 0 || pages_created > 0 {
            eprintln!("Regenerating cross-reference index...");
            let non_index: Vec<WikiPage> = store
                .pages
                .iter()
                .filter(|p| p.frontmatter.page_type != PageType::Index)
                .cloned()
                .collect();
            let index_page = self
                .generate_index(&non_index, &git_ref, &timestamp)
                .await?;
            store.upsert_page(index_page);
        }

        // 10. Update manifest ref
        store.manifest.generated_at_ref = git_ref;
        store.manifest.generated_at = timestamp;

        Ok(UpdateResult {
            pages_updated,
            pages_removed,
            pages_created,
        })
    }

    /// Update a single wiki page with diff context.
    async fn update_page(
        &self,
        existing: &WikiPage,
        diff_markdown: &str,
        all_pages_str: &str,
        git_ref: &str,
        timestamp: &str,
    ) -> Result<WikiPage> {
        let mut ctx = String::new();
        let mut truncated = false;
        let limit = Self::PAGE_CONTEXT_CHAR_LIMIT;

        ctx.push_str("# Current Wiki Page Content\n\n");
        ctx.push_str(&existing.content);
        ctx.push_str("\n\n");

        // Budget-aware diff section: truncate if the context is already large
        ctx.push_str("# Structural Diff\n\n");
        if ctx.len() + diff_markdown.len() > limit {
            let remaining = limit.saturating_sub(ctx.len()).saturating_sub(500);
            if remaining > 0 {
                // Truncate on a line boundary to avoid mid-line cuts
                let safe = floor_char_boundary(diff_markdown, remaining);
                let trunc_point = diff_markdown[..safe].rfind('\n').unwrap_or(safe);
                ctx.push_str(&diff_markdown[..trunc_point]);
                ctx.push_str("\n\n... (diff truncated)\n");
            }
            truncated = true;
            eprintln!(
                "Warning: structural diff truncated to fit {}k char context budget",
                limit / 1000
            );
        } else {
            ctx.push_str(diff_markdown);
        }
        ctx.push_str("\n\n");

        // Budget-aware cross-references section
        ctx.push_str("# Other Wiki Pages\n");
        if ctx.len() + all_pages_str.len() > limit {
            let remaining = limit.saturating_sub(ctx.len()).saturating_sub(500);
            if remaining > 0 {
                let safe = floor_char_boundary(all_pages_str, remaining);
                let trunc_point = all_pages_str[..safe].rfind('\n').unwrap_or(safe);
                ctx.push_str(&all_pages_str[..trunc_point]);
                ctx.push_str("\n... (page list truncated)\n");
            }
            truncated = true;
        } else {
            ctx.push_str(all_pages_str);
        }
        ctx.push_str("\n\n");

        // Fresh structural data for source files
        ctx.push_str("# Current Source File Details\n\n");
        for source_path in &existing.frontmatter.source_files {
            if let Some(file) = self.find_file(source_path) {
                let mut section = String::new();
                section.push_str(&format!("## {}\n", source_path));
                section.push_str(&format!(
                    "Language: {}, Lines: {}, Size: {} bytes\n\n",
                    file.language.name(),
                    file.lines,
                    file.size,
                ));

                if !file.imports.is_empty() {
                    section.push_str("**Imports:**\n");
                    for imp in &file.imports {
                        section.push_str(&format!("- `{}`\n", imp.text));
                    }
                    section.push('\n');
                }
                section.push_str("**Declarations:**\n");
                format_declarations(&file.declarations, &mut section, 0);
                section.push('\n');

                if !try_push(&mut ctx, &section, limit) {
                    if !truncated {
                        warn_context_truncated("update", limit);
                    }
                    break;
                }
            }
        }

        let raw_content = self
            .llm
            .complete(
                prompts::update_system_prompt(),
                &[Message {
                    role: Role::User,
                    content: ctx,
                }],
            )
            .await
            .with_context(|| format!("Failed to update wiki page: {}", existing.frontmatter.id))?;

        let (content, contradictions) = extract_contradictions(&raw_content, timestamp);
        let links_to = extract_wiki_links(&content);

        Ok(WikiPage {
            frontmatter: Frontmatter {
                id: existing.frontmatter.id.clone(),
                title: existing.frontmatter.title.clone(),
                page_type: existing.frontmatter.page_type.clone(),
                source_files: existing.frontmatter.source_files.clone(),
                generated_at_ref: git_ref.to_string(),
                generated_at: timestamp.to_string(),
                links_to,
                covers: self.extract_covers(&existing.frontmatter.source_files),
                contradictions,
                failures: existing.frontmatter.failures.clone(),
            },
            content,
        })
    }

    /// Collect borrowed references to all FileIndex entries across workspace members.
    fn collect_all_file_refs(&self) -> Vec<&'a FileIndex> {
        self.workspace
            .members
            .iter()
            .flat_map(|m| m.index.files.iter())
            .collect()
    }

    /// Ask the LLM to plan the wiki structure from the structural index.
    async fn plan_structure(&self) -> Result<Vec<PagePlan>> {
        let context = self.build_planning_context();

        let response = self
            .llm
            .complete(
                prompts::plan_system_prompt(),
                &[Message {
                    role: Role::User,
                    content: context,
                }],
            )
            .await
            .context("Failed to get wiki plan from LLM")?;

        // Parse JSON from response (handle potential markdown fencing)
        let json_str = extract_json(&response);
        let plans: Vec<PagePlan> = serde_json::from_str(json_str).with_context(|| {
            let snippet: String = json_str.chars().take(200).collect();
            format!("Failed to parse wiki plan JSON from LLM. Response starts with: {snippet}")
        })?;

        if plans.is_empty() {
            anyhow::bail!("LLM returned an empty wiki plan — no pages to generate");
        }

        // Sanitize all page IDs and deduplicate
        let mut seen_ids = HashSet::new();
        let plans: Vec<PagePlan> = plans
            .into_iter()
            .map(|mut p| {
                p.id = page::sanitize_id(&p.id);
                p.source_files = self.resolve_source_files(&p.source_files);
                p
            })
            // Drop plans with empty IDs after sanitization
            .filter(|p| !p.id.is_empty())
            // Deduplicate by ID (keep first)
            .filter(|p| seen_ids.insert(p.id.clone()))
            .collect();

        if plans.is_empty() {
            anyhow::bail!(
                "All page IDs from LLM were empty after sanitization — cannot generate wiki"
            );
        }

        Ok(self.augment_plan_coverage(plans))
    }

    /// Plan what to do with source files not covered by any existing wiki page.
    /// Returns assignments to existing pages and plans for new pages.
    async fn plan_incremental(
        &self,
        uncovered_files: &[String],
        existing_pages: &[WikiPage],
    ) -> Result<IncrementalPlan> {
        let mut ctx = String::from("# Uncovered Source Files\n\n");

        for file_path in uncovered_files {
            ctx.push_str(&format!("## {}\n", file_path));
            // Add structural data if available
            if let Some(entries) = self.file_index.get(file_path.as_str()) {
                for (fi, _member) in entries {
                    ctx.push_str(&format!(
                        "Language: {:?}, Lines: {}, Declarations: {}\n",
                        fi.language,
                        fi.lines,
                        fi.declarations.len()
                    ));
                    for decl in &fi.declarations {
                        ctx.push_str(&format!("  - {} {:?}", decl.name, decl.kind));
                        if !decl.signature.is_empty() {
                            ctx.push_str(&format!(": {}", decl.signature));
                        }
                        ctx.push('\n');
                    }
                }
            }
            ctx.push('\n');
        }

        ctx.push_str("# Existing Wiki Pages\n\n");
        for page in existing_pages {
            if page.frontmatter.page_type == PageType::Index {
                continue;
            }
            ctx.push_str(&format!(
                "- {} (type: {}, id: {})\n  Source files: {}\n",
                page.frontmatter.title,
                page.frontmatter.page_type.as_str(),
                page.frontmatter.id,
                page.frontmatter.source_files.join(", "),
            ));
        }

        let response = self
            .llm
            .complete(
                prompts::incremental_plan_system_prompt(),
                &[Message {
                    role: Role::User,
                    content: ctx,
                }],
            )
            .await
            .context("Failed to get incremental plan from LLM")?;

        let json_str = extract_json(&response);
        let mut plan: IncrementalPlan = serde_json::from_str(json_str).with_context(|| {
            let snippet: String = json_str.chars().take(200).collect();
            format!("Failed to parse incremental plan JSON. Response starts with: {snippet}")
        })?;

        // Sanitize new page IDs
        for p in &mut plan.new_pages {
            p.id = page::sanitize_id(&p.id);
        }
        plan.new_pages.retain(|p| !p.id.is_empty());

        Ok(plan)
    }

    /// Generate a single wiki page.
    async fn generate_page(
        &self,
        plan: &PagePlan,
        all_pages_str: &str,
        git_ref: &str,
        timestamp: &str,
    ) -> Result<WikiPage> {
        let system = prompts::page_system_prompt(plan.page_type.as_str());

        let context = self.build_page_context(plan, all_pages_str);

        let content = self
            .llm
            .complete(
                &system,
                &[Message {
                    role: Role::User,
                    content: context,
                }],
            )
            .await
            .with_context(|| format!("Failed to generate wiki page: {}", plan.id))?;

        // Extract cross-references from the generated content
        let links_to = extract_wiki_links(&content);

        Ok(WikiPage {
            frontmatter: Frontmatter {
                id: plan.id.clone(),
                title: plan.title.clone(),
                page_type: plan.page_type.clone(),
                source_files: plan.source_files.clone(),
                generated_at_ref: git_ref.to_string(),
                generated_at: timestamp.to_string(),
                links_to,
                covers: self.extract_covers(&plan.source_files),
                contradictions: vec![],
                failures: vec![],
            },
            content,
        })
    }

    /// Generate the cross-reference index page.
    async fn generate_index(
        &self,
        pages: &[WikiPage],
        git_ref: &str,
        timestamp: &str,
    ) -> Result<WikiPage> {
        let mut ctx = String::from("Wiki pages to index:\n\n");
        for page in pages {
            ctx.push_str(&format!(
                "- [[{}]] (type: {:?}) — {}\n  Covers: {}\n",
                page.frontmatter.id,
                page.frontmatter.page_type,
                page.frontmatter.title,
                if page.frontmatter.covers.is_empty() {
                    "(general)".to_string()
                } else {
                    page.frontmatter.covers.join(", ")
                }
            ));
        }

        let content = self
            .llm
            .complete(
                prompts::index_system_prompt(),
                &[Message {
                    role: Role::User,
                    content: ctx,
                }],
            )
            .await
            .context("Failed to generate wiki index")?;

        let links_to: Vec<String> = pages.iter().map(|p| p.frontmatter.id.clone()).collect();

        Ok(WikiPage {
            frontmatter: Frontmatter {
                id: "index".to_string(),
                title: "Wiki Index".to_string(),
                page_type: PageType::Index,
                source_files: Vec::new(),
                generated_at_ref: git_ref.to_string(),
                generated_at: timestamp.to_string(),
                links_to,
                covers: Vec::new(),
                contradictions: vec![],
                failures: vec![],
            },
            content,
        })
    }

    /// Build the context string for the planning call (delegates to standalone fn).
    fn build_planning_context(&self) -> String {
        build_planning_context(self.workspace)
    }

    /// Approximate character limit for page context.  Same budget as the
    /// planning context — keeps the LLM input within typical context windows.
    const PAGE_CONTEXT_CHAR_LIMIT: usize = 100_000;

    /// Build the context for generating a single page.
    /// Truncates declaration details when the context exceeds
    /// [`Self::PAGE_CONTEXT_CHAR_LIMIT`], keeping file headers so the LLM
    /// still knows which files are involved.
    fn build_page_context(&self, plan: &PagePlan, all_pages_str: &str) -> String {
        let mut ctx = String::new();
        let mut truncated = false;
        let limit = Self::PAGE_CONTEXT_CHAR_LIMIT;

        ctx.push_str("# Page Plan\n");
        ctx.push_str(&format!("- ID: {}\n", plan.id));
        ctx.push_str(&format!("- Title: {}\n", plan.title));
        ctx.push_str(&format!("- Type: {:?}\n\n", plan.page_type));

        // All other wiki pages (for cross-referencing)
        ctx.push_str("# Other Wiki Pages\n");
        if ctx.len() + all_pages_str.len() + 2 > limit {
            let remaining = limit.saturating_sub(ctx.len()).saturating_sub(500);
            if remaining > 0 {
                let safe = floor_char_boundary(all_pages_str, remaining);
                let trunc_point = all_pages_str[..safe].rfind('\n').unwrap_or(safe);
                ctx.push_str(&all_pages_str[..trunc_point]);
                ctx.push_str("\n... (page list truncated)\n\n");
            }
            if !truncated {
                truncated = true;
                warn_context_truncated("page", limit);
            }
        } else {
            ctx.push_str(all_pages_str);
            ctx.push_str("\n\n");
        }

        // Structural data for source files
        ctx.push_str("# Source File Details\n\n");
        for source_path in &plan.source_files {
            if let Some(file) = self.find_file(source_path) {
                let mut section = String::new();
                section.push_str(&format!("## {}\n", source_path));
                section.push_str(&format!(
                    "Language: {}, Lines: {}, Size: {} bytes\n\n",
                    file.language.name(),
                    file.lines,
                    file.size,
                ));

                if !file.imports.is_empty() {
                    section.push_str("**Imports:**\n");
                    for imp in &file.imports {
                        section.push_str(&format!("- `{}`\n", imp.text));
                    }
                    section.push('\n');
                }

                // Declarations with full signatures
                section.push_str("**Declarations:**\n");
                format_declarations(&file.declarations, &mut section, 0);
                section.push('\n');

                if !try_push(&mut ctx, &section, limit) {
                    if !truncated {
                        warn_context_truncated("page", limit);
                    }
                    break;
                }
            }
        }

        ctx
    }

    /// Extract "kind:name" covers from source files.
    fn extract_covers(&self, source_files: &[String]) -> Vec<String> {
        let mut covers = Vec::new();
        for path in source_files {
            if let Some(file) = self.find_file(path) {
                for decl in &file.declarations {
                    if matches!(decl.visibility, Visibility::Public) {
                        covers.push(format!("{}:{}", decl.kind, decl.name));
                    }
                }
            }
        }
        covers
    }

    fn find_file(&self, path: &str) -> Option<&'a FileIndex> {
        // 1. Exact match by full path
        if let Some(entries) = self.file_index.get(path) {
            if let Some((fi, _)) = entries.first() {
                return Some(fi);
            }
        }

        // 2. Try matching by filename (for paths that differ in prefix)
        let path_buf = Path::new(path);
        if let Some(name) = path_buf.file_name() {
            let name_str = name.to_string_lossy();
            if let Some(entries) = self.file_index.get(name_str.as_ref()) {
                // Find the entry whose full path ends with the query path,
                // with a '/' boundary to avoid partial dir matches.
                for (fi, full_path) in entries {
                    if full_path == path {
                        return Some(fi);
                    }
                    if let Some(prefix) = full_path.strip_suffix(path) {
                        if prefix.is_empty() || prefix.ends_with('/') {
                            return Some(fi);
                        }
                    }
                }
            }
        }

        None
    }

    fn augment_plan_coverage(&self, mut plans: Vec<PagePlan>) -> Vec<PagePlan> {
        let mut covered: HashSet<String> = plans
            .iter()
            .flat_map(|plan| plan.source_files.iter().cloned())
            .collect();
        let uncovered: Vec<String> = self
            .workspace_paths
            .iter()
            .filter(|path| !covered.contains(path.as_str()))
            .cloned()
            .collect();

        if uncovered.is_empty() {
            return plans;
        }

        eprintln!(
            "Warning: wiki plan left {} files uncovered; repairing coverage automatically",
            uncovered.len()
        );

        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for path in uncovered {
            grouped
                .entry(self.coverage_group_key(&path))
                .or_default()
                .push(path);
        }

        let architecture_idx = plans
            .iter()
            .position(|plan| plan.page_type == PageType::Architecture);
        let mut used_ids: HashSet<String> = plans.iter().map(|plan| plan.id.clone()).collect();

        let mut groups: Vec<(String, Vec<String>)> = grouped.into_iter().collect();
        groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

        for (group_key, mut files) in groups {
            files.sort();

            if let Some(existing_idx) = self.find_best_plan_for_group(&plans, &group_key) {
                for file in files {
                    if covered.insert(file.clone()) {
                        plans[existing_idx].source_files.push(file);
                    }
                }
                plans[existing_idx].source_files.sort();
                plans[existing_idx].source_files.dedup();
                continue;
            }

            let group_lines: usize = files
                .iter()
                .filter_map(|path| self.find_file(path))
                .map(|file| file.lines)
                .sum();

            if files.len() >= 3 || group_lines >= 500 {
                let page_id = self.unique_generated_page_id(&group_key, &mut used_ids);
                plans.push(PagePlan {
                    id: page_id,
                    page_type: PageType::Module,
                    title: format!("{} Module", self.humanize_group_key(&group_key)),
                    source_files: files.clone(),
                });
                covered.extend(files);
                continue;
            }

            if let Some(idx) = architecture_idx {
                for file in files {
                    if covered.insert(file.clone()) {
                        plans[idx].source_files.push(file);
                    }
                }
                plans[idx].source_files.sort();
                plans[idx].source_files.dedup();
            }
        }

        let before = plans.len();
        plans.retain(|plan| plan.page_type == PageType::Index || !plan.source_files.is_empty());
        if plans.len() != before {
            eprintln!(
                "Warning: dropped {} empty wiki plan pages after coverage repair",
                before - plans.len()
            );
        }

        plans
    }

    fn find_best_plan_for_group(&self, plans: &[PagePlan], group_key: &str) -> Option<usize> {
        plans
            .iter()
            .enumerate()
            .filter(|(_, plan)| plan.page_type != PageType::Index)
            .map(|(idx, plan)| {
                let score = plan
                    .source_files
                    .iter()
                    .filter(|path| self.coverage_group_key(path) == group_key)
                    .count();
                (idx, score)
            })
            .filter(|(_, score)| *score > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(idx, _)| idx)
    }

    fn coverage_group_key(&self, path: &str) -> String {
        if let Some(member) = self.owning_member(path) {
            if member.relative_path != Path::new(".") {
                return member.relative_path.to_string_lossy().to_string();
            }
        }

        let parts: Vec<String> = Path::new(path)
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect();

        match parts.as_slice() {
            [first, second, ..] if first == "extensions" => format!("{first}/{second}"),
            [first, second, ..] if first == "review-ui" => format!("{first}/{second}"),
            [first, ..] => first.clone(),
            [] => "misc".to_string(),
        }
    }

    fn owning_member(&self, path: &str) -> Option<&crate::model::MemberIndex> {
        self.workspace
            .members
            .iter()
            .filter(|member| {
                let rel = member.relative_path.to_string_lossy();
                rel == "." || path == rel || path.starts_with(&format!("{rel}/"))
            })
            .max_by_key(|member| member.relative_path.components().count())
    }

    fn unique_generated_page_id(&self, group_key: &str, used_ids: &mut HashSet<String>) -> String {
        let base = page::sanitize_id(&format!("mod-{}", group_key.replace('/', "-")));
        let mut candidate = if base.is_empty() {
            "mod-coverage".to_string()
        } else {
            base
        };
        let mut suffix = 2usize;
        while !used_ids.insert(candidate.clone()) {
            candidate = format!(
                "{}-{}",
                candidate.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-'),
                suffix
            );
            suffix += 1;
        }
        candidate
    }

    fn humanize_group_key(&self, group_key: &str) -> String {
        group_key
            .split('/')
            .map(|part| {
                part.split(['-', '_'])
                    .filter(|s| !s.is_empty())
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn resolve_source_files(&self, source_files: &[String]) -> Vec<String> {
        let mut resolved = Vec::new();
        let mut seen = HashSet::new();

        for source in source_files {
            let source = source.trim();
            if source.is_empty() {
                continue;
            }

            for path in self.match_source_pattern(source) {
                if seen.insert(path.clone()) {
                    resolved.push(path);
                }
            }
        }

        resolved
    }

    fn match_source_pattern(&self, pattern: &str) -> Vec<String> {
        for candidate in self.source_pattern_candidates(pattern) {
            let has_glob = candidate.contains('*')
                || candidate.contains('?')
                || candidate.contains('[')
                || candidate.contains('{');

            if !candidate.contains('/') && !has_glob {
                let matches: Vec<String> = self
                    .workspace_paths
                    .iter()
                    .filter(|path| *path == &candidate || path.ends_with(&format!("/{candidate}")))
                    .cloned()
                    .collect();
                if !matches.is_empty() {
                    return matches;
                }
                continue;
            }

            if self.workspace_paths.iter().any(|path| path == &candidate) {
                return vec![candidate];
            }

            let mut matches = Vec::new();
            let matcher = compile_source_glob(&candidate);

            for path in &self.workspace_paths {
                let is_match = if let Some(matcher) = &matcher {
                    matcher.is_match(path)
                } else if candidate.contains('/') {
                    path == &candidate
                } else {
                    path == &candidate || path.ends_with(&format!("/{candidate}"))
                };

                if is_match {
                    matches.push(path.clone());
                }
            }

            if !matches.is_empty() {
                return matches;
            }
        }

        eprintln!(
            "Warning: wiki plan source entry did not match indexed files: {}",
            pattern
        );
        Vec::new()
    }

    fn source_pattern_candidates(&self, pattern: &str) -> Vec<String> {
        let mut candidates = vec![pattern.to_string()];

        for member in &self.workspace.members {
            let prefix = format!("{}/", member.name);
            if let Some(rest) = pattern.strip_prefix(&prefix) {
                let mapped = if member.relative_path == Path::new(".") {
                    rest.to_string()
                } else {
                    format!("{}/{}", member.relative_path.to_string_lossy(), rest)
                };
                if !candidates.contains(&mapped) {
                    candidates.push(mapped);
                }
            }
        }

        candidates
    }
}

// ---------------------------------------------------------------------------
// Standalone context builders (used by MCP tools without LlmClient)
// ---------------------------------------------------------------------------

/// Approximate character limit for the planning context.  100k chars ≈
/// 25-30k tokens — well within common LLM context windows while leaving
/// room for the system prompt and response.
const PLANNING_CONTEXT_CHAR_LIMIT: usize = 100_000;

/// Build a planning context string from the workspace structural index.
/// This provides the codebase overview needed to plan wiki pages.
pub(crate) fn build_planning_context(workspace: &WorkspaceIndex) -> String {
    let mut ctx = String::new();
    let mut truncated = false;
    let limit = PLANNING_CONTEXT_CHAR_LIMIT;

    ctx.push_str("# Codebase Structural Index\n\n");

    'members: for member in &workspace.members {
        if workspace.members.len() > 1 {
            let header = format!("## Workspace member: {}\n\n", member.name);
            if !try_push(&mut ctx, &header, limit) {
                if !truncated {
                    truncated = true;
                    warn_context_truncated("planning", limit);
                }
                break 'members;
            }
        }

        // Directory tree
        let mut tree_section = String::from("### Directory Tree\n```\n");
        format_tree(&member.index.tree, &mut tree_section);
        tree_section.push_str("```\n\n");
        if !try_push(&mut ctx, &tree_section, limit) {
            if !truncated {
                truncated = true;
                warn_context_truncated("planning", limit);
            }
            break 'members;
        }

        // Per-file summaries (compact)
        if !try_push(&mut ctx, "### Files\n\n", limit) {
            if !truncated {
                truncated = true;
                warn_context_truncated("planning", limit);
            }
            break 'members;
        }
        for file in &member.index.files {
            let path = file.path.to_string_lossy();
            let decl_count = count_declarations(&file.declarations);
            let public_count = count_public(&file.declarations);

            let summary = format!(
                "**{}** ({}, {} lines, {} decls, {} public)\n",
                path,
                file.language.name(),
                file.lines,
                decl_count,
                public_count,
            );
            if !try_push(&mut ctx, &summary, limit) {
                if !truncated {
                    truncated = true;
                    warn_context_truncated("planning", limit);
                }
                break 'members;
            }

            // List top-level declarations (name + kind only for planning)
            for decl in &file.declarations {
                let mut line = format!("  - {} `{}`", decl.kind, decl.name,);
                if !decl.children.is_empty() {
                    line.push_str(&format!(" ({} children)", decl.children.len()));
                }
                line.push('\n');
                if !try_push(&mut ctx, &line, limit) {
                    if !truncated {
                        truncated = true;
                        warn_context_truncated("planning", limit);
                    }
                    break 'members;
                }
            }

            if !try_push(&mut ctx, "\n", limit) {
                if !truncated {
                    truncated = true;
                    warn_context_truncated("planning", limit);
                }
                break 'members;
            }
        }
    }

    // Stats
    let stats_header = format!(
        "### Stats\n- Total files: {}\n- Total lines: {}\n",
        workspace.stats.total_files, workspace.stats.total_lines,
    );
    if try_push(&mut ctx, &stats_header, limit) {
        for (lang, count) in &workspace.stats.languages {
            let line = format!("- {}: {} files\n", lang, count);
            if !try_push(&mut ctx, &line, limit) {
                if !truncated {
                    warn_context_truncated("planning", limit);
                }
                break;
            }
        }
    }

    ctx
}

fn try_push(out: &mut String, text: &str, limit: usize) -> bool {
    if out.len() + text.len() > limit {
        return false;
    }
    out.push_str(text);
    true
}

fn compile_source_glob(pattern: &str) -> Option<globset::GlobMatcher> {
    if !pattern.contains('*')
        && !pattern.contains('?')
        && !pattern.contains('[')
        && !pattern.contains('{')
    {
        return None;
    }

    let effective = if !pattern.contains('/') {
        format!("**/{pattern}")
    } else {
        pattern.to_string()
    };

    GlobBuilder::new(&effective)
        .literal_separator(true)
        .build()
        .ok()
        .map(|glob| glob.compile_matcher())
}

fn warn_context_truncated(kind: &str, limit: usize) {
    eprintln!(
        "Warning: {} context exceeds {}k chars, truncating remaining content",
        kind,
        limit / 1000
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_tree(entries: &[TreeEntry], out: &mut String) {
    for entry in entries {
        let indent = "  ".repeat(entry.depth);
        let suffix = if entry.is_dir { "/" } else { "" };
        out.push_str(&format!("{}{}{}\n", indent, entry.path, suffix));
    }
}

fn format_declarations(decls: &[Declaration], out: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    for decl in decls {
        let vis = match decl.visibility {
            Visibility::Public => "pub ",
            Visibility::PublicCrate => "pub(crate) ",
            Visibility::Private => "",
        };
        out.push_str(&format!(
            "{}- {} {}{}`{}`",
            indent,
            decl.kind,
            vis,
            if decl.is_async { "async " } else { "" },
            decl.signature,
        ));
        if let Some(ref doc) = decl.doc_comment {
            let short = doc.lines().next().unwrap_or("").trim();
            if !short.is_empty() {
                let truncated = match short.char_indices().nth(100) {
                    Some((idx, _)) => format!("{}...", &short[..idx]),
                    None => short.to_string(),
                };
                out.push_str(&format!(" — {}", truncated));
            }
        }
        out.push('\n');

        if !decl.children.is_empty() {
            format_declarations(&decl.children, out, depth + 1);
        }
    }
}

fn count_declarations(decls: &[Declaration]) -> usize {
    let mut count = decls.len();
    for d in decls {
        count += count_declarations(&d.children);
    }
    count
}

fn count_public(decls: &[Declaration]) -> usize {
    let mut count = 0;
    for d in decls {
        if matches!(d.visibility, Visibility::Public) {
            count += 1;
        }
        count += count_public(&d.children);
    }
    count
}

/// Get the current HEAD commit hash.
fn current_git_ref(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .context("Failed to run git rev-parse HEAD")?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok("unknown".to_string())
    }
}

/// Extract JSON content from an LLM response that might be wrapped in markdown
/// fencing or preceded by preamble text.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    // 1. Markdown fenced block (```json ... ``` or ``` ... ```)
    if let Some(after) = trimmed.strip_prefix("```json") {
        if let Some(end) = after.rfind("```") {
            return after[..end].trim();
        }
    }
    if let Some(after) = trimmed.strip_prefix("```") {
        if let Some(end) = after.rfind("```") {
            return after[..end].trim();
        }
    }
    // 2. Find raw JSON array boundaries (handles preamble text from LLMs)
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }
    trimmed
}

/// Find the largest byte index ≤ `max` that falls on a char boundary.
pub(crate) fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Extract structured contradictions from an LLM update response.
///
/// Looks for a `<!-- CONTRADICTIONS [...] -->` HTML comment block at the end of
/// the content. Returns the cleaned content (block removed) and parsed
/// contradictions. If the block is missing or malformed, returns the content
/// as-is with an empty Vec.
fn extract_contradictions(content: &str, timestamp: &str) -> (String, Vec<Contradiction>) {
    let marker_start = "<!-- CONTRADICTIONS";
    let marker_end = "-->";

    let Some(start_pos) = content.find(marker_start) else {
        return (content.to_string(), vec![]);
    };

    let after_marker = &content[start_pos + marker_start.len()..];
    let Some(end_pos) = after_marker.find(marker_end) else {
        // Malformed block (no closing `-->`): strip the broken marker to avoid
        // leaking raw HTML into wiki pages.
        return (content[..start_pos].trim_end().to_string(), vec![]);
    };

    let json_str = after_marker[..end_pos].trim();
    let cleaned = content[..start_pos].trim_end().to_string();

    // Parse the JSON array of contradiction objects
    #[derive(Deserialize)]
    struct RawContradiction {
        description: String,
        source: String,
    }

    match serde_json::from_str::<Vec<RawContradiction>>(json_str) {
        Ok(raw) => {
            let contradictions = raw
                .into_iter()
                .map(|r| Contradiction {
                    description: r.description,
                    source: r.source,
                    detected_at: timestamp.to_string(),
                    resolved_at: None,
                })
                .collect();
            (cleaned, contradictions)
        }
        Err(e) => {
            eprintln!("Warning: failed to parse contradictions block: {}", e);
            (cleaned, vec![])
        }
    }
}

/// Extract [[page-id]] wiki links from content, sanitizing each link.
/// Skips links inside fenced code blocks.
pub(crate) fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut in_code_block = false;

    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        let mut rest = line;
        while let Some(start) = rest.find("[[") {
            let after = &rest[start + 2..];
            if let Some(end) = after.find("]]") {
                let raw = &after[..end];
                let sanitized = page::sanitize_id(raw);
                if !sanitized.is_empty() && !links.contains(&sanitized) {
                    links.push(sanitized);
                }
                rest = &after[end + 2..];
            } else {
                break;
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::languages::Language;
    use crate::model::declarations::{DeclKind, Declaration, Visibility};
    use crate::model::{CodebaseIndex, FileIndex, IndexStats, MemberIndex, WorkspaceIndex};
    use crate::workspace::WorkspaceKind;

    #[test]
    fn test_extract_json_plain() {
        let input = r#"[{"id": "test"}]"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn test_extract_json_fenced() {
        let input = "```json\n[{\"id\": \"test\"}]\n```";
        assert_eq!(extract_json(input), "[{\"id\": \"test\"}]");
    }

    #[test]
    fn test_extract_json_with_preamble() {
        let input = "Here is the plan:\n[{\"id\": \"test\"}]";
        assert_eq!(extract_json(input), "[{\"id\": \"test\"}]");
    }

    #[test]
    fn test_extract_json_with_preamble_and_trailing() {
        let input = "Sure! Here's the JSON:\n[{\"id\": \"a\"}, {\"id\": \"b\"}]\nHope this helps!";
        assert_eq!(extract_json(input), "[{\"id\": \"a\"}, {\"id\": \"b\"}]");
    }

    #[test]
    fn test_floor_char_boundary_ascii() {
        assert_eq!(floor_char_boundary("hello", 3), 3);
        assert_eq!(floor_char_boundary("hello", 10), 5);
        assert_eq!(floor_char_boundary("hello", 0), 0);
    }

    #[test]
    fn test_floor_char_boundary_multibyte() {
        // "café": c(1) a(1) f(1) é(2) = 5 bytes
        let s = "café";
        assert_eq!(s.len(), 5);
        assert_eq!(floor_char_boundary(s, 3), 3); // "caf"
        // Byte 4 is inside the 2-byte 'é', rounds down to 3
        assert_eq!(floor_char_boundary(s, 4), 3);
        assert_eq!(floor_char_boundary(s, 5), 5); // full string
        assert_eq!(floor_char_boundary(s, 10), 5); // beyond end
    }

    #[test]
    fn test_build_planning_context_respects_hard_limit() {
        let declarations: Vec<Declaration> = (0..150)
            .map(|i| {
                Declaration::new(
                    DeclKind::Function,
                    format!("very_long_function_name_{i:03}"),
                    format!("fn very_long_function_name_{i:03}()"),
                    Visibility::Public,
                    i + 1,
                )
            })
            .collect();

        let files: Vec<FileIndex> = (0..400)
            .map(|i| FileIndex {
                path: PathBuf::from(format!("src/module_{i:03}.rs")),
                language: Language::Rust,
                size: 10_000,
                lines: 500,
                imports: Vec::new(),
                declarations: declarations.clone(),
            })
            .collect();

        let index = CodebaseIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            generated_at: "now".to_string(),
            tree: Vec::new(),
            stats: IndexStats {
                total_files: files.len(),
                total_lines: files.iter().map(|f| f.lines).sum(),
                languages: HashMap::from([("rust".to_string(), files.len())]),
                duration_ms: 0,
            },
            files,
        };

        let workspace = WorkspaceIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            workspace_kind: WorkspaceKind::None,
            generated_at: "now".to_string(),
            stats: index.stats.clone(),
            members: vec![MemberIndex {
                name: "project".to_string(),
                relative_path: PathBuf::from("."),
                index,
            }],
        };

        let context = build_planning_context(&workspace);
        assert!(context.len() <= PLANNING_CONTEXT_CHAR_LIMIT);
    }

    #[test]
    fn test_resolve_source_files_expands_globs_and_filenames() {
        let files = vec![
            FileIndex {
                path: PathBuf::from("Cargo.toml"),
                language: Language::Toml,
                size: 100,
                lines: 10,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("crates/core/Cargo.toml"),
                language: Language::Toml,
                size: 100,
                lines: 10,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("apps/web/src/app.ts"),
                language: Language::TypeScript,
                size: 100,
                lines: 10,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("apps/web/src/view.jsx"),
                language: Language::JavaScript,
                size: 100,
                lines: 10,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
        ];

        let index = CodebaseIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            generated_at: "now".to_string(),
            tree: Vec::new(),
            stats: IndexStats {
                total_files: files.len(),
                total_lines: files.iter().map(|f| f.lines).sum(),
                languages: HashMap::from([
                    ("toml".to_string(), 2),
                    ("typescript".to_string(), 1),
                    ("javascript".to_string(), 1),
                ]),
                duration_ms: 0,
            },
            files,
        };

        let workspace = WorkspaceIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            workspace_kind: WorkspaceKind::None,
            generated_at: "now".to_string(),
            stats: index.stats.clone(),
            members: vec![MemberIndex {
                name: "project".to_string(),
                relative_path: PathBuf::from("."),
                index,
            }],
        };

        let llm = crate::llm::LlmClient::from_command("cat".to_string(), None);
        let generator = WikiGenerator::new(&llm, &workspace);
        let resolved = generator.resolve_source_files(&[
            "Cargo.toml".to_string(),
            "apps/web/src/**/*.{ts,jsx}".to_string(),
        ]);

        assert_eq!(
            resolved,
            vec![
                "Cargo.toml".to_string(),
                "crates/core/Cargo.toml".to_string(),
                "apps/web/src/app.ts".to_string(),
                "apps/web/src/view.jsx".to_string(),
            ]
        );
    }

    #[test]
    fn test_augment_plan_coverage_creates_page_for_uncovered_group() {
        let files = vec![
            FileIndex {
                path: PathBuf::from("apps/api/src/main.rs"),
                language: Language::Rust,
                size: 100,
                lines: 50,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("extensions/pi-permission-system/index.ts"),
                language: Language::TypeScript,
                size: 100,
                lines: 200,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("extensions/pi-permission-system/src/common.ts"),
                language: Language::TypeScript,
                size: 100,
                lines: 180,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
            FileIndex {
                path: PathBuf::from("extensions/pi-permission-system/src/status.ts"),
                language: Language::TypeScript,
                size: 100,
                lines: 170,
                imports: Vec::new(),
                declarations: Vec::new(),
            },
        ];

        let index = CodebaseIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            generated_at: "now".to_string(),
            tree: Vec::new(),
            stats: IndexStats {
                total_files: files.len(),
                total_lines: files.iter().map(|f| f.lines).sum(),
                languages: HashMap::new(),
                duration_ms: 0,
            },
            files,
        };

        let workspace = WorkspaceIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            workspace_kind: WorkspaceKind::None,
            generated_at: "now".to_string(),
            stats: index.stats.clone(),
            members: vec![MemberIndex {
                name: "project".to_string(),
                relative_path: PathBuf::from("."),
                index,
            }],
        };

        let llm = crate::llm::LlmClient::from_command("cat".to_string(), None);
        let generator = WikiGenerator::new(&llm, &workspace);
        let plans = generator.augment_plan_coverage(vec![
            PagePlan {
                id: "architecture".to_string(),
                page_type: PageType::Architecture,
                title: "Architecture".to_string(),
                source_files: vec!["apps/api/src/main.rs".to_string()],
            },
            PagePlan {
                id: "index".to_string(),
                page_type: PageType::Index,
                title: "Index".to_string(),
                source_files: Vec::new(),
            },
        ]);

        let extension_plan = plans
            .iter()
            .find(|plan| plan.id.starts_with("mod-extensions-pi-permission-system"))
            .expect("expected coverage repair page");
        assert_eq!(extension_plan.source_files.len(), 3);
    }

    #[test]
    fn test_resolve_source_files_accepts_member_name_prefixes() {
        let files = vec![FileIndex {
            path: PathBuf::from("src/main.rs"),
            language: Language::Rust,
            size: 100,
            lines: 10,
            imports: Vec::new(),
            declarations: Vec::new(),
        }];

        let index = CodebaseIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            generated_at: "now".to_string(),
            tree: Vec::new(),
            stats: IndexStats {
                total_files: files.len(),
                total_lines: 10,
                languages: HashMap::new(),
                duration_ms: 0,
            },
            files,
        };

        let workspace = WorkspaceIndex {
            root: PathBuf::from("/tmp/project"),
            root_name: "project".to_string(),
            workspace_kind: WorkspaceKind::Cargo,
            generated_at: "now".to_string(),
            stats: index.stats.clone(),
            members: vec![MemberIndex {
                name: "agenctl".to_string(),
                relative_path: PathBuf::from("."),
                index,
            }],
        };

        let llm = crate::llm::LlmClient::from_command("cat".to_string(), None);
        let generator = WikiGenerator::new(&llm, &workspace);
        let resolved = generator.resolve_source_files(&["agenctl/src/main.rs".to_string()]);
        assert_eq!(resolved, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn test_extract_wiki_links() {
        let content = "See [[architecture]] and [[mod-parser]] for details. Also [[architecture]].";
        let links = extract_wiki_links(content);
        assert_eq!(links, vec!["architecture", "mod-parser"]);
    }

    #[test]
    fn test_extract_wiki_links_sanitizes() {
        let content = "See [[MCP-Server]] and [[../../etc/passwd]] and [[]] end.";
        let links = extract_wiki_links(content);
        assert_eq!(links, vec!["mcp-server", "etcpasswd"]);
    }

    #[test]
    fn test_extract_wiki_links_skips_code_blocks() {
        let content = "See [[architecture]] for details.\n\n```\nExample: [[not-a-link]]\n```\n\nAlso [[mod-parser]].";
        let links = extract_wiki_links(content);
        assert_eq!(links, vec!["architecture", "mod-parser"]);
    }

    #[test]
    fn test_extract_contradictions_with_block() {
        let content = r#"# Updated Module

Some updated content here.

<!-- CONTRADICTIONS
[{"description": "Wiki stated sync channels but code now uses async", "source": "src/mcp/mod.rs:383"}]
-->"#;
        let (cleaned, contradictions) = extract_contradictions(content, "2026-04-06T10:00:00Z");
        assert_eq!(cleaned, "# Updated Module\n\nSome updated content here.");
        assert_eq!(contradictions.len(), 1);
        assert_eq!(contradictions[0].source, "src/mcp/mod.rs:383");
        assert_eq!(contradictions[0].detected_at, "2026-04-06T10:00:00Z");
        assert!(contradictions[0].resolved_at.is_none());
    }

    #[test]
    fn test_extract_contradictions_multiple() {
        let content = r#"# Module

Content.

<!-- CONTRADICTIONS
[{"description": "A changed", "source": "a.rs:1"}, {"description": "B changed", "source": "b.rs:2"}]
-->"#;
        let (_, contradictions) = extract_contradictions(content, "2026-04-06T10:00:00Z");
        assert_eq!(contradictions.len(), 2);
        assert_eq!(contradictions[0].source, "a.rs:1");
        assert_eq!(contradictions[1].source, "b.rs:2");
    }

    #[test]
    fn test_extract_contradictions_no_block() {
        let content = "# Module\n\nJust normal content.";
        let (cleaned, contradictions) = extract_contradictions(content, "2026-04-06T10:00:00Z");
        assert_eq!(cleaned, content);
        assert!(contradictions.is_empty());
    }

    #[test]
    fn test_extract_contradictions_malformed_json() {
        let content = "# Module\n\n<!-- CONTRADICTIONS\nnot valid json\n-->";
        let (cleaned, contradictions) = extract_contradictions(content, "2026-04-06T10:00:00Z");
        assert_eq!(cleaned, "# Module");
        assert!(contradictions.is_empty());
    }

    #[test]
    fn test_extract_contradictions_unclosed_marker() {
        // LLM wrote the marker start but no closing `-->` — should strip the
        // broken marker from the content rather than leaking raw HTML.
        let content = "# Module\n\nSome content.\n\n<!-- CONTRADICTIONS\n[{\"description\": \"x\", \"source\": \"y\"}]";
        let (cleaned, contradictions) = extract_contradictions(content, "2026-04-06T10:00:00Z");
        assert_eq!(cleaned, "# Module\n\nSome content.");
        assert!(contradictions.is_empty());
    }
}
