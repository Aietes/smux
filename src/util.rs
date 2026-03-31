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
    let expanded = expand_tilde_path(path);
    expanded
        .canonicalize()
        .with_context(|| format!("failed to resolve path {}", expanded.display()))
}

pub fn expand_and_normalize_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde_path(path);
    expanded
        .canonicalize()
        .with_context(|| format!("failed to resolve path {}", expanded.display()))
}

pub fn expand_and_absolutize_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde_path(path);

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

pub fn validated_project_name(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_end_matches(".toml");

    if trimmed.is_empty() {
        bail!("project name resolved to an empty value");
    }

    if trimmed == "." || trimmed == ".." {
        bail!("project name must not be . or ..");
    }

    if trimmed.contains(std::path::MAIN_SEPARATOR)
        || trimmed.contains('/')
        || trimmed.contains('\\')
    {
        bail!("project name must not contain path separators");
    }

    Ok(trimmed.to_owned())
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

pub fn path_to_config_string(path: &Path) -> Result<String> {
    let path = path_to_string(path)?;
    if let Ok(home) = std::env::var("HOME") {
        if path == home {
            return Ok("~".to_owned());
        }

        if let Some(stripped) = path.strip_prefix(&(home.clone() + "/")) {
            return Ok(format!("~/{stripped}"));
        }
    }

    Ok(path)
}

pub fn expand_tilde_path(path: &Path) -> PathBuf {
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
    use super::{
        expand_tilde_path, path_to_config_string, sanitize_session_name, validated_project_name,
        validated_session_name,
    };
    use std::path::Path;
    use std::sync::Mutex;

    static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn expands_tilde_paths() {
        let _guard = HOME_ENV_LOCK.lock().expect("home env lock should work");
        unsafe {
            std::env::set_var("HOME", "/Users/stefan");
        }

        let path = expand_tilde_path(Path::new("~/code"));
        assert!(path.is_absolute());

        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn strips_invalid_session_characters() {
        assert_eq!(sanitize_session_name(" hello/world "), "hello_world");
    }

    #[test]
    fn rejects_empty_session_names() {
        assert!(validated_session_name("...").is_err());
    }

    #[test]
    fn rejects_project_names_with_path_separators() {
        assert!(validated_project_name("foo/bar").is_err());
    }

    #[test]
    fn strips_toml_suffix_from_project_name() {
        assert_eq!(
            validated_project_name("example.toml").expect("project name should validate"),
            "example"
        );
    }

    #[test]
    fn collapses_home_for_config_paths() {
        let _guard = HOME_ENV_LOCK.lock().expect("home env lock should work");
        unsafe {
            std::env::set_var("HOME", "/Users/stefan");
        }

        let home = std::env::var("HOME").expect("HOME should be set");
        let path = Path::new(&home).join("code").join("smux");
        assert_eq!(
            path_to_config_string(&path).expect("path should render"),
            "~/code/smux"
        );

        unsafe {
            std::env::remove_var("HOME");
        }
    }
}
