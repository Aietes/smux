use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::process::CommandRunner;
use crate::ui::DisplayStyle;
use crate::util;

/// One repository row from `gh repo list`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepoListing {
    pub name_with_owner: String,
    pub private: bool,
    pub updated_at: String,
    pub description: String,
}

/// List the authenticated user's repositories plus those of any configured
/// extra owners, most recently updated first, deduplicated by full name.
///
/// gh renders the rows as TSV via `--template`, so no JSON parsing is needed
/// on our side.
pub fn list_repos(
    runner: &Arc<dyn CommandRunner>,
    extra_owners: &[String],
) -> Result<Vec<RepoListing>> {
    let mut seen = HashSet::new();
    let mut repos = Vec::new();

    let mut owners: Vec<Option<&str>> = vec![None];
    owners.extend(extra_owners.iter().map(|owner| Some(owner.as_str())));

    for owner in owners {
        for repo in list_owner_repos(runner, owner)? {
            if seen.insert(repo.name_with_owner.clone()) {
                repos.push(repo);
            }
        }
    }

    // `gh repo list` already sorts by recency per owner; merge the owner
    // lists into one recency order.
    repos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(repos)
}

fn list_owner_repos(
    runner: &Arc<dyn CommandRunner>,
    owner: Option<&str>,
) -> Result<Vec<RepoListing>> {
    let mut args = vec!["repo".to_owned(), "list".to_owned()];
    if let Some(owner) = owner {
        args.push(owner.to_owned());
    }
    args.extend([
        "--limit".to_owned(),
        "1000".to_owned(),
        "--json".to_owned(),
        "nameWithOwner,visibility,updatedAt,description".to_owned(),
        "--template".to_owned(),
        r#"{{range .}}{{printf "%s\t%s\t%s\t%s\n" .nameWithOwner .visibility .updatedAt .description}}{{end}}"#.to_owned(),
    ]);

    let output = runner.run_capture("gh", &args).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "browsing repositories requires the GitHub CLI (gh); install it or pass a repository URL"
            )
        } else {
            anyhow::Error::new(error).context("failed to execute gh repo list")
        }
    })?;

    if !output.status.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh repo list failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("gh output was not valid utf-8")?;
    Ok(stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            Some(RepoListing {
                name_with_owner: parts.next()?.to_owned(),
                private: parts.next()? == "PRIVATE",
                updated_at: parts.next()?.to_owned(),
                description: parts.next().unwrap_or_default().to_owned(),
            })
        })
        .collect())
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_DIM: &str = "\x1b[2m";
const LOCK_ICON: &str = "\u{f023}";

/// Render one repo as a picker row: `owner/name`, a red (private) or green
/// (public) lock, the relative update time, and the dimmed description.
pub fn repo_label(style: DisplayStyle, repo: &RepoListing, now_epoch: u64) -> String {
    let age = util::relative_time_ago(&repo.updated_at, now_epoch).unwrap_or_default();
    if style.icons_enabled() {
        let lock_color = if repo.private { ANSI_RED } else { ANSI_GREEN };
        format!(
            "{:<50}  {lock_color}{LOCK_ICON}{ANSI_RESET}  {ANSI_BLUE}{age:>8}{ANSI_RESET}  {ANSI_DIM}{}{ANSI_RESET}",
            repo.name_with_owner, repo.description
        )
    } else {
        let visibility = if repo.private { "private" } else { "public " };
        format!(
            "{:<50}  {visibility}  {age:>8}  {}",
            repo.name_with_owner, repo.description
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{RepoListing, list_repos, repo_label};
    use crate::process::{CommandOutput, CommandStatus, FakeCommandRunner};
    use crate::ui::DisplayStyle;
    use std::sync::Arc;

    fn ok(stdout: &[u8]) -> std::io::Result<CommandOutput> {
        Ok(CommandOutput {
            status: CommandStatus {
                success: true,
                code: Some(0),
            },
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        })
    }

    #[test]
    fn lists_and_merges_owner_repos() {
        let runner = Arc::new(FakeCommandRunner::new());
        runner.push_capture(ok(
            b"me/tool\tPUBLIC\t2026-07-01T10:00:00Z\tA tool\nme/shared\tPRIVATE\t2026-06-01T10:00:00Z\t\n",
        ));
        runner.push_capture(ok(
            b"org/app\tPRIVATE\t2026-07-05T10:00:00Z\tThe app\nme/shared\tPRIVATE\t2026-06-01T10:00:00Z\t\n",
        ));
        let runner_dyn: Arc<dyn crate::process::CommandRunner> = runner.clone();

        let repos = list_repos(&runner_dyn, &["org".to_owned()]).expect("listing should succeed");

        // Newest first across owners, duplicates dropped.
        assert_eq!(
            repos
                .iter()
                .map(|repo| repo.name_with_owner.as_str())
                .collect::<Vec<_>>(),
            vec!["org/app", "me/tool", "me/shared"]
        );
        assert!(repos[0].private);
        assert_eq!(repos[1].description, "A tool");

        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, "gh");
        assert_eq!(recorded[0].args[..2], ["repo", "list"]);
        // The extra owner is queried by name.
        assert_eq!(recorded[1].args[..3], ["repo", "list", "org"]);
    }

    #[test]
    fn labels_show_visibility_and_age_without_icons() {
        let style = DisplayStyle::from_icon_mode(crate::config::IconMode::Never);
        let repo = RepoListing {
            name_with_owner: "me/tool".to_owned(),
            private: true,
            updated_at: "2026-07-05T12:00:00Z".to_owned(),
            description: "A tool".to_owned(),
        };
        // now = 2026-07-08T12:00:00Z
        let label = repo_label(style, &repo, 1_783_512_000);
        assert!(label.contains("me/tool"));
        assert!(label.contains("private"));
        assert!(label.contains("3d ago"));
        assert!(label.contains("A tool"));
    }
}
