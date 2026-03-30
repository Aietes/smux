use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn command_available(command: &str) -> bool {
    which::which(command).is_ok()
}

pub fn inside_tmux() -> bool {
    std::env::var_os("TMUX").is_some()
}

pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    expanded
        .canonicalize()
        .with_context(|| format!("failed to resolve path {}", expanded.display()))
}

pub fn expand_and_normalize_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    expanded
        .canonicalize()
        .with_context(|| format!("failed to resolve path {}", expanded.display()))
}

pub fn expand_and_absolutize_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path);

    if expanded.exists() {
        return expanded
            .canonicalize()
            .with_context(|| format!("failed to resolve path {}", expanded.display()));
    }

    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        let current_dir =
            std::env::current_dir().context("failed to resolve current working directory")?;
        Ok(current_dir.join(expanded))
    }
}

pub fn session_name_from_path(path: &Path) -> Result<String> {
    let basename = path
        .file_name()
        .and_then(OsStr::to_str)
        .context("path does not have a valid terminal directory name")?;

    let sanitized = sanitize_session_name(basename);

    if sanitized.is_empty() {
        bail!(
            "could not derive a valid tmux session name from {}",
            path.display()
        );
    }

    Ok(sanitized)
}

pub fn validated_session_name(value: &str) -> Result<String> {
    let sanitized = sanitize_session_name(value);

    if sanitized.is_empty() {
        bail!("session name resolved to an empty value");
    }

    Ok(sanitized)
}

pub fn sanitize_session_name(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| match character {
            ' ' | ':' | '.' | '\t' | '\n' | '\r' => '_',
            character if is_tmux_safe(character) => character,
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

pub fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .context("path was not valid utf-8")
}

fn expand_tilde(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if !text.starts_with("~/") && text != "~" {
        return path.to_path_buf();
    }

    let Some(home) = std::env::var_os("HOME") else {
        return path.to_path_buf();
    };

    if text == "~" {
        return PathBuf::from(home);
    }

    let suffix = text.trim_start_matches("~/");
    PathBuf::from(home).join(suffix)
}

fn is_tmux_safe(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::{expand_tilde, sanitize_session_name, validated_session_name};
    use std::path::Path;

    #[test]
    fn expands_tilde_paths() {
        let path = expand_tilde(Path::new("~/code"));
        assert!(path.is_absolute());
    }

    #[test]
    fn strips_invalid_session_characters() {
        assert_eq!(sanitize_session_name(" hello/world "), "hello_world");
    }

    #[test]
    fn rejects_empty_session_names() {
        assert!(validated_session_name("...").is_err());
    }
}
