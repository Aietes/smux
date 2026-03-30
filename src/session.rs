use std::path::Path;

use anyhow::Result;

use crate::tmux::Tmux;
use crate::util;

pub fn connect_path(tmux: &Tmux, path: &Path, override_name: Option<&str>) -> Result<()> {
    let normalized = util::normalize_path(path)?;
    let session_name = match override_name {
        Some(name) => util::validated_session_name(name)?,
        None => util::session_name_from_path(&normalized)?,
    };

    if tmux.has_session(&session_name)? {
        return tmux.switch_or_attach(&session_name);
    }

    tmux.create_session(&session_name, &normalized)?;
    tmux.switch_or_attach(&session_name)
}

pub fn switch_existing(tmux: &Tmux, session: &str) -> Result<()> {
    let session = util::validated_session_name(session)?;
    tmux.ensure_session_exists(&session)?;
    tmux.switch_or_attach(&session)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::util;

    #[test]
    fn sanitizes_session_names() {
        assert_eq!(util::sanitize_session_name("my app"), "my_app");
        assert_eq!(util::sanitize_session_name("api:v1"), "api_v1");
        assert_eq!(util::sanitize_session_name("foo.bar"), "foo_bar");
    }

    #[test]
    fn derives_session_name_from_path() -> Result<()> {
        let tempdir = tempfile::tempdir()?;
        let directory = tempdir.path().join("my-project");
        std::fs::create_dir(&directory)?;

        let session = util::session_name_from_path(&directory)?;
        assert_eq!(session, "my-project");

        Ok(())
    }
}
