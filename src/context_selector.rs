// unused import
use std::process::Command;

/// Smart context selector — picks relevant files based on task description
pub struct ContextSelector {
    pub project_dir: std::path::PathBuf,
}

impl ContextSelector {
    pub fn new(project_dir: std::path::PathBuf) -> Self {
        Self { project_dir }
    }

    /// Analyze a task description and return relevant file paths
    pub fn select_context(&self, task_description: &str) -> Result<Vec<String>, String> {
        let keywords = self.extract_keywords(task_description);
        let mut relevant_files = Vec::new();

        // Strategy 1: Search by keywords
        for keyword in &keywords {
            if keyword.len() < 3 { continue; }
            if let Ok(files) = self.search_files(keyword) {
                for file in files {
                    if !relevant_files.contains(&file) {
                        relevant_files.push(file);
                    }
                }
            }
        }

        // Strategy 2: If task mentions specific files, include them
        for keyword in &keywords {
            if keyword.ends_with(".rs") || keyword.ends_with(".toml") || keyword.ends_with(".md") {
                let path = self.project_dir.join(keyword);
                if path.exists() && !relevant_files.contains(keyword) {
                    relevant_files.push(keyword.clone());
                }
            }
        }

        // Strategy 3: Include files related to mentioned modules
        for keyword in &keywords {
            if let Ok(files) = self.find_module_deps(keyword) {
                for file in files {
                    if !relevant_files.contains(&file) {
                        relevant_files.push(file);
                    }
                }
            }
        }

        // If we found nothing, fall back to a broad file listing
        if relevant_files.is_empty() {
            relevant_files = self.list_project_files()?;
        }

        Ok(relevant_files)
    }

    /// Extract meaningful keywords from task description
    fn extract_keywords(&self, description: &str) -> Vec<String> {
        let mut keywords: Vec<String> = Vec::new();
        let stop_words = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been",
            "have", "has", "had", "do", "does", "did", "will", "would",
            "could", "should", "may", "might", "can", "shall", "to", "of",
            "in", "for", "on", "with", "at", "by", "from", "as", "into",
            "through", "during", "before", "after", "above", "below",
            "between", "out", "off", "over", "under", "again", "further",
            "then", "once", "here", "there", "all", "each", "every",
            "both", "few", "more", "most", "other", "some", "such", "no",
            "nor", "not", "only", "own", "same", "so", "than", "too",
            "very", "just", "because", "but", "and", "or", "if", "while",
            "this", "that", "these", "those", "it", "its", "please", "fix",
            "add", "update", "change", "remove", "delete", "implement",
            "refactor", "improve", "make", "need", "want", "help",
        ];

        let cleaned = description
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '_' && c != '/' && c != '-', " ");

        for word in cleaned.split_whitespace() {
            let word = word.trim();
            if word.len() < 3 { continue; }
            if stop_words.contains(&word) { continue; }
            if word.chars().all(|c| c.is_ascii_digit()) { continue; }
            if !keywords.contains(&word.to_string()) {
                keywords.push(word.to_string());
            }
        }

        keywords
    }

    /// Search for files matching a keyword using grep
    fn search_files(&self, keyword: &str) -> Result<Vec<String>, String> {
        let output = Command::new("grep")
            .args(["-rnl", "--binary-files=without-match", keyword, "."])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("grep failed: {}", e))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let result = String::from_utf8_lossy(&output.stdout);
        Ok(result
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Find files related to a module name (e.g., "patch" -> "src/patch.rs")
    fn find_module_deps(&self, module_name: &str) -> Result<Vec<String>, String> {
        let mut files = Vec::new();

        // Check common source directories
        let candidates = [
            format!("src/{}.rs", module_name),
            format!("src/{}/mod.rs", module_name),
            format!("src/{}.rs", module_name.trim_end_matches('_')),
        ];

        for candidate in &candidates {
            let path = self.project_dir.join(candidate);
            if path.exists() {
                files.push(candidate.clone());
            }
        }

        Ok(files)
    }

    /// Fallback: list all Rust source files in the project
    fn list_project_files(&self) -> Result<Vec<String>, String> {
        let output = Command::new("find")
            .args([".", "-name", "*.rs", "-not", "-path", "./target/*", "-type", "f"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("find failed: {}", e))?;

        let result = String::from_utf8_lossy(&output.stdout);
        Ok(result
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .take(30) // Limit to 30 files
            .collect())
    }

    /// Build a contextual prompt for the LLM based on selected files
    pub fn build_context_prompt(&self, files: &[String], max_tokens: usize) -> Result<String, String> {
        let mut prompt = String::new();
        let mut total_size = 0;

        for file_path in files {
            let full_path = self.project_dir.join(file_path);
            if !full_path.exists() { continue; }

            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Error reading {}: {}", file_path, e))?;

            let file_size = content.len();

            // Include file header always
            let header = format!("\n--- {} ---\n", file_path);
            prompt.push_str(&header);
            total_size += header.len();

            // Truncate large files
            if file_size > max_tokens / 2 {
                let preview = content.lines().take(50).collect::<Vec<_>>().join("\n");
                prompt.push_str(&preview);
                prompt.push_str(&format!("\n... ({} lines total, showing first 50)", content.lines().count()));
                total_size += preview.len();
            } else {
                prompt.push_str(&content);
                total_size += file_size;
            }

            if total_size > max_tokens {
                prompt.push_str("\n... (context truncated)");
                break;
            }
        }

        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_keywords_basic() {
        let selector = ContextSelector::new(PathBuf::from("."));
        let keywords = selector.extract_keywords("Fix the cargo warnings in the project");
        assert!(keywords.contains(&"cargo".to_string()));
        assert!(keywords.contains(&"warnings".to_string()));
        assert!(keywords.contains(&"project".to_string()));
        // "the" and "in" are stop words
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"in".to_string()));
    }

    #[test]
    fn test_extract_keywords_with_file() {
        let selector = ContextSelector::new(PathBuf::from("."));
        let keywords = selector.extract_keywords("Update src/main.rs to add a new module");
        assert!(keywords.contains(&"src/main.rs".to_string()));
        assert!(keywords.contains(&"module".to_string()));
    }

    #[test]
    fn test_extract_keywords_short_words_ignored() {
        let selector = ContextSelector::new(PathBuf::from("."));
        let keywords = selector.extract_keywords("Fix it now");
        // "fix", "now" are 3+ chars
        assert!(!keywords.contains(&"it".to_string()));
    }

    #[test]
    fn test_extract_keywords_empty() {
        let selector = ContextSelector::new(PathBuf::from("."));
        let keywords = selector.extract_keywords("");
        assert!(keywords.is_empty());
    }
}
