use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Properties {
    values: BTreeMap<String, String>,
}

impl Properties {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

#[derive(Debug)]
pub struct EditorConfig {
    sections: Vec<Section>,
}

#[derive(Debug)]
struct Section {
    matcher: GlobSet,
    properties: BTreeMap<String, String>,
}

impl EditorConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Self> {
        let mut sections = Vec::new();
        let mut current_patterns: Option<Vec<String>> = None;
        let mut current_properties = BTreeMap::new();

        for raw_line in source.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                if let Some(patterns) = current_patterns.take() {
                    sections.push(build_section(
                        patterns,
                        std::mem::take(&mut current_properties),
                    )?);
                }
                current_patterns = Some(expand_section_patterns(&line[1..line.len() - 1]));
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if current_patterns.is_some() {
                current_properties.insert(
                    key.trim().to_ascii_lowercase(),
                    value.trim().to_ascii_lowercase(),
                );
            }
        }

        if let Some(patterns) = current_patterns {
            sections.push(build_section(patterns, current_properties)?);
        }

        Ok(Self { sections })
    }

    pub fn properties_for(&self, path: &Path) -> Properties {
        let normalized = normalize_path(path);
        let mut values = BTreeMap::new();
        for section in &self.sections {
            if section.matcher.is_match(&normalized) {
                for (key, value) in &section.properties {
                    if value == "unset" {
                        values.remove(key);
                    } else {
                        values.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        Properties { values }
    }
}

fn build_section(patterns: Vec<String>, properties: BTreeMap<String, String>) -> Result<Section> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(&pattern).with_context(|| format!("invalid editorconfig glob: {pattern}"))?,
        );
    }
    Ok(Section {
        matcher: builder.build()?,
        properties,
    })
}

fn expand_section_patterns(pattern: &str) -> Vec<String> {
    let pattern = normalize_section_pattern(pattern.trim());
    if pattern.contains('/') {
        vec![pattern]
    } else {
        vec![pattern.clone(), format!("**/{pattern}")]
    }
}

fn normalize_section_pattern(pattern: &str) -> String {
    let mut output = String::with_capacity(pattern.len());
    let mut in_brace = false;

    for ch in pattern.chars() {
        match ch {
            '{' => {
                in_brace = true;
                output.push(ch);
            }
            '}' => {
                in_brace = false;
                output.push(ch);
            }
            ' ' | '\t' if in_brace => {}
            _ => output.push(ch),
        }
    }

    output
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_sections_override_earlier_sections() {
        let config = EditorConfig::parse(
            r#"
[*]
indent_style = space
trim_trailing_whitespace = true

[*.md]
trim_trailing_whitespace = false
"#,
        )
        .unwrap();

        let props = config.properties_for(&std::path::PathBuf::from("docs/readme.md"));

        assert_eq!(props.get("indent_style"), Some("space"));
        assert_eq!(props.get("trim_trailing_whitespace"), Some("false"));
    }

    #[test]
    fn supports_brace_patterns() {
        let config = EditorConfig::parse(
            r#"
[*.{cs,vb}]
charset = utf-8-bom
"#,
        )
        .unwrap();

        assert_eq!(
            config
                .properties_for(&std::path::PathBuf::from("Elsa/Program.cs"))
                .get("charset"),
            Some("utf-8-bom")
        );
    }

    #[test]
    fn trims_spaces_inside_brace_patterns() {
        let config = EditorConfig::parse(
            r#"
[*.{cmd, bat}]
end_of_line = crlf
"#,
        )
        .unwrap();

        assert_eq!(
            config
                .properties_for(&std::path::PathBuf::from("scripts/build.bat"))
                .get("end_of_line"),
            Some("crlf")
        );
    }
}
