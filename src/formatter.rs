use std::path::Path;

use crate::csharp::{format_csharp, CSharpOptions};
use crate::editorconfig::Properties;

pub const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
pub const BOM_MARK: &str = "\u{feff}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndOfLine {
    Lf,
    CrLf,
}

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
    pub end_of_line: Option<EndOfLine>,
    pub charset_utf8_bom: Option<bool>,
    pub indent: Option<()>,
    pub csharp: Option<CSharpOptions>,
}

impl FormatOptions {
    pub fn from_properties(
        properties: &Properties,
        _include_text: bool,
        _include_indent: bool,
        include_csharp: bool,
        include_csharp_newlines: bool,
        path: &Path,
    ) -> Self {
        let trim_trailing_whitespace = None;
        let insert_final_newline = None;
        let end_of_line = None;
        let charset_utf8_bom = None;
        let indent = None;
        let csharp = (include_csharp || include_csharp_newlines)
            .then_some(())
            .and_then(|()| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
                    .then(|| {
                        if include_csharp {
                            CSharpOptions::from_properties(properties)
                        } else {
                            CSharpOptions::newlines_from_properties(properties)
                        }
                    })
            });

        Self {
            trim_trailing_whitespace,
            insert_final_newline,
            end_of_line,
            charset_utf8_bom,
            indent,
            csharp,
        }
    }

    fn is_noop(&self) -> bool {
        self.trim_trailing_whitespace.is_none()
            && self.insert_final_newline.is_none()
            && self.end_of_line.is_none()
            && self.charset_utf8_bom.is_none()
            && self.indent.is_none()
            && self.csharp.is_none()
    }
}

pub fn format_text(input: &str, options: FormatOptions) -> String {
    if options.is_noop() {
        return input.to_string();
    }

    let had_bom = input.starts_with(BOM_MARK);
    let body = input.strip_prefix(BOM_MARK).unwrap_or(input);
    let eol = options.end_of_line.unwrap_or_else(|| detect_eol(body));
    let newline = match eol {
        EndOfLine::Lf => "\n",
        EndOfLine::CrLf => "\r\n",
    };

    let mut output = String::with_capacity(input.len());
    if had_bom && options.charset_utf8_bom.unwrap_or(true) {
        output.push_str(BOM_MARK);
    }

    let mut normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    if let Some(csharp) = options.csharp {
        normalized = format_csharp(&normalized, csharp);
    }
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    let had_final_newline = lines.last().is_some_and(|last| last.is_empty());
    if had_final_newline {
        lines.pop();
    }

    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push_str(newline);
        }

        let mut formatted_line = (*line).to_string();
        if options.trim_trailing_whitespace == Some(true) {
            formatted_line = formatted_line.trim_end_matches([' ', '\t']).to_string();
        }
        output.push_str(&formatted_line);
    }

    let should_insert_final_newline = options.insert_final_newline.unwrap_or(had_final_newline);
    if should_insert_final_newline && (!output.ends_with('\n') && !output.ends_with('\r')) {
        output.push_str(newline);
    }

    output
}

fn detect_eol(text: &str) -> EndOfLine {
    if text.as_bytes().windows(2).any(|window| window == b"\r\n") {
        EndOfLine::CrLf
    } else {
        EndOfLine::Lf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_trailing_whitespace_and_final_newline() {
        let output = format_text(
            "a  \r\nb\t",
            FormatOptions {
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                end_of_line: None,
                charset_utf8_bom: None,
                indent: None,
                csharp: None,
            },
        );

        assert_eq!(output, "a  \r\nb\t");
    }

    #[test]
    fn preserves_missing_utf8_bom() {
        let output = format_text(
            "class C {}\n",
            FormatOptions {
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                end_of_line: None,
                charset_utf8_bom: Some(true),
                indent: None,
                csharp: None,
            },
        );

        assert!(!output.as_bytes().starts_with(UTF8_BOM));
    }
}
