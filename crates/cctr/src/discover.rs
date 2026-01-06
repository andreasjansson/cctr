use crate::error::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Suite {
    pub path: PathBuf,
    pub name: String,
    pub has_fixture: bool,
    pub has_setup: bool,
    pub has_teardown: bool,
}

impl Suite {
    pub fn new(path: PathBuf, base_dir: &Path) -> Self {
        let name = path
            .strip_prefix(base_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());

        let has_fixture = path.join("fixture").is_dir();
        let has_setup = path.join("_setup.txt").is_file();
        let has_teardown = path.join("_teardown.txt").is_file();

        Self {
            path,
            name,
            has_fixture,
            has_setup,
            has_teardown,
        }
    }

    pub fn corpus_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.path)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map_or(false, |ext| ext == "txt")
                    && !p
                        .file_name()
                        .map_or(false, |n| n.to_string_lossy().starts_with('_'))
            })
            .collect();
        files.sort();
        files
    }
}

pub fn discover_suites(root: &Path) -> Result<Vec<Suite>> {
    let mut suite_dirs: HashSet<PathBuf> = HashSet::new();

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }
        if path.extension().map_or(true, |ext| ext != "txt") {
            continue;
        }
        if path
            .file_name()
            .map_or(false, |n| n.to_string_lossy().starts_with('_'))
        {
            continue;
        }
        if let Ok(rel_path) = path.strip_prefix(root) {
            if rel_path.components().any(|c| c.as_os_str() == "fixture") {
                continue;
            }
        }

        if let Some(parent) = path.parent() {
            suite_dirs.insert(parent.to_path_buf());
        }
    }

    let mut suites: Vec<Suite> = suite_dirs
        .into_iter()
        .map(|p| Suite::new(p, root))
        .collect();

    suites.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(suites)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_discover_single_suite() {
        let tmp = TempDir::new().unwrap();
        let suite_dir = tmp.path().join("suite1");
        fs::create_dir_all(&suite_dir).unwrap();
        create_test_file(&suite_dir, "test.txt", "===\ntest\n===\necho hi\n---\nhi\n");

        let suites = discover_suites(tmp.path()).unwrap();
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].name, "suite1");
    }

    #[test]
    fn test_discover_nested_suites() {
        let tmp = TempDir::new().unwrap();
        let suite1 = tmp.path().join("lang/python");
        let suite2 = tmp.path().join("lang/go");
        fs::create_dir_all(&suite1).unwrap();
        fs::create_dir_all(&suite2).unwrap();
        create_test_file(&suite1, "test.txt", "===\ntest\n===\necho hi\n---\nhi\n");
        create_test_file(&suite2, "test.txt", "===\ntest\n===\necho hi\n---\nhi\n");

        let suites = discover_suites(tmp.path()).unwrap();
        assert_eq!(suites.len(), 2);
    }

    #[test]
    fn test_skip_fixture_directory() {
        let tmp = TempDir::new().unwrap();
        let suite_dir = tmp.path().join("suite1");
        let fixture_dir = suite_dir.join("fixture");
        fs::create_dir_all(&fixture_dir).unwrap();
        create_test_file(&suite_dir, "test.txt", "===\ntest\n===\necho hi\n---\nhi\n");
        create_test_file(
            &fixture_dir,
            "data.txt",
            "===\nfake\n===\nfake\n---\nfake\n",
        );

        let suites = discover_suites(tmp.path()).unwrap();
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].corpus_files().len(), 1);
    }

    #[test]
    fn test_suite_detects_setup_teardown() {
        let tmp = TempDir::new().unwrap();
        let suite_dir = tmp.path().join("suite1");
        fs::create_dir_all(&suite_dir).unwrap();
        create_test_file(&suite_dir, "test.txt", "===\ntest\n===\necho hi\n---\nhi\n");
        create_test_file(
            &suite_dir,
            "_setup.txt",
            "===\nsetup\n===\necho setup\n---\n",
        );

        let suites = discover_suites(tmp.path()).unwrap();
        assert!(suites[0].has_setup);
        assert!(!suites[0].has_teardown);
    }
}
