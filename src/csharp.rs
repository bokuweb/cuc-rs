use std::collections::{BTreeSet, HashMap};

use crate::editorconfig::Properties;

#[derive(Debug, Clone)]
pub struct CSharpOptions {
    pub sort_system_directives_first: bool,
    pub separate_import_directive_groups: bool,
    pub modifier_order: Vec<String>,
    pub space_after_comma: bool,
    pub space_before_comma: bool,
    pub space_after_dot: bool,
    pub space_before_dot: bool,
    pub space_after_semicolon_in_for: bool,
    pub space_before_semicolon_in_for: bool,
    pub space_around_binary_operators: bool,
    pub new_line_before_else: bool,
    pub new_line_before_catch: bool,
    pub new_line_before_finally: bool,
}

impl CSharpOptions {
    pub fn from_properties(properties: &Properties) -> Self {
        Self {
            sort_system_directives_first: properties
                .get("dotnet_sort_system_directives_first")
                .map(|value| value == "true")
                .unwrap_or(true),
            separate_import_directive_groups: properties
                .get("dotnet_separate_import_directive_groups")
                .map(|value| value == "true")
                .unwrap_or(false),
            modifier_order: parse_modifier_order(properties),
            space_after_comma: bool_property(properties, "csharp_space_after_comma", true),
            space_before_comma: bool_property(properties, "csharp_space_before_comma", false),
            space_after_dot: bool_property(properties, "csharp_space_after_dot", false),
            space_before_dot: bool_property(properties, "csharp_space_before_dot", false),
            space_after_semicolon_in_for: bool_property(
                properties,
                "csharp_space_after_semicolon_in_for_statement",
                true,
            ),
            space_before_semicolon_in_for: bool_property(
                properties,
                "csharp_space_before_semicolon_in_for_statement",
                false,
            ),
            space_around_binary_operators: properties
                .get("csharp_space_around_binary_operators")
                .map(|value| value == "before_and_after")
                .unwrap_or(true),
            new_line_before_else: bool_property(properties, "csharp_new_line_before_else", true),
            new_line_before_catch: bool_property(properties, "csharp_new_line_before_catch", true),
            new_line_before_finally: bool_property(
                properties,
                "csharp_new_line_before_finally",
                true,
            ),
        }
    }
}

pub fn format_csharp(input: &str, options: CSharpOptions) -> String {
    let input = sort_using_blocks(input, &options);
    let input = reorder_modifiers(&input, &options);
    let input = normalize_token_spacing(&input, &options);
    normalize_control_flow_newlines(&input, &options)
}

fn bool_property(properties: &Properties, key: &str, default: bool) -> bool {
    properties
        .get(key)
        .map(|value| value == "true")
        .unwrap_or(default)
}

