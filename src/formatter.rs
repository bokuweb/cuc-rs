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
    pub indent: Option<IndentOptions>,
    pub csharp: Option<CSharpOptions>,
}

#[derive(Debug, Clone, Copy)]
pub struct IndentOptions {
    pub style: IndentStyle,
    pub size: usize,
    pub tab_width: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum IndentStyle {
    Space,
    Tab,
}

impl FormatOptions {
    pub fn from_properties(
        properties: &Properties,
        include_text: bool,
        include_indent: bool,
        include_csharp: bool,
        include_csharp_newlines: bool,
        path: &Path,
    ) -> Self {
        let trim_trailing_whitespace = include_text
            .then(|| parse_bool(properties.get("trim_trailing_whitespace")))
            .flatten();
        let insert_final_newline = include_text
            .then(|| parse_bool(properties.get("insert_final_newline")))
            .flatten();
        let end_of_line = include_text
            .then(|| match properties.get("end_of_line") {
                Some("lf") => Some(EndOfLine::Lf),
                Some("crlf") => Some(EndOfLine::CrLf),
                _ => None,
            })
            .flatten();
        let charset_utf8_bom = include_text
            .then(|| match properties.get("charset") {
                Some("utf-8-bom") => Some(true),
                Some("utf-8") => Some(false),
                _ => None,
            })
            .flatten();
        let indent = if include_indent {
            parse_indent(properties)
        } else {
            None
        };
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
    if options.charset_utf8_bom.unwrap_or(had_bom) {
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
        if let Some(indent) = options.indent {
            formatted_line = normalize_indent(&formatted_line, indent);
        }
        output.push_str(&formatted_line);
    }

    let should_insert_final_newline = options.insert_final_newline.unwrap_or(had_final_newline);
    if should_insert_final_newline && (!output.ends_with('\n') && !output.ends_with('\r')) {
        output.push_str(newline);
    }

    output
}

fn parse_indent(properties: &Properties) -> Option<IndentOptions> {
    let style = match properties.get("indent_style") {
        Some("space") => IndentStyle::Space,
        Some("tab") => IndentStyle::Tab,
        _ => return None,
    };
    let tab_width = properties
        .get("tab_width")
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);
    let size = properties
        .get("indent_size")
        .and_then(|value| {
            if value == "tab" {
                Some(tab_width)
            } else {
                value.parse().ok()
            }
        })
        .unwrap_or(tab_width);

    Some(IndentOptions {
        style,
        size,
        tab_width,
    })
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn detect_eol(text: &str) -> EndOfLine {
    if text.as_bytes().windows(2).any(|window| window == b"\r\n") {
        EndOfLine::CrLf
    } else {
        EndOfLine::Lf
    }
}

fn normalize_indent(line: &str, options: IndentOptions) -> String {
    let split = line
        .find(|ch| ch != ' ' && ch != '\t')
        .unwrap_or(line.len());
    let (prefix, rest) = line.split_at(split);
    if prefix.is_empty() {
        return line.to_string();
    }

    let columns = prefix.chars().fold(0usize, |columns, ch| match ch {
        '\t' => columns + options.tab_width,
        ' ' => columns + 1,
        _ => columns,
    });
    let normalized_prefix = match options.style {
        IndentStyle::Space => " ".repeat(columns),
        IndentStyle::Tab => {
            let tabs = columns / options.size;
            let spaces = columns % options.size;
            format!("{}{}", "\t".repeat(tabs), " ".repeat(spaces))
        }
    };

    format!("{normalized_prefix}{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_whitespace_and_inserts_final_newline() {
        let output = format_text(
            "a  \r\nb\t",
            FormatOptions {
                trim_trailing_whitespace: Some(true),
                insert_final_newline: Some(true),
                end_of_line: Some(EndOfLine::Lf),
                charset_utf8_bom: None,
                indent: None,
                csharp: None,
            },
        );

        assert_eq!(output, "a\nb\n");
    }

    #[test]
    fn can_add_utf8_bom() {
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

        assert!(output.as_bytes().starts_with(UTF8_BOM));
    }
}
