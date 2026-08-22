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
    pub xaml: bool,
}

impl FormatOptions {
    pub fn from_properties(
        properties: &Properties,
        include_text: bool,
        _include_indent: bool,
        include_csharp: bool,
        include_csharp_newlines: bool,
        include_non_csharp_text: bool,
        path: &Path,
    ) -> Self {
        let is_csharp = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"));
        let include_text = include_text
            && (include_non_csharp_text
                || !(include_csharp || include_csharp_newlines)
                || is_csharp);
        let preserves_generated_resx_whitespace = include_non_csharp_text
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("resx"));
        let trim_trailing_whitespace = (include_text && !preserves_generated_resx_whitespace)
            .then(|| bool_property(properties, "trim_trailing_whitespace"))
            .flatten();
        let insert_final_newline = include_text
            .then(|| {
                is_csharp
                    .then(|| bool_property(properties, "resharper_csharp_insert_final_newline"))
                    .flatten()
                    .or_else(|| bool_property(properties, "insert_final_newline"))
                    .or_else(|| {
                        (include_non_csharp_text
                            && path
                                .extension()
                                .and_then(|extension| extension.to_str())
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml")))
                        .then_some(false)
                    })
            })
            .flatten();
        let end_of_line = include_text
            .then(|| match properties.get("end_of_line") {
                Some("lf") => Some(EndOfLine::Lf),
                Some("crlf") => Some(EndOfLine::CrLf),
                _ => None,
            })
            .flatten();
        // cleanupcode preserves the existing BOM state in Elsa even though the
        // C# section says utf-8-bom. Rewriting every existing file would create
        // hundreds of false positives, so charset conversion stays disabled.
        let charset_utf8_bom = None;
        let indent = None;
        let csharp = (include_csharp || include_csharp_newlines)
            .then_some(())
            .and_then(|()| {
                is_csharp.then(|| {
                    if include_csharp {
                        CSharpOptions::from_properties(properties)
                    } else {
                        CSharpOptions::newlines_from_properties(properties)
                    }
                })
            });
        let xaml = include_text
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xaml"));

        Self {
            trim_trailing_whitespace,
            insert_final_newline,
            end_of_line,
            charset_utf8_bom,
            indent,
            csharp,
            xaml,
        }
    }

    fn is_noop(&self) -> bool {
        self.trim_trailing_whitespace.is_none()
            && self.insert_final_newline.is_none()
            && self.end_of_line.is_none()
            && self.charset_utf8_bom.is_none()
            && self.indent.is_none()
            && self.csharp.is_none()
            && !self.xaml
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
    if options.xaml {
        normalized = crate::xaml::format_xaml(&normalized);
    }
    if let Some(csharp) = options.csharp {
        normalized = format_csharp(&normalized, csharp);
    }
    let verbatim_string_lines = crate::csharp::verbatim_string_lines(&normalized);
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
        if options.trim_trailing_whitespace == Some(true)
            && verbatim_string_lines.get(index) != Some(&true)
        {
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

fn bool_property(properties: &Properties, key: &str) -> Option<bool> {
    match properties.get(key) {
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
                xaml: false,
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
                charset_utf8_bom: None,
                indent: None,
                csharp: None,
                xaml: false,
            },
        );

        assert!(!output.as_bytes().starts_with(UTF8_BOM));
    }

    #[test]
    fn applies_text_properties() {
        let properties = Properties::from_pairs(&[
            ("trim_trailing_whitespace", "true"),
            ("resharper_csharp_insert_final_newline", "true"),
            ("end_of_line", "crlf"),
        ]);
        let options = FormatOptions::from_properties(
            &properties,
            true,
            false,
            false,
            false,
            false,
            Path::new("Program.cs"),
        );

        assert_eq!(format_text("a  \nb\t", options), "a\r\nb\r\n");
    }

    #[test]
    fn preserves_trailing_whitespace_inside_verbatim_strings() {
        let properties = Properties::from_pairs(&[("trim_trailing_whitespace", "true")]);
        let options = FormatOptions::from_properties(
            &properties,
            true,
            false,
            true,
            false,
            false,
            Path::new("Program.cs"),
        );

        assert_eq!(
            format_text("var xml = @\"<a> \n  <b /> \n</a>\";  \n", options),
            "var xml = @\"<a> \n  <b /> \n</a>\";\n"
        );
    }

    #[test]
    fn removes_xml_final_newline_in_solution_scope() {
        let properties = Properties::from_pairs(&[("trim_trailing_whitespace", "true")]);
        let options = FormatOptions::from_properties(
            &properties,
            true,
            false,
            true,
            false,
            true,
            Path::new("Ribbon.xml"),
        );

        assert_eq!(format_text("<customUI />\n", options), "<customUI />");
    }
}