fn parse_modifier_order(properties: &Properties) -> Vec<String> {
    properties
        .get("csharp_preferred_modifier_order")
        .or_else(|| properties.get("visual_basic_preferred_modifier_order"))
        .map(|value| {
            value
                .split(':')
                .next()
                .unwrap_or(value)
                .split(',')
                .map(str::trim)
                .filter(|modifier| !modifier.is_empty())
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_else(|| {
            [
                "public",
                "private",
                "protected",
                "internal",
                "file",
                "static",
                "extern",
                "new",
                "virtual",
                "abstract",
                "sealed",
                "override",
                "readonly",
                "unsafe",
                "volatile",
                "async",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        })
}

fn sort_using_blocks(input: &str, options: &CSharpOptions) -> String {
    let mut output = Vec::new();
    let lines: Vec<&str> = input.split('\n').collect();
    let mut index = 0usize;

    while index < lines.len() {
        if is_using_directive(lines[index]) {
            let start = index;
            while index < lines.len() && is_using_directive(lines[index]) {
                index += 1;
            }
            output.extend(format_using_block(&lines[start..index], options));
        } else {
            output.push(lines[index].to_string());
            index += 1;
        }
    }

    output.join("\n")
}

fn is_using_directive(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed
        .strip_prefix("using ")
        .or_else(|| trimmed.strip_prefix("global using "))
    else {
        return false;
    };

    rest.ends_with(';') && !rest.contains('{') && !rest.contains('}')
}

fn format_using_block(lines: &[&str], options: &CSharpOptions) -> Vec<String> {
    let indent = lines
        .first()
        .and_then(|line| line.get(..line.len() - line.trim_start().len()))
        .unwrap_or("");
    let directives: BTreeSet<String> = lines.iter().map(|line| line.trim().to_string()).collect();
    let mut directives: Vec<String> = directives.into_iter().collect();

    directives.sort_by(|left, right| compare_using_directives(left, right, options));

    if options.sort_system_directives_first && options.separate_import_directive_groups {
        let first_non_system = directives.iter().position(|line| !is_system_using(line));
        if let Some(split) =
            first_non_system.filter(|split| *split > 0 && *split < directives.len())
        {
            return directives
                .into_iter()
                .enumerate()
                .flat_map(|(index, directive)| {
                    let mut lines = Vec::new();
                    if index == split {
                        lines.push(String::new());
                    }
                    lines.push(format!("{indent}{directive}"));
                    lines
                })
                .collect();
        }
    }

    directives
        .into_iter()
        .map(|directive| format!("{indent}{directive}"))
        .collect()
}

fn compare_using_directives(
    left: &str,
    right: &str,
    options: &CSharpOptions,
) -> std::cmp::Ordering {
    if options.sort_system_directives_first {
        let left_system = is_system_using(left);
        let right_system = is_system_using(right);
        match (left_system, right_system) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
    }

    using_sort_key(left).cmp(using_sort_key(right))
}

fn is_system_using(line: &str) -> bool {
    let target = using_sort_key(line);

    target == "System"
        || target.starts_with("System.")
        || target.starts_with("static System.")
        || target.contains("= System.")
}

fn using_sort_key(line: &str) -> &str {
    line.trim()
        .strip_prefix("using ")
        .or_else(|| line.trim().strip_prefix("global using "))
        .unwrap_or(line)
        .trim_start()
        .trim_end_matches(';')
}

fn reorder_modifiers(input: &str, options: &CSharpOptions) -> String {
    if options.modifier_order.is_empty() {
        return input.to_string();
    }

    let rank = options
        .modifier_order
        .iter()
        .enumerate()
        .map(|(index, modifier)| (modifier.as_str(), index))
        .collect::<HashMap<_, _>>();

    input
        .split('\n')
        .map(|line| reorder_modifiers_in_line(line, &rank))
        .collect::<Vec<_>>()
        .join("\n")
}

fn reorder_modifiers_in_line(line: &str, rank: &HashMap<&str, usize>) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    if rest.starts_with("//") || rest.starts_with('#') || rest.starts_with('[') {
        return line.to_string();
    }

    let mut tokens = rest.split_whitespace().peekable();
    let mut modifiers = Vec::new();
    let mut consumed_len = 0usize;

    while let Some(token) = tokens.peek().copied() {
        let bare = token.trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
        if !rank.contains_key(bare) {
            break;
        }
        modifiers.push(bare.to_string());
        consumed_len += token.len();
        if consumed_len < rest.len() && rest.as_bytes().get(consumed_len) == Some(&b' ') {
            consumed_len += 1;
        }
        tokens.next();
    }

    if modifiers.len() < 2 {
        return line.to_string();
    }

    let original = modifiers.clone();
    modifiers.sort_by_key(|modifier| rank.get(modifier.as_str()).copied().unwrap_or(usize::MAX));
    if modifiers == original {
        return line.to_string();
    }

    let suffix = rest[consumed_len..].trim_start();
    format!("{indent}{} {suffix}", modifiers.join(" "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeState {
    Normal,
    LineComment,
    BlockComment,
    String { verbatim: bool },
    Char,
}

fn normalize_token_spacing(input: &str, options: &CSharpOptions) -> String {
    let mut output = String::with_capacity(input.len());
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut state = CodeState::Normal;
    let mut line_start = true;

    while index < chars.len() {
        let ch = chars[index];
        match state {
            CodeState::Normal => {
                if line_start && ch == '#' {
                    index = copy_until_newline(&chars, index, &mut output);
                    line_start = true;
                    continue;
                }

                if ch == '/' && chars.get(index + 1) == Some(&'/') {
                    output.push(ch);
                    output.push('/');
                    index += 2;
                    state = CodeState::LineComment;
                    continue;
                }

                if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    output.push(ch);
                    output.push('*');
                    index += 2;
                    state = CodeState::BlockComment;
                    continue;
                }

                if ch == '"' || starts_verbatim_string(&chars, index) {
                    let verbatim = starts_verbatim_string(&chars, index);
                    if verbatim {
                        output.push('@');
                        output.push('"');
                        index += 2;
                    } else {
                        output.push(ch);
                        index += 1;
                    }
                    state = CodeState::String { verbatim };
                    line_start = false;
                    continue;
                }

                if ch == '\'' {
                    output.push(ch);
                    index += 1;
                    state = CodeState::Char;
                    line_start = false;
                    continue;
                }

                if ch == ',' {
                    write_separator_spacing(
                        &mut output,
                        &chars,
                        &mut index,
                        ',',
                        options.space_before_comma,
                        options.space_after_comma,
                    );
                    line_start = false;
                    continue;
                }

                if ch == '.' {
                    write_separator_spacing(
                        &mut output,
                        &chars,
                        &mut index,
                        '.',
                        options.space_before_dot,
                        options.space_after_dot,
                    );
                    line_start = false;
                    continue;
                }

                if ch == ';' {
                    write_separator_spacing(
                        &mut output,
                        &chars,
                        &mut index,
                        ';',
                        options.space_before_semicolon_in_for,
                        options.space_after_semicolon_in_for,
                    );
                    line_start = false;
                    continue;
                }

                if matches!(ch, '(' | '[') {
                    write_open_bracket_spacing(&mut output, &chars, &mut index, ch);
                    line_start = false;
                    continue;
                }

                if matches!(ch, ')' | ']') {
                    trim_horizontal_space(&mut output);
                    output.push(ch);
                    index += 1;
                    line_start = false;
                    continue;
                }

                if options.space_around_binary_operators {
                    if let Some(operator_len) = binary_operator_len(&chars, index) {
                        write_operator_spacing(&mut output, &chars, &mut index, operator_len);
                        line_start = false;
                        continue;
                    }
                }

                output.push(ch);
                index += 1;
                line_start = ch == '\n';
            }
            CodeState::LineComment => {
                output.push(ch);
                index += 1;
                if ch == '\n' {
                    state = CodeState::Normal;
                    line_start = true;
                }
            }
            CodeState::BlockComment => {
                output.push(ch);
                if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    output.push('/');
                    index += 2;
                    state = CodeState::Normal;
                } else {
                    index += 1;
                }
                line_start = ch == '\n';
            }
            CodeState::String { verbatim } => {
                output.push(ch);
                if verbatim && ch == '"' && chars.get(index + 1) == Some(&'"') {
                    output.push('"');
                    index += 2;
                    continue;
                }
                if ch == '"' {
                    state = CodeState::Normal;
                } else if !verbatim && ch == '\\' {
                    if let Some(next) = chars.get(index + 1) {
                        output.push(*next);
                        index += 2;
                        continue;
                    }
                }
                index += 1;
                line_start = ch == '\n';
            }
            CodeState::Char => {
                output.push(ch);
                if ch == '\'' {
                    state = CodeState::Normal;
                } else if ch == '\\' {
                    if let Some(next) = chars.get(index + 1) {
                        output.push(*next);
                        index += 2;
                        continue;
                    }
                }
                index += 1;
                line_start = false;
            }
        }
    }

    output
}

fn starts_verbatim_string(chars: &[char], index: usize) -> bool {
    chars.get(index) == Some(&'@') && chars.get(index + 1) == Some(&'"')
}

fn copy_until_newline(chars: &[char], mut index: usize, output: &mut String) -> usize {
    while let Some(ch) = chars.get(index) {
        output.push(*ch);
        index += 1;
        if *ch == '\n' {
            break;
        }
    }
    index
}

fn write_separator_spacing(
    output: &mut String,
    chars: &[char],
    index: &mut usize,
    separator: char,
    space_before: bool,
    space_after: bool,
) {
    trim_horizontal_space(output);
    if space_before && needs_space_before(output) {
        output.push(' ');
    }
    output.push(separator);
    *index += 1;
    skip_horizontal_space(chars, index);
    if space_after && needs_space_after(chars, *index) {
        output.push(' ');
    }
}

fn write_operator_spacing(
    output: &mut String,
    chars: &[char],
    index: &mut usize,
    operator_len: usize,
) {
    trim_horizontal_space(output);
    if needs_space_before(output) {
        output.push(' ');
    }
    for offset in 0..operator_len {
        output.push(chars[*index + offset]);
    }
    *index += operator_len;
    skip_horizontal_space(chars, index);
    if needs_space_after(chars, *index) {
        output.push(' ');
    }
}

fn write_open_bracket_spacing(
    output: &mut String,
    chars: &[char],
    index: &mut usize,
    bracket: char,
) {
    if bracket == '(' && previous_word(output).is_some_and(is_control_keyword) {
        trim_horizontal_space(output);
        output.push(' ');
    } else {
        trim_horizontal_space(output);
    }

    output.push(bracket);
    *index += 1;
    skip_horizontal_space(chars, index);
}

fn trim_horizontal_space(output: &mut String) {
    while output.ends_with(' ') || output.ends_with('\t') {
        output.pop();
    }
}

fn previous_word(output: &str) -> Option<&str> {
    let trimmed = output.trim_end_matches([' ', '\t']);
    let end = trimmed.len();
    let start = trimmed
        .char_indices()
        .rev()
        .find_map(|(index, ch)| {
            (!ch.is_ascii_alphanumeric() && ch != '_').then_some(index + ch.len_utf8())
        })
        .unwrap_or(0);

    trimmed.get(start..end)
}

fn is_control_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "for" | "foreach" | "while" | "switch" | "catch" | "using" | "lock" | "fixed"
    )
}

fn skip_horizontal_space(chars: &[char], index: &mut usize) {
    while matches!(chars.get(*index), Some(' ' | '\t')) {
        *index += 1;
    }
}

fn needs_space_before(output: &str) -> bool {
    output
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_whitespace() && ch != '(' && ch != '[' && ch != '{')
}

