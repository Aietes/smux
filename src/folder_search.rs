use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::FolderSearchSettings;
use crate::util;

const SKIPPED_DIRECTORY_NAMES: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".git",
    ".direnv",
    ".cache",
];
const SKIPPED_ROOT_CHILD_NAMES: &[&str] = &["Library"];

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct FolderSearchResult {
    pub directories: Vec<String>,
    pub warnings: Vec<FolderSearchWarning>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FolderSearchWarning {
    pub root: String,
    pub message: String,
}

pub fn list_directories(settings: &FolderSearchSettings) -> FolderSearchResult {
    let mut result = FolderSearchResult::default();
    let mut seen = HashSet::new();

    for root in &settings.roots {
        let expanded = util::expand_tilde_path(Path::new(root));
        let Ok(root_path) = expanded.canonicalize() else {
            result.warnings.push(FolderSearchWarning {
                root: root.clone(),
                message: format!(
                    "failed to resolve folder search root {}",
                    expanded.display()
                ),
            });
            continue;
        };

        walk_directory(
            root,
            &root_path,
            0,
            settings.max_depth,
            settings.include_hidden,
            &mut seen,
            &mut result,
        );
    }

    result.directories.sort();
    result
}

fn walk_directory(
    root_label: &str,
    directory: &Path,
    depth: usize,
    max_depth: usize,
    include_hidden: bool,
    seen: &mut HashSet<PathBuf>,
    result: &mut FolderSearchResult,
) {
    if !seen.insert(directory.to_path_buf()) {
        return;
    }

    if let Ok(path) = util::path_to_string(directory) {
        result.directories.push(path);
    }

    if depth >= max_depth {
        return;
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            result.warnings.push(FolderSearchWarning {
                root: root_label.to_owned(),
                message: format!("failed to read {}: {error}", directory.display()),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        if should_skip_child(&path, depth, include_hidden) {
            continue;
        }

        let Ok(path) = path.canonicalize() else {
            continue;
        };
        walk_directory(
            root_label,
            &path,
            depth + 1,
            max_depth,
            include_hidden,
            seen,
            result,
        );
    }
}

fn should_skip_child(path: &Path, parent_depth: usize, include_hidden: bool) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    (!include_hidden && name.starts_with('.'))
        || SKIPPED_DIRECTORY_NAMES.contains(&name)
        || (parent_depth == 0 && SKIPPED_ROOT_CHILD_NAMES.contains(&name))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::FolderSearchSettings;

    #[test]
    fn finds_directories_under_configured_roots() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        fs::create_dir(tempdir.path().join("app")).expect("app dir should be created");
        fs::create_dir_all(tempdir.path().join("app").join("src"))
            .expect("nested dir should be created");

        let result = super::list_directories(&FolderSearchSettings {
            roots: vec![tempdir.path().display().to_string()],
            max_depth: 1,
            include_hidden: false,
        });

        assert!(result.warnings.is_empty());
        assert!(
            result
                .directories
                .contains(&tempdir.path().canonicalize().unwrap().display().to_string())
        );
        assert!(
            result.directories.contains(
                &tempdir
                    .path()
                    .join("app")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string()
            )
        );
        assert!(
            !result.directories.contains(
                &tempdir
                    .path()
                    .join("app")
                    .join("src")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string()
            )
        );
    }

    #[test]
    fn skips_hidden_directories_by_default() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        fs::create_dir(tempdir.path().join(".hidden")).expect("hidden dir should be created");

        let result = super::list_directories(&FolderSearchSettings {
            roots: vec![tempdir.path().display().to_string()],
            max_depth: 1,
            include_hidden: false,
        });

        assert!(
            !result.directories.contains(
                &tempdir
                    .path()
                    .join(".hidden")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string()
            )
        );
    }

    #[test]
    fn skips_common_heavy_directories() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        fs::create_dir(tempdir.path().join("target")).expect("target dir should be created");

        let result = super::list_directories(&FolderSearchSettings {
            roots: vec![tempdir.path().display().to_string()],
            max_depth: 1,
            include_hidden: true,
        });

        assert!(
            !result.directories.contains(
                &tempdir
                    .path()
                    .join("target")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string()
            )
        );
    }

    #[test]
    fn reports_missing_roots_without_failing() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let missing = tempdir.path().join("missing");

        let result = super::list_directories(&FolderSearchSettings {
            roots: vec![missing.display().to_string()],
            max_depth: 1,
            include_hidden: false,
        });

        assert!(result.directories.is_empty());
        assert_eq!(result.warnings.len(), 1);
    }
}
