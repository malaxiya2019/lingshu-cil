use anyhow::Result;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

/// Represents the context collected from the workspace
#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub working_dir: PathBuf,
    pub files: Vec<FileInfo>,
    pub total_lines: usize,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub relative_path: String,
    pub lines: usize,
    pub extension: String,
    pub size_bytes: u64,
}

pub struct ContextEngine {
    workspace: PathBuf,
    max_depth: usize,
    max_file_size: u64,       // max file size in bytes to read
    max_context_files: usize, // max files to include in context window
    ignored_patterns: Vec<String>,
    matcher: Arc<SkimMatcherV2>,
    cached_context: Option<WorkspaceContext>,
}

impl ContextEngine {
    pub fn new(workspace: PathBuf) -> Self {
        let ignored = vec![
            ".git".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
            "vendor".to_string(),
            ".DS_Store".to_string(),
            "__pycache__".to_string(),
            "*.pyc".to_string(),
            ".termpkg".to_string(),
        ];

        Self {
            workspace,
            max_depth: 8,
            max_file_size: 100 * 1024, // 100KB
            max_context_files: 20,
            ignored_patterns: ignored,
            matcher: Arc::new(SkimMatcherV2::default()),
            cached_context: None,
        }
    }

    pub fn set_workspace(&mut self, path: PathBuf) {
        self.workspace = path;
        self.cached_context = None;
    }

    pub fn workspace(&self) -> &PathBuf {
        &self.workspace
    }

    fn is_ignored(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.ignored_patterns
            .iter()
            .any(|p| path_str.contains(p) || path_str.ends_with(p.trim_start_matches('*')))
    }

    /// Scan the workspace and collect file information
    pub fn scan_workspace(&mut self) -> Result<WorkspaceContext> {
        let mut files = Vec::new();
        let mut total_lines = 0;

        for entry in WalkDir::new(&self.workspace)
            .max_depth(self.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let is_ignored = self.is_ignored(e.path());
                !is_ignored
            })
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let metadata = entry.metadata()?;
            if metadata.len() > self.max_file_size {
                continue;
            }

            let path = entry.path().to_path_buf();
            let relative = path
                .strip_prefix(&self.workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Count lines
            let content = std::fs::read_to_string(&path).ok();
            let lines = content.as_ref().map(|c| c.lines().count()).unwrap_or(0);
            total_lines += lines;

            files.push(FileInfo {
                path,
                relative_path: relative,
                lines,
                extension: ext,
                size_bytes: metadata.len(),
            });
        }

        // Sort by size (smallest first) then take up to max_context_files
        files.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes));
        files.truncate(self.max_context_files);

        let summary = format!(
            "📁 {} | {} files | {} lines | depth: {}",
            self.workspace.display(),
            files.len(),
            total_lines,
            self.max_depth,
        );

        let ctx = WorkspaceContext {
            working_dir: self.workspace.clone(),
            files,
            total_lines,
            summary,
        };

        self.cached_context = Some(ctx.clone());
        Ok(ctx)
    }

    /// Get smart context for a query: fuzzy-find relevant files
    pub fn context_for_query(&self, query: &str) -> Vec<FileInfo> {
        let ctx = match &self.cached_context {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut scored: Vec<(i64, &FileInfo)> = ctx
            .files
            .iter()
            .filter_map(|f| {
                let score = self
                    .matcher
                    .fuzzy_match(&f.relative_path, query)
                    .or_else(|| self.matcher.fuzzy_match(&f.extension, query))
                    .unwrap_or(0);
                if score > 0 {
                    Some((score, f))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(5); // top 5 relevant files

        // If no fuzzy matches, return the most recently modified files
        if scored.is_empty() {
            return ctx.files.iter().take(5).cloned().collect();
        }

        scored.into_iter().map(|(_, f)| f.clone()).collect()
    }

    /// Build a context prompt string for the LLM
    pub fn build_context_prompt(&self, query: &str) -> String {
        let ctx_summary = self
            .cached_context
            .as_ref()
            .map(|c| c.summary.clone())
            .unwrap_or_default();

        let relevant = self.context_for_query(query);
        let file_context: String = relevant
            .iter()
            .filter_map(|f| {
                let content = std::fs::read_to_string(&f.path).ok()?;
                let preview: String = content
                    .lines()
                    .take(50) // first 50 lines per file
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(format!(
                    "\n### File: {}\n```{}",
                    f.relative_path, f.extension
                ) + "\n" + &preview + "\n```\n")
            })
            .collect();

        format!(
            "[Workspace Context]\n{}\n\n[Relevant Files]\n{}\n\n[User Query]\n{}",
            ctx_summary, file_context, query
        )
    }

    /// Get a compact tree view of the workspace
    pub fn tree_view(&self) -> String {
        let ctx = match &self.cached_context {
            Some(c) => c,
            None => return "No context scanned yet.".to_string(),
        };

        let mut tree = String::new();
        tree.push_str(&format!("{}\n", ctx.summary));
        tree.push_str("──────────────────\n");

        for f in &ctx.files {
            let indent = f
                .relative_path
                .chars()
                .filter(|&c| c == '/' || c == '\\')
                .count();
            let prefix = if indent == 0 {
                "📄"
            } else {
                " ├"
            };
            tree.push_str(&format!(
                "{} {}\n",
                prefix,
                f.relative_path
            ));
        }

        tree
    }
}