fn needs_space_after(chars: &[char], index: usize) -> bool {
    chars
        .get(index)
        .is_some_and(|ch| !ch.is_whitespace() && !matches!(ch, ')' | ']' | '}' | ';' | ','))
}

fn binary_operator_len(chars: &[char], index: usize) -> Option<usize> {
    let current = *chars.get(index)?;
    let next = chars.get(index + 1).copied();
    let previous = previous_non_space(chars, index);

    match (current, next) {
        ('=', Some('>')) => Some(2),
        ('=', Some('=')) => Some(2),
        ('!', Some('=')) => Some(2),
        ('<', Some('=')) => Some(2),
        ('>', Some('=')) => Some(2),
        ('&', Some('&')) => Some(2),
        ('|', Some('|')) => Some(2),
        ('?', Some('?')) => Some(2),
        ('+', Some('='))
        | ('-', Some('='))
        | ('*', Some('='))
        | ('/', Some('='))
        | ('%', Some('='))
        | ('&', Some('='))
        | ('|', Some('='))
        | ('^', Some('=')) => Some(2),
        ('=', _) => Some(1),
        ('*' | '/' | '%' | '&' | '|' | '^', _) => Some(1),
        ('+' | '-', _) if previous.is_some_and(can_precede_binary_plus_or_minus) => Some(1),
        _ => None,
    }
}

