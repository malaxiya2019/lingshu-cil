use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Represents the context collected from the workspace
#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub files: Vec<FileInfo>,
    pub total_lines: usize,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    #[allow(dead_code)]
    pub path: PathBuf,
    #[allow(dead_code)]
    pub relative_path: String,
#[allow(dead_code)]
    pub lines: usize,
    #[allow(dead_code)]
    pub extension: String,
    #[allow(dead_code)]
    pub size_bytes: u64,
}

pub struct ContextEngine {
    workspace: PathBuf,
    max_depth: usize,
    max_file_size: u64,
    max_context_files: usize,
    ignored_patterns: Vec<String>,
    cached_context: Option<WorkspaceContext>,
}

impl ContextEngine {
    pub fn new(workspace: PathBuf) -> Self {
        let ignored = vec![
            ".git".to_string(), "node_modules".to_string(), "target".to_string(),
            "vendor".to_string(), ".DS_Store".to_string(), "__pycache__".to_string(),
            "*.pyc".to_string(), ".termpkg".to_string(),
        ];
        Self {
            workspace,
            max_depth: 8,
            max_file_size: 100 * 1024,
            max_context_files: 20,
            ignored_patterns: ignored,
            cached_context: None,
        }
    }

    pub fn set_workspace(&mut self, path: PathBuf) {
        self.workspace = path;
        self.cached_context = None;
    }

    fn is_ignored(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.ignored_patterns.iter().any(|p| path_str.contains(p) || path_str.ends_with(p.trim_start_matches('*')))
    }

    /// Scan the workspace and collect file information
    pub fn scan_workspace(&mut self) -> Result<WorkspaceContext> {
        let mut files = Vec::new();
        let mut total_lines = 0;

        for entry in WalkDir::new(&self.workspace)
            .max_depth(self.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.is_ignored(e.path()))
        {
            let entry = entry?;
            if !entry.file_type().is_file() { continue; }
            let metadata = entry.metadata()?;
            if metadata.len() > self.max_file_size { continue; }

            let path = entry.path().to_path_buf();
            let relative = path.strip_prefix(&self.workspace).unwrap_or(&path)
                .to_string_lossy().to_string();
            let ext = path.extension().unwrap_or_default().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path).ok();
            let lines = content.as_ref().map(|c| c.lines().count()).unwrap_or(0);
            total_lines += lines;

            files.push(FileInfo {
                path, relative_path: relative, lines, extension: ext, size_bytes: metadata.len(),
            });
        }

        files.sort_by_key(|a| a.size_bytes);
        files.truncate(self.max_context_files);

        let summary = format!("{} | {} files | {} lines", self.workspace.display(), files.len(), total_lines);
        let ctx = WorkspaceContext { files, total_lines, summary };
        self.cached_context = Some(ctx.clone());
        Ok(ctx)
    }

}
