use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap_complete::Shell;
use clap_complete::aot::{generate, generate_to};

use crate::cli::Cli;

pub fn generate_completions(shell: Shell, dir: Option<&Path>) -> Result<Option<PathBuf>> {
    let mut command = Cli::command();

    match dir {
        Some(dir) => {
            fs::create_dir_all(dir).with_context(|| {
                format!("failed to create completion directory {}", dir.display())
            })?;
            let path = generate_to(shell, &mut command, "smux", dir)
                .with_context(|| format!("failed to write completions to {}", dir.display()))?;
            Ok(Some(path))
        }
        None => {
            let mut stdout = io::stdout();
            generate(shell, &mut command, "smux", &mut stdout);
            Ok(None)
        }
    }
}

pub fn generate_man_pages(dir: Option<&Path>) -> Result<Option<Vec<PathBuf>>> {
    let command = Cli::command();

    match dir {
        Some(dir) => {
            fs::create_dir_all(dir).with_context(|| {
                format!("failed to create man page directory {}", dir.display())
            })?;
            let mut paths = Vec::new();
            write_man_tree(command, dir, &["smux".to_owned()], &mut paths)?;
            copy_static_man_page(dir, "smux-config.5", &mut paths)?;
            Ok(Some(paths))
        }
        None => {
            let mut buffer = Vec::new();
            clap_mangen::Man::new(command).render(&mut buffer)?;
            print!(
                "{}",
                String::from_utf8(buffer).context("generated man page was not valid utf-8")?
            );
            Ok(None)
        }
    }
}

fn write_man_tree(
    command: clap::Command,
    dir: &Path,
    lineage: &[String],
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let name = lineage.join("-");
    let static_name: &'static str = Box::leak(name.clone().into_boxed_str());
    let command_for_page = command.clone().name(static_name).bin_name(static_name);
    let mut buffer = Vec::new();
    clap_mangen::Man::new(command_for_page.clone()).render(&mut buffer)?;

    let path = dir.join(format!("{name}.1"));
    fs::write(&path, &buffer)
        .with_context(|| format!("failed to write man page {}", path.display()))?;
    paths.push(path);

    for subcommand in command_for_page
        .get_subcommands()
        .cloned()
        .collect::<Vec<_>>()
    {
        let mut next_lineage = lineage.to_vec();
        next_lineage.push(subcommand.get_name().to_owned());
        write_man_tree(subcommand, dir, &next_lineage, paths)?;
    }

    Ok(())
}

fn copy_static_man_page(dir: &Path, filename: &str, paths: &mut Vec<PathBuf>) -> Result<()> {
    let source = Path::new("docs").join(filename);
    let destination = dir.join(filename);
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to copy static man page {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    paths.push(destination);
    Ok(())
}