fn previous_non_space(chars: &[char], index: usize) -> Option<char> {
    chars
        .get(..index)?
        .iter()
        .rev()
        .find(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r'))
        .copied()
}

fn can_precede_binary_plus_or_minus(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | ')' | ']' | '}')
}

fn normalize_control_flow_newlines(input: &str, options: &CSharpOptions) -> String {
    input
        .split('\n')
        .flat_map(|line| split_control_flow_line(line, options))
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_control_flow_line(line: &str, options: &CSharpOptions) -> Vec<String> {
    let mut pending = vec![line.to_string()];
    let mut changed = true;
    while changed {
        changed = false;
        let mut next = Vec::new();
        for line in pending {
            let split = split_one_control_flow_line(&line, options);
            if split.len() > 1 {
                changed = true;
            }
            next.extend(split);
        }
        pending = next;
    }
    pending
}

fn split_one_control_flow_line(line: &str, options: &CSharpOptions) -> Vec<String> {
    let rules = [
        (" else", options.new_line_before_else),
        (" catch", options.new_line_before_catch),
        (" finally", options.new_line_before_finally),
    ];

    for (needle, enabled) in rules {
        if !enabled {
            continue;
        }
        let Some(position) = line.find(needle) else {
            continue;
        };
        if !line[..position].trim_end().ends_with('}') {
            continue;
        }

        let indent = line
            .get(..line.len() - line.trim_start().len())
            .unwrap_or("");
        let first = line[..position].trim_end().to_string();
        let second = format!("{indent}{}", line[position + 1..].trim_start());
        return vec![first, second];
    }

    vec![line.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> CSharpOptions {
        CSharpOptions {
            sort_system_directives_first: true,
            separate_import_directive_groups: true,
            space_after_comma: true,
            space_before_comma: false,
            space_after_dot: false,
            space_before_dot: false,
            space_after_semicolon_in_for: true,
            space_before_semicolon_in_for: false,
            space_around_binary_operators: true,
            new_line_before_else: true,
            new_line_before_catch: true,
            new_line_before_finally: true,
            modifier_order: [
                "public",
                "private",
                "protected",
                "internal",
                "file",
                "static",
                "extern",
                "new",
                "virtual",
                "abstract",
                "sealed",
                "override",
                "readonly",
                "unsafe",
                "volatile",
                "async",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }

    #[test]
    fn sorts_and_deduplicates_using_blocks() {
        let input = "using Elsa;\nusing System.Text;\nusing System;\nusing Elsa;\n\nclass C {}\n";

        assert_eq!(
            format_csharp(input, options()),
            "using System;\nusing System.Text;\n\nusing Elsa;\n\nclass C {}\n"
        );
    }

    #[test]
    fn reorders_modifiers() {
        let input = "class C\n{\n    static private readonly string Value;\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    private static readonly string Value;\n}\n"
        );
    }

    #[test]
    fn normalizes_simple_token_spacing() {
        let input =
            "class C\n{\n    void M(){ var x=a+b; Call( a ,b,c ); for(i=0;i<10;i+=1){} }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M(){ var x = a + b; Call(a, b, c); for (i = 0; i<10; i += 1){} }\n}\n"
        );
    }

    #[test]
    fn leaves_comments_and_strings_unchanged() {
        let input =
            "class C\n{\n    string S = \"a,b+c\"; // x,y+z\n    string V = @\"a,b+c\";\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    string S = \"a,b+c\"; // x,y+z\n    string V = @\"a,b+c\";\n}\n"
        );
    }

    #[test]
    fn splits_else_catch_and_finally_to_new_lines() {
        let input = "class C\n{\n    void M(){ if (x){} else {} try {} catch {} finally {} }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M(){ if (x){}\n    else {} try {}\n    catch {}\n    finally {} }\n}\n"
        );
    }
}
