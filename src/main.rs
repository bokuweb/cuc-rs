mod csharp;
mod editorconfig;
mod formatter;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use editorconfig::EditorConfig;
use formatter::{format_text, FormatOptions};
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "A small .editorconfig-driven fast cleanup prototype."
)]
struct Cli {
    /// Files or directories to format.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Path to .editorconfig.
    #[arg(short, long, default_value = ".editorconfig")]
    config: PathBuf,

    /// Check whether files are already formatted without writing changes.
    #[arg(long)]
    check: bool,

    /// Print changed file paths.
    #[arg(long)]
    list: bool,

    /// Enable basic text cleanup derived from .editorconfig.
    #[arg(long)]
    text: bool,

    /// Include leading indentation conversion from indent_style/indent_size.
    ///
    /// This is intentionally opt-in because it can disturb aligned multi-line text.
    #[arg(long)]
    indent: bool,

    /// Enable experimental C# formatter passes.
    #[arg(long)]
    csharp: bool,

    /// Enable only experimental C# newline formatter passes.
    #[arg(long)]
    csharp_newlines: bool,

    /// Skip files larger than this many bytes.
    #[arg(long, default_value_t = 2 * 1024 * 1024)]
    max_bytes: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli
        .config
        .canonicalize()
        .with_context(|| format!("failed to locate {}", cli.config.display()))?;
    let root = config_path
        .parent()
        .context("config path has no parent directory")?
        .to_path_buf();
    let config = EditorConfig::load(&config_path)?;

    let mut visited = 0usize;
    let mut changed = Vec::new();

    for path in collect_files(&cli.paths)? {
        let Some(relative_path) = pathdiff(&path, &root) else {
            continue;
        };
        let properties = config.properties_for(&relative_path);
        if properties.is_empty() {
            continue;
        }

        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to stat {}", path.display()))?;
        if metadata.len() > cli.max_bytes {
            continue;
        }

        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let Some(input) = decode_utf8(&bytes) else {
            continue;
        };
        let options = FormatOptions::from_properties(
            &properties,
            cli.text,
            cli.indent,
            cli.csharp,
            cli.csharp_newlines,
            &relative_path,
        );
        let output = format_text(&input, options);
        visited += 1;

        if output.as_bytes() != bytes {
            changed.push(path.clone());
            if !cli.check {
                fs::write(&path, output.as_bytes())
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        }
    }

    if cli.list {
        for path in &changed {
            println!("{}", path.display());
        }
    }

    if cli.check && !changed.is_empty() {
        anyhow::bail!(
            "{} of {} checked files need cleanup",
            changed.len(),
            visited
        );
    }

    eprintln!(
        "{} checked, {} {}",
        visited,
        changed.len(),
        if cli.check { "would change" } else { "changed" }
    );

    Ok(())
}

fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.canonicalize()?);
            continue;
        }

        for entry in WalkDir::new(path)
            .into_iter()
            .filter_entry(|entry| !is_skipped_dir(entry.path()))
        {
            let entry = entry?;
            if entry.file_type().is_file() {
                files.push(entry.path().canonicalize()?);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".claude"
                    | ".git"
                    | ".idea"
                    | ".vs"
                    | ".worktrees"
                    | "bin"
                    | "node_modules"
                    | "obj"
                    | "packages"
                    | "target"
            )
        })
}

fn pathdiff(path: &Path, root: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

fn decode_utf8(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(formatter::UTF8_BOM) {
        String::from_utf8(rest.to_vec())
            .ok()
            .map(|text| format!("{}{}", formatter::BOM_MARK, text))
    } else {
        String::from_utf8(bytes.to_vec()).ok()
    }
}
