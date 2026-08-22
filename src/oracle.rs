use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::editorconfig::EditorConfig;
use crate::formatter::{format_text, FormatOptions, BOM_MARK, UTF8_BOM};

pub struct CompareOptions {
    pub include_text: bool,
    pub include_indent: bool,
    pub include_csharp: bool,
    pub include_csharp_newlines: bool,
    pub list: bool,
    pub check: bool,
}

pub fn compare_commits(
    repository: &Path,
    commits: &[String],
    config: &EditorConfig,
    options: CompareOptions,
) -> Result<()> {
    let mut exact = 0usize;
    let mut missed = 0usize;
    let mut partial = 0usize;

    for commit in commits {
        let parent = git_text(repository, &["rev-parse", &format!("{commit}^")])?;
        let historical_config = git_bytes(
            repository,
            &["show", &format!("{}:.editorconfig", parent.trim())],
        )
        .ok()
        .and_then(|bytes| decode_utf8(&bytes))
        .and_then(|source| EditorConfig::parse(&source).ok());
        let commit_config = historical_config.as_ref().unwrap_or(config);
        let paths = git_text(
            repository,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                "--diff-filter=AM",
                commit,
            ],
        )?;

        for path in paths.lines() {
            let before = git_bytes(repository, &["show", &format!("{}:{path}", parent.trim())])?;
            let expected = git_bytes(repository, &["show", &format!("{commit}:{path}")])?;
            let Some(input) = decode_utf8(&before) else {
                continue;
            };
            let properties = commit_config.properties_for(Path::new(path));
            let candidate = format_text(
                &input,
                FormatOptions::from_properties(
                    &properties,
                    options.include_text,
                    options.include_indent,
                    options.include_csharp,
                    options.include_csharp_newlines,
                    true,
                    Path::new(path),
                ),
            );
            let candidate = candidate.as_bytes();

            let status = if candidate == expected {
                exact += 1;
                "exact"
            } else if candidate == before {
                missed += 1;
                "missed"
            } else {
                partial += 1;
                "partial"
            };
            if options.list {
                println!("[{status}] {commit} {path}");
            }
        }
    }

    let total = exact + missed + partial;
    eprintln!("oracle: {exact}/{total} exact, {partial} partial, {missed} missed");
    if options.check && exact != total {
        bail!("{} oracle files do not match cleanupcode", total - exact);
    }
    Ok(())
}

fn git_text(repository: &Path, args: &[&str]) -> Result<String> {
    let output = git_output(repository, args)?;
    String::from_utf8(output).context("git output is not UTF-8")
}

fn git_bytes(repository: &Path, args: &[&str]) -> Result<Vec<u8>> {
    git_output(repository, args)
}

fn git_output(repository: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn decode_utf8(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        String::from_utf8(rest.to_vec())
            .ok()
            .map(|text| format!("{BOM_MARK}{text}"))
    } else {
        String::from_utf8(bytes.to_vec()).ok()
    }
}
