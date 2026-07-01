use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The bundled Claude Code skill, embedded so `smux skill` always emits the
/// version that matches this binary. The source of truth is
/// `assets/smux-skill.md`.
pub const SKILL_MD: &str = include_str!("../assets/smux-skill.md");

/// Write the skill to `<dir>/SKILL.md`, creating `dir` if needed, and return the
/// written path. With no `dir`, print the skill to stdout and return `None`.
pub fn write_skill(dir: Option<&Path>) -> Result<Option<PathBuf>> {
    match dir {
        Some(dir) => {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create skill directory {}", dir.display()))?;
            let path = dir.join("SKILL.md");
            fs::write(&path, SKILL_MD)
                .with_context(|| format!("failed to write skill to {}", path.display()))?;
            Ok(Some(path))
        }
        None => {
            print!("{SKILL_MD}");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_skill_is_well_formed() {
        assert!(super::SKILL_MD.starts_with("---\nname: smux-config\n"));
        assert!(super::SKILL_MD.contains("## Troubleshooting"));
        assert!(super::SKILL_MD.contains("## Errors → fixes"));
    }
}
