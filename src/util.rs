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
    let trimmed = value.trim();
    let trimmed = trimmed.strip_suffix(".toml").unwrap_or(trimmed);

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

/// Render a child-process exit code for error messages: "exit code 1", or
/// "termination by signal" when the process died without one.
pub fn exit_status_label(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit code {code}"),
        None => "termination by signal".to_owned(),
    }
}

/// Render a value as a quoted JSON string. The `--json` payloads are flat
/// lists of names and paths, so escaping by hand beats a serde_json
/// dependency.
pub fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// One process-wide lock for every test that mutates environment variables
/// (HOME, TMUX, XDG_CONFIG_HOME, ...). Unit tests across all modules run in
/// the same parallel test binary, so per-module locks don't exclude each
/// other and would eventually flake.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        expand_tilde_path, json_string, path_to_config_string, sanitize_session_name,
        validated_project_name, validated_session_name,
    };
    use std::path::Path;

    #[test]
    fn escapes_json_strings() {
        assert_eq!(json_string("plain"), r#""plain""#);
        assert_eq!(json_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(json_string("tab\there"), r#""tab\there""#);
        assert_eq!(json_string("bell\u{7}"), r#""bell\u0007""#);
        assert_eq!(json_string("ünïcode"), r#""ünïcode""#);
    }

    #[test]
    fn expands_tilde_paths() {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
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
    fn strips_only_a_single_toml_suffix() {
        assert_eq!(
            validated_project_name("example.toml.toml").expect("project name should validate"),
            "example.toml"
        );
    }

    #[test]
    fn collapses_home_for_config_paths() {
        let _guard = crate::util::test_env::lock();
        unsafe {
            std::env::set_var("HOME", "/Users/dev");
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
