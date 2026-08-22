mod csharp;
mod editorconfig;
mod formatter;
mod oracle;
mod syntax;
mod xaml;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

    /// Git repository containing committed cleanupcode output to use as an oracle.
    #[arg(long)]
    compare_repo: Option<PathBuf>,

    /// Compare cuc output from COMMIT^ with the cleanup result in COMMIT.
    #[arg(long, requires = "compare_repo")]
    compare_commit: Vec<String>,
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
    let solution_scope = cli.paths.iter().any(|path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sln"))
    });

    if !cli.compare_commit.is_empty() {
        return oracle::compare_commits(
            cli.compare_repo
                .as_deref()
                .context("missing --compare-repo")?,
            &cli.compare_commit,
            &config,
            oracle::CompareOptions {
                include_text: cli.text,
                include_indent: cli.indent,
                include_csharp: cli.csharp,
                include_csharp_newlines: cli.csharp_newlines,
                list: cli.list,
                check: cli.check,
            },
        );
    }

    let mut visited = 0usize;
    let mut changed = Vec::new();
    let files = collect_files(&cli.paths)?;
    let interface_layout = if cli.csharp {
        let sources = files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
            })
            .filter_map(|path| fs::read(path).ok())
            .filter(|bytes| {
                bytes
                    .windows(b"interface ".len())
                    .any(|window| window == b"interface ")
            })
            .filter_map(|bytes| decode_utf8(&bytes))
            .map(|source| source.trim_start_matches(formatter::BOM_MARK).to_string())
            .collect::<Vec<_>>();
        Some(Arc::new(syntax::InterfaceLayout::from_sources(
            sources.iter().map(String::as_str),
        )))
    } else {
        None
    };

    for path in files {
        let Some(relative_path) = pathdiff(&path, &root) else {
            continue;
        };
        let properties = config.properties_for(&relative_path);
        if properties.is_empty() {
            continue;
        }
        if (cli.csharp || cli.csharp_newlines) && is_generated_csharp_file(&relative_path) {
            continue;
        }

        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to stat {}", path.display()))?;
        if metadata.len() > cli.max_bytes {
            if is_cleanup_text_file(&relative_path) {
                anyhow::bail!(
                    "refusing to silently skip cleanup text file larger than {} bytes: {}",
                    cli.max_bytes,
                    path.display()
                );
            }
            continue;
        }

        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let input = match decode_utf8(&bytes) {
            Some(input) => input,
            None if is_cleanup_text_file(&relative_path) => {
                anyhow::bail!(
                    "refusing to silently skip non-UTF-8 cleanup text file: {}",
                    path.display()
                );
            }
            None => continue,
        };
        let mut options = FormatOptions::from_properties(
            &properties,
            cli.text,
            cli.indent,
            cli.csharp,
            cli.csharp_newlines,
            solution_scope,
            &relative_path,
        );
        if let (Some(csharp), Some(layout)) = (&mut options.csharp, &interface_layout) {
            csharp.interface_layout = Some(Arc::clone(layout));
        }
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
            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sln"))
            {
                files.extend(collect_solution_files(path)?);
            } else {
                files.push(path.canonicalize()?);
            }
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

fn collect_solution_files(solution: &Path) -> Result<Vec<PathBuf>> {
    let source = fs::read_to_string(solution)
        .with_context(|| format!("failed to read {}", solution.display()))?;
    let solution_dir = solution
        .parent()
        .context("solution path has no parent directory")?;
    let mut files = Vec::new();

    for field in source.lines().flat_map(|line| line.split('"')) {
        if !field.trim_end().ends_with(".csproj") {
            continue;
        }
        let project_path = solution_dir.join(normalize_windows_path(field.trim()));
        files.extend(collect_project_files(&project_path)?);
    }
    Ok(files)
}

fn collect_project_files(project: &Path) -> Result<Vec<PathBuf>> {
    let source = fs::read_to_string(project)
        .with_context(|| format!("failed to read {}", project.display()))?;
    let project_dir = project
        .parent()
        .context("project path has no parent directory")?;
    let mut files = Vec::new();
    let mut rest = source.as_str();

    while let Some(start) = rest.find(" Include=\"") {
        rest = &rest[start + " Include=\"".len()..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let include = &rest[..end];
        if !include.contains(['*', '$']) {
            let path = project_dir.join(normalize_windows_path(include));
            if path.is_file() {
                files.push(path.canonicalize()?);
            }
        }
        rest = &rest[end + 1..];
    }

    let sdk_style = is_sdk_style_project(&source);
    let uses_default_compile_items = sdk_style
        && !has_false_msbuild_property(&source, "EnableDefaultItems")
        && !has_false_msbuild_property(&source, "EnableDefaultCompileItems");
    let uses_default_xaml_items = sdk_style
        && has_true_msbuild_property(&source, "UseWPF")
        && !has_false_msbuild_property(&source, "EnableDefaultItems")
        && !has_false_msbuild_property(&source, "EnableDefaultPageItems");
    let fallback_to_csharp_walk = !sdk_style && files.is_empty();
    if uses_default_compile_items || uses_default_xaml_items || fallback_to_csharp_walk {
        for entry in WalkDir::new(project_dir)
            .into_iter()
            .filter_entry(|entry| !is_skipped_dir(entry.path()))
        {
            let entry = entry?;
            let extension = entry.path().extension().and_then(|value| value.to_str());
            let is_default_compile = uses_default_compile_items
                && extension.is_some_and(|value| value.eq_ignore_ascii_case("cs"));
            let is_default_xaml = uses_default_xaml_items
                && extension.is_some_and(|value| value.eq_ignore_ascii_case("xaml"));
            let is_fallback_csharp = fallback_to_csharp_walk
                && extension.is_some_and(|value| value.eq_ignore_ascii_case("cs"));
            if entry.file_type().is_file()
                && (is_default_compile || is_default_xaml || is_fallback_csharp)
            {
                files.push(entry.path().canonicalize()?);
            }
        }
    }

    Ok(files)
}

fn is_sdk_style_project(source: &str) -> bool {
    let Some(project_start) = source.find("<Project") else {
        return false;
    };
    let Some(project_end) = source[project_start..].find('>') else {
        return false;
    };
    let project_tag = &source[project_start..project_start + project_end];
    project_tag.contains(" Sdk=\"")
        || project_tag.contains(" Sdk='")
        || source.contains("<Sdk Name=\"")
        || source.contains("<Sdk Name='")
}

fn has_false_msbuild_property(source: &str, property: &str) -> bool {
    has_msbuild_property_value(source, property, "false")
}

fn has_true_msbuild_property(source: &str, property: &str) -> bool {
    has_msbuild_property_value(source, property, "true")
}

fn has_msbuild_property_value(source: &str, property: &str, expected: &str) -> bool {
    let opening = format!("<{property}>");
    let closing = format!("</{property}>");
    let Some(start) = source.find(&opening) else {
        return false;
    };
    let value_start = start + opening.len();
    let Some(end) = source[value_start..].find(&closing) else {
        return false;
    };
    source[value_start..value_start + end]
        .trim()
        .eq_ignore_ascii_case(expected)
}

fn normalize_windows_path(path: &str) -> PathBuf {
    PathBuf::from(path.replace('\\', "/"))
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

fn is_generated_csharp_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".Designer.cs")
                || name.ends_with(".g.cs")
                || name.ends_with(".g.i.cs")
                || name.ends_with(".AssemblyInfo.cs")
        })
}

