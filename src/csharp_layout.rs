//! CIで確認した構文上の整形だけを適用する。名前解決を必要とするusing追加は行わない。
use tree_sitter::Node;

use crate::editorconfig::Properties;

#[derive(Debug, Clone, Default)]
pub struct LayoutOptions {
    chop_parameters: bool,
    compact_initializers: bool,
    wrap_literal_assignments: bool,
}

impl LayoutOptions {
    pub fn from_properties(properties: &Properties) -> Self {
        Self {
            chop_parameters: properties.get("resharper_csharp_wrap_parameters_style")
                == Some("chop_if_long")
                && properties.get("resharper_keep_existing_declaration_parens_arrangement")
                    == Some("false")
                && properties.get("resharper_wrap_after_declaration_lpar") == Some("true"),
            compact_initializers: properties.get("resharper_keep_existing_initializer_arrangement")
                == Some("false"),
            wrap_literal_assignments: true,
        }
    }
}

pub fn format_layout(source: &str, options: &LayoutOptions, margin: usize) -> String {
    let Some(tree) = crate::syntax::parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }
    let mut edits = Vec::new();
    collect(tree.root_node(), source, options, margin, &mut edits);
    let mut result = source.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    result
}

fn collect(
    node: Node<'_>,
    source: &str,
    options: &LayoutOptions,
    margin: usize,
    edits: &mut Vec<(usize, usize, String)>,
) {
    let replacement = match node.kind() {
        "parameter_list" if options.chop_parameters => parameters(node, source, margin),
        "local_declaration_statement" => local_declaration(node, source, options, margin),
        _ => None,
    };
    if let Some(value) = replacement {
        if value != source[node.byte_range()] {
            edits.push((node.start_byte(), node.end_byte(), value));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, options, margin, edits);
    }
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn has_trivia(node: Node<'_>) -> bool {
    node.kind() == "comment"
        || node.kind().starts_with("preproc_")
        || children(node).into_iter().any(has_trivia)
}

fn line_prefix(source: &str, offset: usize) -> &str {
    source[..offset].rsplit('\n').next().unwrap_or("")
}

fn indent(source: &str, offset: usize) -> String {
    line_prefix(source, offset)
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .collect()
}

fn parameters(node: Node<'_>, source: &str, margin: usize) -> Option<String> {
    if !matches!(
        node.parent()?.kind(),
        "method_declaration" | "constructor_declaration"
    ) || has_trivia(node)
    {
        return None;
    }
    let parts = children(node);
    if parts.len() < 2
        || parts
            .iter()
            .any(|p| p.kind() != "parameter" || source[p.byte_range()].contains('\n'))
    {
        return None;
    }
    let values: Vec<_> = parts.iter().map(|p| &source[p.byte_range()]).collect();
    if line_prefix(source, node.start_byte()).chars().count()
        + values.join(", ").chars().count()
        + 2
        <= margin
    {
        return None;
    }
    let continuation = format!("{}    ", indent(source, node.start_byte()));
    Some(format!(
        "(\n{continuation}{})",
        values.join(&format!(",\n{continuation}"))
    ))
}

fn local_declaration(
    node: Node<'_>,
    source: &str,
    options: &LayoutOptions,
    margin: usize,
) -> Option<String> {
    if has_trivia(node) {
        return None;
    }
    let declaration = children(node)
        .into_iter()
        .find(|c| c.kind() == "variable_declaration")?;
    let variables: Vec<_> = children(declaration)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if variables.len() != 1 {
        return None;
    }
    let value = children(variables[0]).into_iter().last()?;
    let prefix = &source[node.start_byte()..value.start_byte()];
    let suffix = &source[value.end_byte()..node.end_byte()];
    let padding = indent(source, node.start_byte());
    if options.wrap_literal_assignments
        && matches!(value.kind(), "string_literal" | "verbatim_string_literal")
        && !source[node.byte_range()].contains('\n')
        && padding.len() + source[node.byte_range()].chars().count() > margin
    {
        return Some(format!(
            "{}\n{padding}    {}{suffix}",
            prefix.trim_end(),
            &source[value.byte_range()]
        ));
    }
    if !options.compact_initializers
        || value.kind() != "object_creation_expression"
        || prefix.contains('\n')
    {
        return None;
    }
    let initializer = value.child_by_field_name("initializer")?;
    let members = children(initializer);
    // 単純な名前の代入だけを対象にし、式・コメント・ネストした初期化子の配置は保持する。
    if members.len() < 2
        || members.iter().any(|m| {
            m.kind() != "assignment_expression"
                || children(*m).len() != 2
                || children(*m).iter().any(|c| c.kind() != "identifier")
        })
    {
        return None;
    }
    let last = members.last()?;
    if source[last.end_byte()..initializer.end_byte()].contains(',') {
        return None;
    }
    let before = &source[node.start_byte()..initializer.start_byte()];
    let members: Vec<_> = members.iter().map(|m| &source[m.byte_range()]).collect();
    let body = members.join(", ");
    if body.contains('\n') || padding.len() + 4 + body.chars().count() > margin {
        return None;
    }
    let single = format!("{} {{ {body} }}{suffix}", before.trim_end());
    if padding.len() + single.chars().count() <= margin {
        return Some(single);
    }
    Some(format!(
        "{}\n{padding}{{\n{padding}    {body}\n{padding}}}{suffix}",
        before.trim_end()
    ))
}