fn is_cleanup_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cs" | "xaml" | "xml" | "resx" | "config" | "settings" | "props" | "targets"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_project_includes_implicit_and_linked_csharp_files() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("App");
        let shared_dir = temp.path().join("Shared");
        fs::create_dir_all(project_dir.join("Sub")).unwrap();
        fs::create_dir_all(project_dir.join("obj")).unwrap();
        fs::create_dir_all(&shared_dir).unwrap();
        fs::write(project_dir.join("Program.cs"), "class Program {}\n").unwrap();
        fs::write(project_dir.join("Sub/Feature.cs"), "class Feature {}\n").unwrap();
        fs::write(project_dir.join("MainWindow.xaml"), "<Window />").unwrap();
        fs::write(project_dir.join("obj/Generated.cs"), "class Generated {}\n").unwrap();
        fs::write(shared_dir.join("Linked.cs"), "class Linked {}\n").unwrap();
        let project = project_dir.join("App.csproj");
        fs::write(
            &project,
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><UseWPF>true</UseWPF></PropertyGroup><ItemGroup><PackageReference Include=\"Example\" /><Compile Include=\"..\\Shared\\Linked.cs\" /></ItemGroup></Project>",
        )
        .unwrap();

        let files = collect_project_files(&project).unwrap();
        assert!(files.contains(&project_dir.join("Program.cs").canonicalize().unwrap()));
        assert!(files.contains(&project_dir.join("Sub/Feature.cs").canonicalize().unwrap()));
        assert!(files.contains(&project_dir.join("MainWindow.xaml").canonicalize().unwrap()));
        assert!(files.contains(&shared_dir.join("Linked.cs").canonicalize().unwrap()));
        assert!(!files.contains(&project_dir.join("obj/Generated.cs").canonicalize().unwrap()));
    }

    #[test]
    fn disabled_default_compile_items_do_not_include_stray_files() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("App");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("Included.cs"), "class Included {}\n").unwrap();
        fs::write(project_dir.join("Stray.cs"), "class Stray {}\n").unwrap();
        let project = project_dir.join("App.csproj");
        fs::write(
            &project,
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup><ItemGroup><Compile Include=\"Included.cs\" /></ItemGroup></Project>",
        )
        .unwrap();

        let files = collect_project_files(&project).unwrap();
        assert_eq!(
            files,
            vec![project_dir.join("Included.cs").canonicalize().unwrap()]
        );
    }

    #[test]
    fn classic_project_does_not_assume_default_compile_items() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("App");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("Included.cs"), "class Included {}\n").unwrap();
        fs::write(project_dir.join("Stray.cs"), "class Stray {}\n").unwrap();
        let project = project_dir.join("App.csproj");
        fs::write(
            &project,
            "<Project ToolsVersion=\"15.0\"><ItemGroup><Compile Include=\"Included.cs\" /></ItemGroup></Project>",
        )
        .unwrap();

        let files = collect_project_files(&project).unwrap();
        assert_eq!(
            files,
            vec![project_dir.join("Included.cs").canonicalize().unwrap()]
        );
    }

    #[test]
    fn identifies_files_that_must_not_be_silently_skipped() {
        assert!(is_cleanup_text_file(Path::new("Source.CS")));
        assert!(is_cleanup_text_file(Path::new("View.xaml")));
        assert!(is_cleanup_text_file(Path::new("Resource.resx")));
        assert!(!is_cleanup_text_file(Path::new("fixture.docx")));
        assert!(!is_cleanup_text_file(Path::new("icon.png")));
    }
}
