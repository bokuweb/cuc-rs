use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::editorconfig::Properties;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CSharpOptions {
    pub interface_layout: Option<Arc<crate::syntax::InterfaceLayout>>,
    pub sort_usings: bool,
    pub arrange_fields: bool,
    pub remove_clearly_unused_usings: bool,
    pub reorder_modifiers: bool,
    pub normalize_spacing: bool,
    pub normalize_newlines: bool,
    pub collapse_simple_wrapping: bool,
    pub prefer_expression_bodied_members: bool,
    pub prefer_explicit_type_when_apparent: bool,
    pub max_line_length: usize,
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
            interface_layout: None,
            sort_usings: true,
            // Both operations need a fully resolved semantic model. Elsa's
            // authoritative Windows/VSTO cleanup exposed false positives in
            // the local-name heuristics, so production mode keeps them off.
            arrange_fields: false,
            remove_clearly_unused_usings: false,
            reorder_modifiers: false,
            normalize_spacing: true,
            normalize_newlines: true,
            collapse_simple_wrapping: properties
                .get("resharper_keep_existing_invocation_parens_arrangement")
                .map(|value| value == "false")
                .unwrap_or(false),
            prefer_expression_bodied_members: style_enabled(
                properties,
                "csharp_style_expression_bodied_constructors",
            ) || style_enabled(
                properties,
                "csharp_style_expression_bodied_methods",
            ),
            prefer_explicit_type_when_apparent: !style_enabled(
                properties,
                "csharp_style_var_when_type_is_apparent",
            ),
            max_line_length: properties
                .get("max_line_length")
                .and_then(|value| value.parse().ok())
                .unwrap_or(120),
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

    pub fn newlines_from_properties(properties: &Properties) -> Self {
        let mut options = Self::from_properties(properties);
        options.sort_usings = false;
        options.arrange_fields = false;
        options.remove_clearly_unused_usings = false;
        options.reorder_modifiers = false;
        options.normalize_spacing = false;
        options.normalize_newlines = true;
        options.collapse_simple_wrapping = false;
        options.prefer_expression_bodied_members = false;
        options.prefer_explicit_type_when_apparent = false;
        options
    }
}

pub fn format_csharp(input: &str, options: CSharpOptions) -> String {
    let input_for_syntax_guard = input;
    let input = if options.sort_usings {
        let input = simplify_known_framework_qualifications(input);
        if options.remove_clearly_unused_usings {
            remove_clearly_unused_system_using(&input)
        } else {
            input
        }
    } else {
        input.to_string()
    };
    let input = if options.sort_usings {
        sort_using_blocks(&input, &options)
    } else {
        input
    };
    let input = if options.arrange_fields {
        let input = options
            .interface_layout
            .as_deref()
            .map(|layout| crate::syntax::arrange_interface_implementations(&input, layout))
            .unwrap_or(input);
        crate::syntax::arrange_misplaced_fields(&input)
    } else {
        input
    };
    let input = if options.reorder_modifiers {
        reorder_modifiers(&input, &options)
    } else {
        input
    };
    let input = if options.normalize_spacing {
        normalize_token_spacing(&input, &options)
    } else {
        input
    };
    let input = if options.prefer_explicit_type_when_apparent {
        replace_apparent_var_declarations(&input)
    } else {
        input
    };
    let input = if options.normalize_spacing {
        let input = remove_verified_redundant_named_arguments(&input);
        let input = remove_known_framework_named_arguments(&input);
        let input = remove_asserted_null_forgiving_operators(&input);
        remove_redundant_global_namespace_qualifiers(&input)
    } else {
        input
    };
    let input = if options.prefer_expression_bodied_members {
        collapse_single_statement_members(&input, options.max_line_length)
    } else {
        input
    };
    let input = if options.collapse_simple_wrapping {
        let input = expand_long_object_creation_arguments(&input, options.max_line_length);
        let input = collapse_single_property_initializers(&input, options.max_line_length);
        let input = collapse_adjacent_initializer_items(&input, options.max_line_length);
        collapse_simple_wrapping(&input, options.max_line_length)
    } else {
        input
    };
    let output = if options.normalize_newlines {
        normalize_control_flow_newlines(&input, &options)
    } else {
        input
    };
    preserve_parseable_input(input_for_syntax_guard, output)
}

fn preserve_parseable_input(original: &str, candidate: String) -> String {
    if candidate == original {
        return candidate;
    }
    let original_is_parseable =
        crate::syntax::parse_csharp(original).is_some_and(|tree| !tree.root_node().has_error());
    if !original_is_parseable {
        return candidate;
    }
    let candidate_is_parseable =
        crate::syntax::parse_csharp(&candidate).is_some_and(|tree| !tree.root_node().has_error());
    if candidate_is_parseable {
        candidate
    } else {
        original.to_string()
    }
}

fn expand_long_object_creation_arguments(input: &str, max_line_length: usize) -> String {
    let normal_lines = normal_code_lines(input);
    let mut output = input
        .lines()
        .zip(normal_lines)
        .map(|(line, normal)| {
            if !normal || line.chars().count() <= max_line_length {
                return line.to_string();
            }
            let Some(new_start) = line.find(" = new ") else {
                return line.to_string();
            };
            let Some(relative_open) = line[new_start + 7..].find('(') else {
                return line.to_string();
            };
            let open = new_start + 7 + relative_open;
            let Some(close) = line.rfind(");") else {
                return line.to_string();
            };
            if close <= open || !line[close + 2..].trim().is_empty() {
                return line.to_string();
            }
            let Some(commas) = top_level_comma_offsets(&line[open + 1..close]) else {
                return line.to_string();
            };
            if commas.is_empty() {
                return line.to_string();
            }
            let mut arguments = Vec::with_capacity(commas.len() + 1);
            let mut start = open + 1;
            for comma in commas {
                let end = open + 1 + comma;
                arguments.push(line[start..end].trim());
                start = end + 1;
            }
            arguments.push(line[start..close].trim());
            if arguments.iter().any(|argument| argument.is_empty()) {
                return line.to_string();
            }
            let continuation = format!("{}    ", &line[..line.len() - line.trim_start().len()]);
            let mut output = format!("{}\n", &line[..=open]);
            for (index, argument) in arguments.iter().enumerate() {
                output.push_str(&continuation);
                output.push_str(argument);
                if index + 1 == arguments.len() {
                    output.push_str(");");
                } else {
                    output.push_str(",\n");
                }
            }
            output
        })
        .collect::<Vec<_>>()
        .join("\n");
    if input.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn top_level_comma_offsets(arguments: &str) -> Option<Vec<usize>> {
    if arguments.contains(['\'', '"']) {
        return None;
    }
    let mut parentheses = 0i32;
    let mut brackets = 0i32;
    let mut braces = 0i32;
    let mut commas = Vec::new();
    for (index, ch) in arguments.char_indices() {
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '{' => braces += 1,
            '}' => braces -= 1,
            ',' if parentheses == 0 && brackets == 0 && braces == 0 => commas.push(index),
            _ => {}
        }
        if parentheses < 0 || brackets < 0 || braces < 0 {
            return None;
        }
    }
    (parentheses == 0 && brackets == 0 && braces == 0).then_some(commas)
}

fn remove_clearly_unused_system_using(input: &str) -> String {
    if !input.lines().any(|line| line.trim() == "using System;") {
        return input.to_string();
    }
    const SYSTEM_IDENTIFIERS: &[&str] = &[
        "Action",
        "Activator",
        "AggregateException",
        "AppContext",
        "ApplicationException",
        "ArgumentException",
        "ArgumentNullException",
        "ArgumentOutOfRangeException",
        "Array",
        "ArraySegment",
        "AsyncCallback",
        "Attribute",
        "BitConverter",
        "Buffer",
        "Char",
        "CLSCompliant",
        "Comparison",
        "Console",
        "Convert",
        "DateTime",
        "DateTimeKind",
        "DateTimeOffset",
        "DayOfWeek",
        "Decimal",
        "Delegate",
        "DivideByZeroException",
        "DllNotFoundException",
        "Environment",
        "EventArgs",
        "EventHandler",
        "Exception",
        "Flags",
        "FormatException",
        "Func",
        "GC",
        "Guid",
        "IAsyncResult",
        "IAsyncDisposable",
        "ICloneable",
        "IComparable",
        "IConvertible",
        "IDisposable",
        "IEquatable",
        "IFormatProvider",
        "IFormattable",
        "IndexOutOfRangeException",
        "IntPtr",
        "InvalidCastException",
        "InvalidOperationException",
        "Lazy",
        "Math",
        "MathF",
        "MidpointRounding",
        "MulticastDelegate",
        "NotImplementedException",
        "NotSupportedException",
        "NullReferenceException",
        "Nullable",
        "Object",
        "Obsolete",
        "OperatingSystem",
        "OperationCanceledException",
        "OutOfMemoryException",
        "OverflowException",
        "PlatformNotSupportedException",
        "Predicate",
        "Random",
        "RankException",
        "Serializable",
        "Span",
        "STAThread",
        "String",
        "StringComparer",
        "StringComparison",
        "StringSplitOptions",
        "SystemException",
        "TimeSpan",
        "TimeProvider",
        "TimeoutException",
        "Tuple",
        "Type",
        "TypeCode",
        "TypeInitializationException",
        "UIntPtr",
        "Ulid",
        "UnauthorizedAccessException",
        "Uri",
        "UriBuilder",
        "UriKind",
        "Version",
        "WeakReference",
    ];
    let body = input
        .lines()
        .filter(|line| !line.trim_start().starts_with("using "))
        .collect::<Vec<_>>()
        .join("\n");
    if SYSTEM_IDENTIFIERS
        .iter()
        .any(|identifier| contains_identifier(&body, identifier))
    {
        return input.to_string();
    }
    let mut output = input
        .lines()
        .filter(|line| line.trim() != "using System;")
        .collect::<Vec<_>>()
        .join("\n");
    if input.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn contains_identifier(source: &str, identifier: &str) -> bool {
    source.match_indices(identifier).any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = source[index + identifier.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

fn simplify_known_framework_qualifications(input: &str) -> String {
    let normal_lines = normal_code_lines(input);
    let mut changed = false;
    let mut needs_virtual_document_using = false;
    let mut lines = input
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if normal_lines.get(index) != Some(&true) {
                return line.to_string();
            }
            let mut rewritten = line.to_string();
            if rewritten.contains("System.StringComparison.") {
                changed = true;
                rewritten = rewritten.replace("System.StringComparison.", "StringComparison.");
            }
            if rewritten.contains("Elsa.Services.VirtualDocumentService.IVirtualDocumentService") {
                changed = true;
                needs_virtual_document_using = true;
                rewritten = rewritten.replace(
                    "Elsa.Services.VirtualDocumentService.IVirtualDocumentService",
                    "IVirtualDocumentService",
                );
            }
            rewritten
        })
        .collect::<Vec<_>>();
    if changed
        && input.contains("System.StringComparison.")
        && !lines.iter().any(|line| line.trim() == "using System;")
    {
        if let Some(index) = lines.iter().position(|line| is_using_directive(line)) {
            lines.insert(index, "using System;".to_string());
        }
    }
    if needs_virtual_document_using
        && !lines
            .iter()
            .any(|line| line.trim() == "using Elsa.Services.VirtualDocumentService;")
    {
        if let Some(index) = lines
            .iter()
            .position(|line| line.trim_start().starts_with("using Elsa."))
            .or_else(|| lines.iter().position(|line| is_using_directive(line)))
        {
            lines.insert(
                index,
                "using Elsa.Services.VirtualDocumentService;".to_string(),
            );
        }
    }
    lines.join("\n")
}

fn replace_apparent_var_declarations(input: &str) -> String {
    let normal_lines = normal_code_lines(input);
    input
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if normal_lines.get(index) != Some(&true) {
                return line.to_string();
            }
            replace_apparent_var_declaration(line).unwrap_or_else(|| line.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_verified_redundant_named_arguments(input: &str) -> String {
    let first_parameters = collect_local_first_parameters(input);
    let normal_lines = normal_code_lines(input);
    input
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if normal_lines.get(index) != Some(&true) {
                return line.to_string();
            }
            remove_verified_named_argument(line, &first_parameters)
                .unwrap_or_else(|| line.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_redundant_global_namespace_qualifiers(input: &str) -> String {
    let current_namespace = input.lines().find_map(|line| {
        let namespace = line.trim().strip_prefix("namespace ")?;
        let namespace = namespace.trim_end_matches([';', '{']).trim();
        (!namespace.is_empty()
            && namespace.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            }))
        .then_some(namespace)
    });
    let Some(current_namespace) = current_namespace else {
        return input.to_string();
    };
    let root_namespace = current_namespace
        .split('.')
        .next()
        .unwrap_or(current_namespace);
    let same_namespace = format!("global::{current_namespace}.");
    let qualified = format!("global::{root_namespace}.");
    let replacement = format!("{root_namespace}.");
    let normal_lines = normal_code_lines(input);
    input
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if normal_lines.get(index) == Some(&true) {
                line.replace(&same_namespace, "")
                    .replace(&qualified, &replacement)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_known_framework_named_arguments(input: &str) -> String {
    let normal_lines = normal_code_lines(input);
    input
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if normal_lines.get(index) == Some(&true) {
                line.replace("new CancellationToken(canceled: ", "new CancellationToken(")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_asserted_null_forgiving_operators(input: &str) -> String {
    let normal_lines = normal_code_lines(input);
    let mut asserted: HashMap<String, i32> = HashMap::new();
    let mut brace_depth = 0i32;
    let mut output = Vec::new();
    for (index, line) in input.split('\n').enumerate() {
        if normal_lines.get(index) != Some(&true) {
            output.push(line.to_string());
            continue;
        }
        asserted.retain(|_, assertion_depth| brace_depth >= *assertion_depth);
        let mut rewritten = line.to_string();
        for identifier in asserted.keys() {
            rewritten = rewritten.replace(
                &format!("{identifier}!.SetValue("),
                &format!("{identifier}.SetValue("),
            );
            if rewritten.trim() == format!("return {identifier}!;") {
                rewritten = rewritten.replace(
                    &format!("return {identifier}!;"),
                    &format!("return {identifier};"),
                );
            }
        }
        if let Some(identifier) = asserted_identifier(line) {
            asserted.insert(identifier, brace_depth);
        }
        output.push(rewritten);
        brace_depth += line.chars().filter(|ch| *ch == '{').count() as i32;
        brace_depth -= line.chars().filter(|ch| *ch == '}').count() as i32;
    }
    output.join("\n")
}

fn asserted_identifier(line: &str) -> Option<String> {
    let start = line.find("Assert.IsNotNull(")? + "Assert.IsNotNull(".len();
    let argument = line[start..].split([',', ')']).next()?.trim();
    argument
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        .then(|| argument.to_string())
}

fn collect_local_first_parameters(input: &str) -> HashMap<String, Option<String>> {
    let lines = input.split('\n').collect::<Vec<_>>();
    let normal_lines = normal_code_lines(input);
    let mut parameters: HashMap<String, Option<String>> = HashMap::new();
    let mut index = 0usize;
    while index < lines.len() {
        let start = lines[index].trim();
        if normal_lines.get(index) != Some(&true)
            || !["public ", "private ", "protected ", "internal "]
                .iter()
                .any(|modifier| start.starts_with(modifier))
            || !start.contains('(')
        {
            index += 1;
            continue;
        }
        let mut declaration = start.to_string();
        while !declaration.contains(')') && index + 1 < lines.len() {
            index += 1;
            if normal_lines.get(index) != Some(&true) {
                break;
            }
            declaration.push(' ');
            declaration.push_str(lines[index].trim());
        }
        if let Some((name, parameter)) = local_method_and_first_parameter(&declaration) {
            parameters
                .entry(name)
                .and_modify(|existing| {
                    if existing.as_deref() != Some(parameter.as_str()) {
                        *existing = None;
                    }
                })
                .or_insert(Some(parameter));
        }
        index += 1;
    }
    parameters
}

fn local_method_and_first_parameter(declaration: &str) -> Option<(String, String)> {
    let open = declaration.find('(')?;
    let before = declaration[..open].trim_end();
    let name = before.split_whitespace().last()?;
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let close = declaration[open + 1..].find(')')? + open + 1;
    let first = declaration[open + 1..close].split(',').next()?.trim();
    if first.is_empty() {
        return None;
    }
    let before_default = first.split('=').next()?.trim_end();
    let parameter = before_default.split_whitespace().last()?;
    parameter
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        .then(|| (name.to_string(), parameter.to_string()))
}

fn remove_verified_named_argument(
    line: &str,
    first_parameters: &HashMap<String, Option<String>>,
) -> Option<String> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open || !line[close + 1..].trim().starts_with(';') {
        return None;
    }
    let callee = line[..open].split_whitespace().last()?;
    if !callee
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let argument = line[open + 1..close].trim();
    if argument.contains(',') || argument.contains(['(', ')']) {
        return None;
    }
    let (name, expression) = argument.split_once(':')?;
    if name.trim() != first_parameters.get(callee)?.as_deref()? || expression.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}{}{}",
        &line[..open + 1],
        expression.trim_start(),
        &line[close..]
    ))
}

fn replace_apparent_var_declaration(line: &str) -> Option<String> {
    let indent_len = leading_width(line);
    let (indent, declaration) = line.split_at(indent_len);
    let declaration = declaration.strip_prefix("var ")?;
    let (name, construction) = declaration.split_once(" = new ")?;
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        || construction.starts_with(['{', '['])
    {
        return None;
    }
    let type_end = construction.find(['(', '{'])?;
    let explicit_type = construction[..type_end].trim_end();
    if explicit_type.is_empty()
        || !explicit_type.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '_' | '.' | ':' | '<' | '>' | ',' | '?' | '[' | ']' | ' '
                )
        })
    {
        return None;
    }
    Some(format!(
        "{indent}{explicit_type} {name} = new {construction}"
    ))
}

fn collapse_adjacent_initializer_items(input: &str, max_line_length: usize) -> String {
    let lines = input.split('\n').collect::<Vec<_>>();
    let normal_lines = normal_code_lines(input);
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        if let Some((formatted, consumed)) = expand_moq_verify_lambda(&lines, &normal_lines, index)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            collapse_expression_member_switch(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            expand_inline_array_initializer_argument(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            collapse_anonymous_object_argument(&lines, &normal_lines, index, max_line_length)
        {
            output.push(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            collapse_invocation_lambda_introduction(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if index > 0
            && index + 2 < lines.len()
            && normal_lines[index..=index + 1].iter().all(|normal| *normal)
            && lines[index - 1].trim() == "{"
            && lines[index].trim_end().ends_with(',')
            && lines[index].trim().contains('(')
            && lines[index + 1].trim().contains('(')
            && !lines[index + 1].trim().starts_with(['{', '}', '#'])
            && matches!(lines[index + 2].trim(), "}" | "};")
            && leading_width(lines[index]) == leading_width(lines[index + 1])
        {
            let joined = format!(
                "{} {}",
                lines[index].trim_end(),
                lines[index + 1].trim_start()
            );
            if joined.chars().count() < max_line_length {
                output.push(joined);
                index += 2;
                continue;
            }
        }
        output.push(lines[index].to_string());
        index += 1;
    }

    output.join("\n")
}

fn expand_moq_verify_lambda(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
) -> Option<(Vec<String>, usize)> {
    let first_line = *lines.get(index)?;
    let (verify_prefix, lambda) = first_line.split_once(".Verify(")?;
    let lambda = lambda.trim();
    if normal_lines.get(index) != Some(&true) || !lambda.contains(" => ") || !lambda.ends_with('(')
    {
        return None;
    }
    let mut end = index + 1;
    while end < lines.len() && !lines[end].contains(", Times.") {
        if normal_lines.get(end) != Some(&true) {
            return None;
        }
        end += 1;
    }
    let ending = *lines.get(end)?;
    let (call_end, times_suffix) = ending.rsplit_once(", Times.")?;
    if !times_suffix.ends_with(");") {
        return None;
    }
    let outer_indent = " ".repeat(leading_width(first_line) + 4);
    let mut formatted = vec![
        format!("{verify_prefix}.Verify("),
        format!("{outer_indent}{lambda}"),
    ];
    for line in &lines[index + 1..end] {
        formatted.push(format!("    {line}"));
    }
    formatted.push(format!("    {},", call_end.trim_end()));
    formatted.push(format!("{outer_indent}Times.{times_suffix}"));
    Some((formatted, end - index + 1))
}

fn collapse_expression_member_switch(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let member_line = *lines.get(index)?;
    let switch_line = *lines.get(index + 1)?;
    if normal_lines.get(index) != Some(&true) || normal_lines.get(index + 1) != Some(&true) {
        return None;
    }
    let member = member_line.trim_end();
    let switch = switch_line.trim();
    if !member.ends_with("=>")
        || !switch.ends_with(" switch")
        || !looks_like_member_declaration(member.trim_end_matches("=>").trim_end())
        || member.chars().count() + 1 + switch.chars().count() > max_line_length
        || lines.get(index + 2)?.trim() != "{"
    {
        return None;
    }
    let mut end = index + 3;
    while end < lines.len() && lines[end].trim() != "};" {
        if normal_lines.get(end) != Some(&true) {
            return None;
        }
        end += 1;
    }
    if end >= lines.len() || leading_width(switch_line) <= leading_width(member_line) {
        return None;
    }
    let shift = leading_width(switch_line) - leading_width(member_line);
    let mut formatted = vec![format!("{member} {switch}")];
    for line in &lines[index + 2..=end] {
        if leading_width(line) < shift {
            return None;
        }
        formatted.push(line[shift..].to_string());
    }
    Some((formatted, end - index + 1))
}

fn expand_inline_array_initializer_argument(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let window = lines.get(index..index + 5)?;
    if normal_lines
        .get(index..index + 5)?
        .iter()
        .any(|normal| !normal)
        || !window[0].trim_end().ends_with("(new[]")
        || window[1].trim() != "{"
        || !window[2].trim_end().ends_with(',')
        || window[3].trim().is_empty()
        || window[3].contains("//")
        || window[4].trim() != "});"
        || leading_width(window[1]) != leading_width(window[0])
        || leading_width(window[2]) <= leading_width(window[1])
        || leading_width(window[3]) != leading_width(window[2])
        || leading_width(window[4]) != leading_width(window[0])
    {
        return None;
    }
    let first = window[0].trim_end().strip_suffix("new[]")?;
    let nested_indent = " ".repeat(leading_width(window[0]) + 4);
    let item_indent = format!("{nested_indent}    ");
    let items = format!("{} {}", window[2].trim(), window[3].trim());
    if item_indent.chars().count() + items.chars().count() > max_line_length {
        return None;
    }
    Some((
        vec![
            first.to_string(),
            format!("{nested_indent}new[]"),
            format!("{nested_indent}{{"),
            format!("{item_indent}{items}"),
            format!("{nested_indent}}});"),
        ],
        5,
    ))
}

fn collapse_single_property_initializers(input: &str, max_line_length: usize) -> String {
    let lines = input.split('\n').collect::<Vec<_>>();
    let normal_lines = normal_code_lines(input);
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        if lines[index].trim().contains(" = new ")
            && !lines[index].contains('{')
            && lines.get(index + 1).is_some_and(|line| line.trim() == "{")
        {
            let mut end = index + 2;
            while end < lines.len() && lines[end].trim() != "};" {
                end += 1;
            }
            let properties = &lines[index + 2..end];
            if end < lines.len()
                && properties.len() == 1
                && normal_lines[index..=end].iter().all(|normal| *normal)
                && leading_width(lines[index + 1]) == leading_width(lines[index])
                && leading_width(lines[end]) == leading_width(lines[index])
                && properties.iter().all(|property| {
                    leading_width(property) > leading_width(lines[index + 1])
                        && property.contains(" = ")
                        && !["//", "{", "}"]
                            .iter()
                            .any(|pattern| property.contains(pattern))
                })
            {
                let joined = format!(
                    "{} {{ {} }};",
                    lines[index].trim_end(),
                    properties
                        .iter()
                        .map(|property| property.trim())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim_end_matches(',')
                );
                if joined.chars().count() <= max_line_length {
                    output.push(joined);
                    index = end + 1;
                    continue;
                }
                let property_line = format!(
                    "{}{}",
                    &properties[0][..leading_width(properties[0])],
                    properties
                        .iter()
                        .map(|property| property.trim())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                if property_line.chars().count() < max_line_length {
                    output.push(lines[index].to_string());
                    output.push(lines[index + 1].to_string());
                    output.push(property_line);
                    output.push(lines[end].to_string());
                    index = end + 1;
                    continue;
                }
            }
        }
        output.push(lines[index].to_string());
        index += 1;
    }

    output.join("\n")
}

fn style_enabled(properties: &Properties, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|value| value.split(':').next())
        == Some("true")
}

fn collapse_single_statement_members(input: &str, max_line_length: usize) -> String {
    let lines = input.split('\n').collect::<Vec<_>>();
    let normal_lines = normal_code_lines(input);
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        if index + 3 < lines.len()
            && normal_lines[index..=index + 3].iter().all(|normal| *normal)
            && can_collapse_single_statement_member(&lines[index..=index + 3])
        {
            let signature = lines[index].trim_end();
            let statement = lines[index + 2].trim();
            let expression = statement.strip_prefix("return ").unwrap_or(statement);
            let joined = format!("{signature} => {expression}");
            if joined.chars().count() < max_line_length {
                output.push(joined);
            } else {
                output.push(format!("{signature} =>"));
                output.push(lines[index + 2].to_string());
            }
            index += 4;
        } else {
            output.push(lines[index].to_string());
            index += 1;
        }
    }

    output.join("\n")
}

fn can_collapse_single_statement_member(lines: &[&str]) -> bool {
    let signature = lines[0].trim();
    let statement = lines[2].trim();
    looks_like_member_declaration(signature)
        && signature.ends_with(')')
        && lines[1].trim() == "{"
        && statement.ends_with(';')
        && !statement.contains("//")
        && lines[3].trim() == "}"
        && leading_width(lines[1]) == leading_width(lines[0])
        && leading_width(lines[2]) > leading_width(lines[1])
        && leading_width(lines[3]) == leading_width(lines[0])
}

fn collapse_simple_wrapping(input: &str, max_line_length: usize) -> String {
    let lines = input.split('\n').collect::<Vec<_>>();
    let normal_lines = normal_code_lines(input);
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        if let Some((formatted, consumed)) =
            align_expression_bodied_boolean_chain(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            collapse_expression_member_assert_lambda(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) =
            collapse_typed_lambda_introduction(&lines, &normal_lines, index, max_line_length)
        {
            output.extend(formatted);
            index += consumed;
            continue;
        }
        if index + 1 < lines.len()
            && normal_lines.get(index) == Some(&true)
            && normal_lines.get(index + 1) == Some(&true)
        {
            if let Some((introduction, body)) = collapse_long_expression_lambda_pair(
                lines[index],
                lines[index + 1],
                max_line_length,
            ) {
                output.push(introduction);
                output.push(body);
                index += 2;
                continue;
            }
        }
        if index + 1 < lines.len()
            && normal_lines.get(index) == Some(&true)
            && normal_lines.get(index + 1) == Some(&true)
            && (!lines[index + 1].trim_end().ends_with("=>")
                || (lines[index + 1].trim_start().starts_with("() =>")
                    && lines
                        .get(index + 2)
                        .is_some_and(|line| line.trim_end().ends_with(");"))))
            && can_collapse_pair(lines[index], lines[index + 1], max_line_length)
        {
            let separator = if lines[index].trim_end().ends_with('(') {
                ""
            } else {
                " "
            };
            output.push(format!(
                "{}{}{}",
                lines[index].trim_end(),
                separator,
                lines[index + 1].trim_start()
            ));
            index += 2;
        } else {
            output.push(lines[index].to_string());
            index += 1;
        }
    }

    output.join("\n")
}

fn collapse_typed_lambda_introduction(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let first_line = *lines.get(index)?;
    let lambda_line = *lines.get(index + 1)?;
    let body_line = *lines.get(index + 2)?;
    if normal_lines
        .get(index..=index + 2)?
        .iter()
        .any(|normal| !normal)
    {
        return None;
    }
    let first = first_line.trim_end();
    let lambda = lambda_line.trim();
    if !first.ends_with('(')
        || !lambda.starts_with('(')
        || !lambda.ends_with("=>")
        || !lambda.contains(") =>")
        || !body_line.trim_end().ends_with(");")
        || leading_width(lambda_line) <= leading_width(first_line)
        || leading_width(body_line) <= leading_width(lambda_line)
        || first.chars().count() + lambda.chars().count() > max_line_length
    {
        return None;
    }
    let body_indent = " ".repeat(leading_width(first_line) + 4);
    Some((
        vec![
            format!("{first}{lambda}"),
            format!("{body_indent}{}", body_line.trim()),
        ],
        3,
    ))
}

fn align_expression_bodied_boolean_chain(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let introduction = lines.get(index)?.trim_end();
    if !introduction.ends_with("=>")
        || !looks_like_member_declaration(introduction.trim_end_matches("=>").trim_end())
    {
        return None;
    }

    let first_expression = lines.get(index + 1)?.trim();
    if normal_lines.get(index + 1) != Some(&true)
        || !first_expression.ends_with("&&")
        || introduction.chars().count() + 1 + first_expression.chars().count() > max_line_length
    {
        return None;
    }

    let mut end = index + 2;
    while let Some(line) = lines.get(end) {
        if normal_lines.get(end) != Some(&true)
            || leading_width(line) != leading_width(lines[index + 1])
        {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.ends_with(';') {
            break;
        }
        if !trimmed.ends_with("&&") {
            return None;
        }
        end += 1;
    }
    if end >= lines.len() || end == index + 2 {
        return None;
    }

    let mut formatted = Vec::with_capacity(end - index + 1);
    formatted.push(format!("{introduction} {first_expression}"));
    let alignment = " ".repeat(introduction.chars().count() + 1);
    for line in &lines[index + 2..=end] {
        formatted.push(format!("{alignment}{}", line.trim()));
    }
    Some((formatted, end - index + 1))
}

fn collapse_expression_member_assert_lambda(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let first_line = *lines.get(index)?;
    let second_line = *lines.get(index + 1)?;
    if normal_lines.get(index) != Some(&true) || normal_lines.get(index + 1) != Some(&true) {
        return None;
    }
    let first = first_line.trim_end();
    let second = second_line.trim();
    if !first.ends_with("=>")
        || !first.contains('(')
        || !looks_like_member_declaration(first.trim_end_matches("=>").trim_end())
        || !second.starts_with("Assert.")
        || !second.ends_with("=>")
        || first.chars().count() + 1 + second.chars().count() > max_line_length
    {
        return None;
    }

    let shift = leading_width(second_line).checked_sub(leading_width(first_line))?;
    if shift == 0 {
        return None;
    }
    let mut formatted = vec![format!("{first} {second}")];
    let mut cursor = index + 2;
    loop {
        let line = *lines.get(cursor)?;
        if normal_lines.get(cursor) != Some(&true) || leading_width(line) < shift {
            return None;
        }
        formatted.push(line[shift..].to_string());
        cursor += 1;
        if line.trim_end().ends_with("));") {
            break;
        }
    }
    Some((formatted, cursor - index))
}

fn collapse_long_expression_lambda_pair(
    first: &str,
    second: &str,
    max_line_length: usize,
) -> Option<(String, String)> {
    let first = first.trim_end();
    let second_trimmed = second.trim();
    let expression = second_trimmed.strip_prefix("() => ")?;
    if !first.ends_with('(') || !expression.ends_with(");") {
        return None;
    }
    let joined = format!("{first}() => {expression}");
    let introduction = format!("{first}() =>");
    if joined.chars().count() < max_line_length || introduction.chars().count() >= max_line_length {
        return None;
    }
    let indent = &second[..leading_width(second)];
    Some((introduction, format!("{indent}{expression}")))
}

fn can_collapse_pair(first: &str, second: &str, max_line_length: usize) -> bool {
    let first_trimmed = first.trim_end();
    let second_trimmed = second.trim();
    if first.contains("//")
        || second.contains("//")
        || second_trimmed.is_empty()
        || second_trimmed.starts_with(['#', '{', '}'])
        || leading_width(second) <= leading_width(first)
    {
        return false;
    }

    let expression_bodied_property = first_trimmed.ends_with("=>")
        && second_trimmed.ends_with(';')
        && !second_trimmed.contains("=>")
        && !first_trimmed.contains('(')
        && looks_like_member_declaration(first_trimmed.trim_end_matches("=>").trim_end());
    let single_string_argument = first_trimmed.ends_with('(')
        && second_trimmed.ends_with(");")
        && second_trimmed.starts_with(['"', '$', '@']);
    let single_simple_argument = first_trimmed.ends_with('(')
        && (second_trimmed.ends_with("),") || second_trimmed.ends_with(");"))
        && !["=>", "//", "{"]
            .iter()
            .any(|pattern| second_trimmed.contains(pattern))
        && balanced_delimiters(&format!("{first_trimmed}{second_trimmed}"));
    let single_assert_argument = first_trimmed.ends_with('(')
        && first_trimmed.contains("Assert.")
        && second_trimmed.ends_with(");");
    let expression_method_to_assert = first_trimmed.ends_with("=>")
        && first_trimmed.contains('(')
        && looks_like_member_declaration(first_trimmed.trim_end_matches("=>").trim_end())
        && second_trimmed.starts_with("Assert.");
    let invocation_to_lambda = first_trimmed.ends_with('(')
        && second_trimmed.ends_with("=>")
        && second_trimmed.starts_with("() =>");
    let member_declaration = first_trimmed.ends_with('(')
        && second_trimmed.ends_with(')')
        && looks_like_member_declaration(first_trimmed);
    let joined_length = first_trimmed.chars().count() + 1 + second_trimmed.chars().count();
    (expression_bodied_property
        || single_string_argument
        || single_simple_argument
        || single_assert_argument
        || expression_method_to_assert
        || invocation_to_lambda
        || member_declaration)
        && joined_length < max_line_length
}

fn collapse_anonymous_object_argument(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(String, usize)> {
    let window = lines.get(index..index + 5)?;
    if normal_lines
        .get(index..index + 5)?
        .iter()
        .any(|normal| !normal)
        || window[0].trim() != "new"
        || window[1].trim() != "{"
        || !window[2].trim_end().ends_with(',')
        || window[2].contains("//")
        || window[3].contains("//")
        || !window[4].trim_start().starts_with("}")
        || leading_width(window[1]) != leading_width(window[0])
        || leading_width(window[2]) <= leading_width(window[1])
        || leading_width(window[3]) != leading_width(window[2])
        || leading_width(window[4]) != leading_width(window[0])
    {
        return None;
    }
    let suffix = window[4].trim_start().strip_prefix('}')?;
    let joined = format!(
        "{}new {{ {} {} }}{}",
        &window[0][..leading_width(window[0])],
        window[2].trim(),
        window[3].trim(),
        suffix
    );
    (joined.chars().count() <= max_line_length).then_some((joined, 5))
}

fn collapse_invocation_lambda_introduction(
    lines: &[&str],
    normal_lines: &[bool],
    index: usize,
    max_line_length: usize,
) -> Option<(Vec<String>, usize)> {
    let first_line = *lines.get(index)?;
    let lambda_line = *lines.get(index + 1)?;
    if normal_lines.get(index) != Some(&true) || normal_lines.get(index + 1) != Some(&true) {
        return None;
    }
    let first = first_line.trim_end();
    let lambda = lambda_line.trim();
    if !first.ends_with('(')
        || !lambda.contains(" => ")
        || !lambda.ends_with('(')
        || first.chars().count() + lambda.chars().count() > max_line_length
        || leading_width(lambda_line) <= leading_width(first_line)
    {
        return None;
    }

    let continuation_indent = " ".repeat(leading_width(first_line) + 8);
    let mut formatted = vec![format!("{first}{lambda}")];
    let mut cursor = index + 2;
    loop {
        let line = *lines.get(cursor)?;
        if normal_lines.get(cursor) != Some(&true)
            || leading_width(line) <= leading_width(first_line)
            || line.trim().is_empty()
        {
            return None;
        }
        formatted.push(format!("{continuation_indent}{}", line.trim()));
        cursor += 1;
        if line.trim_end().ends_with("))") {
            break;
        }
    }
    Some((formatted, cursor - index))
}

fn balanced_delimiters(value: &str) -> bool {
    let mut parentheses = 0i32;
    let mut brackets = 0i32;
    for ch in value.chars() {
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            _ => {}
        }
        if parentheses < 0 || brackets < 0 {
            return false;
        }
    }
    parentheses == 0 && brackets == 0
}

fn looks_like_member_declaration(line: &str) -> bool {
    let trimmed = line.trim_start();
    let has_member_modifier = ["public ", "private ", "protected ", "internal "]
        .iter()
        .any(|modifier| trimmed.starts_with(modifier));
    let before_paren = trimmed.trim_end_matches('(');
    has_member_modifier && !before_paren.contains(['.', '=', '?'])
}

fn leading_width(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn normal_code_lines(input: &str) -> Vec<bool> {
    let mut flags = Vec::new();
    let mut state = CodeState::Normal;
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut line_normal = true;

    while index < chars.len() {
        let ch = chars[index];
        match state {
            CodeState::Normal => {
                if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    state = CodeState::BlockComment;
                    line_normal = false;
                    index += 2;
                    continue;
                }
                if ch == '/' && chars.get(index + 1) == Some(&'/') {
                    state = CodeState::LineComment;
                    index += 2;
                    continue;
                }
                if let Some((literal_len, verbatim)) = string_literal_start(&chars, index) {
                    state = CodeState::String { verbatim };
                    index += literal_len;
                    continue;
                }
                if ch == '\'' {
                    state = CodeState::Char;
                }
                index += 1;
            }
            CodeState::LineComment => {
                index += 1;
                if ch == '\n' {
                    state = CodeState::Normal;
                }
            }
            CodeState::BlockComment => {
                line_normal = false;
                if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    state = CodeState::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            CodeState::String { verbatim } => {
                if verbatim && ch == '\n' {
                    line_normal = false;
                }
                if verbatim && ch == '"' && chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                if ch == '"' {
                    state = CodeState::Normal;
                } else if !verbatim && ch == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
            }
            CodeState::Char => {
                if ch == '\'' {
                    state = CodeState::Normal;
                } else if ch == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
            }
        }

        if ch == '\n' {
            flags.push(line_normal && matches!(state, CodeState::Normal));
            line_normal = matches!(state, CodeState::Normal);
        }
    }
    flags.push(line_normal && matches!(state, CodeState::Normal));
    flags
}

pub(crate) fn verbatim_string_lines(input: &str) -> Vec<bool> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut flags = Vec::new();
    let mut state = CodeState::Normal;
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        match state {
            CodeState::Normal => {
                if ch == '/' && chars.get(index + 1) == Some(&'/') {
                    state = CodeState::LineComment;
                    index += 2;
                    continue;
                }
                if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    state = CodeState::BlockComment;
                    index += 2;
                    continue;
                }
                if let Some((literal_len, verbatim)) = string_literal_start(&chars, index) {
                    state = CodeState::String { verbatim };
                    index += literal_len;
                    continue;
                }
                if ch == '\'' {
                    state = CodeState::Char;
                }
                index += 1;
            }
            CodeState::LineComment => {
                index += 1;
                if ch == '\n' {
                    state = CodeState::Normal;
                }
            }
            CodeState::BlockComment => {
                if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    state = CodeState::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            CodeState::String { verbatim } => {
                if verbatim && ch == '"' && chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                if ch == '"' {
                    state = CodeState::Normal;
                } else if !verbatim && ch == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
            }
            CodeState::Char => {
                if ch == '\'' {
                    state = CodeState::Normal;
                } else if ch == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
            }
        }

        if ch == '\n' {
            flags.push(matches!(state, CodeState::String { verbatim: true }));
        }
    }
    flags.push(matches!(state, CodeState::String { verbatim: true }));
    flags
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
    let input = normalize_alias_using_trivia(input);
    let mut output = Vec::new();
    let lines: Vec<&str> = input.split('\n').collect();
    let mut index = 0usize;
    let mut in_header = true;

    while index < lines.len() {
        if !in_header {
            output.push(lines[index].to_string());
            index += 1;
            continue;
        }

        if is_using_directive(lines[index]) {
            let start = index;
            while index < lines.len()
                && (is_using_directive(lines[index])
                    || (lines[index].trim().is_empty()
                        && index > start
                        && is_using_alias_directive(lines[index - 1])
                        && next_nonblank_is_using_alias_directive(&lines, index + 1)))
            {
                index += 1;
            }
            output.extend(format_using_block(&lines[start..index], options));
        } else {
            if !is_header_trivia(lines[index]) {
                in_header = false;
            }
            output.push(lines[index].to_string());
            index += 1;
        }
    }

    output.join("\n")
}

fn normalize_alias_using_trivia(input: &str) -> String {
    let lines = input.split('\n').collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty()
            && index > 0
            && is_using_directive(lines[index - 1])
            && comment_block_is_followed_by_alias_using(&lines, index + 1)
        {
            continue;
        }
        output.push((*line).to_string());
    }
    output.join("\n")
}

fn comment_block_is_followed_by_alias_using(lines: &[&str], mut index: usize) -> bool {
    let mut saw_comment = false;
    while let Some(line) = lines.get(index) {
        if line.trim_start().starts_with("//") {
            saw_comment = true;
            index += 1;
            continue;
        }
        return saw_comment && is_using_alias_directive(line);
    }
    false
}

fn is_header_trivia(line: &str) -> bool {
    let trimmed = line.trim_start_matches('\u{feff}').trim_start();

    trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
        || trimmed.starts_with('#')
}

fn next_nonblank_is_using_alias_directive(lines: &[&str], mut index: usize) -> bool {
    while index < lines.len() {
        if !lines[index].trim().is_empty() {
            return is_using_alias_directive(lines[index]);
        }
        index += 1;
    }

    false
}

fn is_using_directive(line: &str) -> bool {
    if line.trim_start() != line {
        return false;
    }

    let trimmed = line.trim_start();
    let Some(rest) = trimmed
        .strip_prefix("using ")
        .or_else(|| trimmed.strip_prefix("global using "))
    else {
        return false;
    };

    rest.ends_with(';') && !rest.contains('{') && !rest.contains('}')
}

fn is_using_alias_directive(line: &str) -> bool {
    is_using_directive(line) && using_sort_key(line).contains('=')
}

fn format_using_block(lines: &[&str], options: &CSharpOptions) -> Vec<String> {
    let indent = lines
        .iter()
        .find(|line| is_using_directive(line))
        .and_then(|line| line.get(..line.len() - line.trim_start().len()))
        .unwrap_or("");
    let mut directives = Vec::new();
    let mut seen = BTreeSet::new();
    for directive in lines
        .iter()
        .filter(|line| is_using_directive(line))
        .map(|line| {
            line.trim()
                .replace(" = global::Microsoft.", " = Microsoft.")
                .replace(" = global::System.", " = System.")
        })
    {
        if seen.insert(directive.clone()) {
            directives.push(directive);
        }
    }

    if directives
        .iter()
        .all(|directive| !is_using_alias_directive(directive))
    {
        directives.sort_by(|left, right| compare_using_directives(left, right, options));
    }

    if options.sort_system_directives_first && options.separate_import_directive_groups {
        let first_non_system = directives
            .iter()
            .position(|line| !is_system_using(line) && !is_using_alias_directive(line));
        if let Some(split) = first_non_system.filter(|split| {
            *split > 0
                && *split < directives.len()
                && lines.iter().any(|line| line.trim().is_empty())
        }) {
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

    using_sort_key(left)
        .to_ascii_lowercase()
        .cmp(&using_sort_key(right).to_ascii_lowercase())
}

fn is_system_using(line: &str) -> bool {
    let target = using_sort_key(line);

    target == "System" || target.starts_with("System.") || target.starts_with("static System.")
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

fn normalize_token_spacing(input: &str, _options: &CSharpOptions) -> String {
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

                if let Some((literal_len, verbatim)) = string_literal_start(&chars, index) {
                    for offset in 0..literal_len {
                        output.push(chars[index + offset]);
                    }
                    index += literal_len;
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

                if let Some(keyword_len) = control_keyword_before_paren_len(&chars, index) {
                    for offset in 0..keyword_len {
                        output.push(chars[index + offset]);
                    }
                    output.push(' ');
                    index += keyword_len;
                    line_start = false;
                    continue;
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

fn string_literal_start(chars: &[char], index: usize) -> Option<(usize, bool)> {
    match (chars.get(index), chars.get(index + 1), chars.get(index + 2)) {
        (Some('"'), _, _) => Some((1, false)),
        (Some('@'), Some('"'), _) => Some((2, true)),
        (Some('$'), Some('"'), _) => Some((2, false)),
        (Some('@'), Some('$'), Some('"')) => Some((3, true)),
        (Some('$'), Some('@'), Some('"')) => Some((3, true)),
        _ => None,
    }
}

fn control_keyword_before_paren_len(chars: &[char], index: usize) -> Option<usize> {
    const KEYWORDS: &[&str] = &[
        "foreach", "for", "if", "while", "switch", "catch", "using", "lock", "fixed", "when",
    ];

    KEYWORDS.iter().find_map(|keyword| {
        let len = keyword.len();
        (chars_start_with(chars, index, keyword)
            && keyword_boundary_before(chars, index)
            && chars.get(index + len) == Some(&'('))
        .then_some(len)
    })
}

fn keyword_boundary_before(chars: &[char], index: usize) -> bool {
    index == 0
        || chars
            .get(index - 1)
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && *ch != '_')
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn is_control_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "for" | "foreach" | "while" | "switch" | "catch" | "using" | "lock" | "fixed"
    )
}

#[allow(dead_code)]
fn skip_horizontal_space(chars: &[char], index: &mut usize) {
    while matches!(chars.get(*index), Some(' ' | '\t')) {
        *index += 1;
    }
}

#[allow(dead_code)]
fn needs_space_before(output: &str) -> bool {
    output
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_whitespace() && ch != '(' && ch != '[' && ch != '{')
}

#[allow(dead_code)]
fn needs_space_after(chars: &[char], index: usize) -> bool {
    chars
        .get(index)
        .is_some_and(|ch| !ch.is_whitespace() && !matches!(ch, ')' | ']' | '}' | ';' | ','))
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn previous_non_space(chars: &[char], index: usize) -> Option<char> {
    chars
        .get(..index)?
        .iter()
        .rev()
        .find(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r'))
        .copied()
}

#[allow(dead_code)]
fn can_precede_binary_plus_or_minus(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | ')' | ']' | '}')
}

fn normalize_control_flow_newlines(input: &str, options: &CSharpOptions) -> String {
    let mut output = String::with_capacity(input.len());
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut state = CodeState::Normal;
    let mut line_start = true;
    let mut current_indent = String::new();

    while index < chars.len() {
        let ch = chars[index];
        match state {
            CodeState::Normal => {
                if line_start {
                    if matches!(ch, ' ' | '\t') {
                        current_indent.push(ch);
                    } else {
                        line_start = false;
                    }
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

                if let Some((literal_len, verbatim)) = string_literal_start(&chars, index) {
                    for offset in 0..literal_len {
                        output.push(chars[index + offset]);
                    }
                    index += literal_len;
                    state = CodeState::String { verbatim };
                    continue;
                }

                if ch == '\'' {
                    output.push(ch);
                    index += 1;
                    state = CodeState::Char;
                    continue;
                }

                if let Some(keyword_len) = control_flow_keyword_len(&chars, index, options) {
                    if previous_output_non_space_on_current_line(&output) == Some('}') {
                        trim_horizontal_space(&mut output);
                        output.push('\n');
                        output.push_str(&current_indent);
                        for offset in 1..keyword_len {
                            output.push(chars[index + offset]);
                        }
                        index += keyword_len;
                        line_start = false;
                        continue;
                    }
                }

                output.push(ch);
                index += 1;
                if ch == '\n' {
                    line_start = true;
                    current_indent.clear();
                }
            }
            CodeState::LineComment => {
                output.push(ch);
                index += 1;
                if ch == '\n' {
                    state = CodeState::Normal;
                    line_start = true;
                    current_indent.clear();
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
                if ch == '\n' {
                    line_start = true;
                    current_indent.clear();
                }
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
            }
        }
    }

    output
}

fn control_flow_keyword_len(
    chars: &[char],
    index: usize,
    options: &CSharpOptions,
) -> Option<usize> {
    [
        (" else", options.new_line_before_else),
        (" catch", options.new_line_before_catch),
        (" finally", options.new_line_before_finally),
    ]
    .into_iter()
    .find_map(|(keyword, enabled)| {
        (enabled
            && chars_start_with(chars, index, keyword)
            && keyword_boundary(chars, index + keyword.len()))
        .then_some(keyword.len())
    })
}

fn chars_start_with(chars: &[char], index: usize, pattern: &str) -> bool {
    pattern
        .chars()
        .enumerate()
        .all(|(offset, ch)| chars.get(index + offset) == Some(&ch))
}

fn keyword_boundary(chars: &[char], index: usize) -> bool {
    chars
        .get(index)
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && *ch != '_')
}

fn previous_output_non_space_on_current_line(output: &str) -> Option<char> {
    output
        .chars()
        .rev()
        .take_while(|ch| !matches!(ch, '\n' | '\r'))
        .find(|ch| !matches!(ch, ' ' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> CSharpOptions {
        CSharpOptions {
            interface_layout: None,
            sort_usings: true,
            arrange_fields: true,
            remove_clearly_unused_usings: false,
            reorder_modifiers: true,
            normalize_spacing: true,
            normalize_newlines: true,
            collapse_simple_wrapping: true,
            prefer_expression_bodied_members: true,
            prefer_explicit_type_when_apparent: true,
            max_line_length: 120,
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
    fn production_options_preserve_layout_without_a_semantic_model() {
        let properties = Properties::from_pairs(&[(
            "resharper_keep_existing_invocation_parens_arrangement",
            "false",
        )]);
        let options = CSharpOptions::from_properties(&properties);
        assert!(!options.arrange_fields);
        assert!(!options.remove_clearly_unused_usings);

        let input = "using System;\nusing System.Collections.Generic;\n\nclass C\n{\n    [SetUp]\n    public void SetUp() {}\n\n    private object _app;\n\n    void M()\n    {\n        Item item = new Item\n        {\n            Field = \"field\",\n            Role = \"role\",\n            Label = \"label\"\n        };\n    }\n}\n";
        assert_eq!(format_csharp(input, options), input);
    }

    #[test]
    fn sorts_and_deduplicates_using_blocks() {
        let input = "using Elsa;\nusing System.Text;\nusing System;\nusing Elsa;\n\nclass C {}\n";

        assert_eq!(
            format_csharp(input, options()),
            "using System;\nusing System.Text;\nusing Elsa;\n\nclass C {}\n"
        );
    }

    #[test]
    fn normalizes_blank_lines_inside_using_blocks() {
        let input = "using System;\nusing R3;\n\nusing Task = System.Threading.Tasks.Task;\n\nusing Range = Microsoft.Office.Interop.Word.Range;\n\nnamespace N {}\n";

        assert_eq!(
            format_csharp(input, options()),
            "using System;\nusing R3;\n\nusing Task = System.Threading.Tasks.Task;\nusing Range = Microsoft.Office.Interop.Word.Range;\n\nnamespace N {}\n"
        );
    }

    #[test]
    fn does_not_sort_local_using_statements() {
        let mut options = options();
        options.reorder_modifiers = false;
        options.normalize_spacing = false;

        let input = "using System.Net.Http;\nusing System.Text;\n\nclass C\n{\n    async System.Threading.Tasks.Task M(System.Uri endpoint, HttpClient client, string json, System.Threading.CancellationToken cancellationToken)\n    {\n        using StringContent content = new StringContent(json, Encoding.UTF8, \"application/json\");\n        using HttpResponseMessage response = await client.PostAsync(endpoint, content, cancellationToken);\n    }\n}\n";

        assert_eq!(format_csharp(input, options), input);
    }

    #[test]
    fn does_not_sort_using_text_after_header() {
        let mut options = options();
        options.reorder_modifiers = false;
        options.normalize_spacing = false;

        let input = "namespace N\n{\n    class C\n    {\n        const string Source = @\"\nusing System.Threading;\nusing Microsoft.Office.Interop.Word;\n\";\n    }\n}\n";

        assert_eq!(format_csharp(input, options), input);
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
            "class C\n{\n    void M(){ var x=a+b; Call( a ,b,c ); for (i=0;i<10;i+=1){} }\n}\n"
        );
    }

    #[test]
    fn leaves_comments_and_strings_unchanged() {
        let input = "class C\n{\n    string S = \"if(x)\"; // if(x)\n    string V = @\"for(x)\";\n    string I = $@\"when(x)\";\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    string S = \"if(x)\"; // if(x)\n    string V = @\"for(x)\";\n    string I = $@\"when(x)\";\n}\n"
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

    #[test]
    fn can_run_only_newline_passes() {
        let mut options = options();
        options.sort_usings = false;
        options.reorder_modifiers = false;
        options.normalize_spacing = false;

        let input = "using Z;\nusing A;\nclass C\n{\n    static private string Value;\n    void M(){ if(x){} else {} }\n}\n";

        assert_eq!(
            format_csharp(input, options),
            "using Z;\nusing A;\nclass C\n{\n    static private string Value;\n    void M(){ if(x){}\n    else {} }\n}\n"
        );
    }

    #[test]
    fn newline_pass_ignores_comments_and_strings() {
        let mut options = options();
        options.sort_usings = false;
        options.reorder_modifiers = false;
        options.normalize_spacing = false;

        let input = "class C\n{\n    string Script = \"try {} catch (_) {}\";\n    // } catch (_) {\n    void M(){ try {} catch {} }\n}\n";

        assert_eq!(
            format_csharp(input, options),
            "class C\n{\n    string Script = \"try {} catch (_) {}\";\n    // } catch (_) {\n    void M(){ try {}\n    catch {} }\n}\n"
        );
    }

    #[test]
    fn newline_pass_does_not_add_blank_line_before_existing_catch() {
        let mut options = options();
        options.sort_usings = false;
        options.reorder_modifiers = false;
        options.normalize_spacing = false;

        let input = "class C\n{\n    void M()\n    {\n        try\n        {\n        }\n        catch\n        {\n        }\n    }\n}\n";

        assert_eq!(format_csharp(input, options), input);
    }

    #[test]
    fn collapses_simple_wrapping_that_fits_the_margin() {
        let input = "class C\n{\n    public bool Enabled =>\n        Availability.IsEnabled(s_enabled);\n\n    void M()\n    {\n        Logger.Log(\n            \"disabled\");\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    public bool Enabled => Availability.IsEnabled(s_enabled);\n\n    void M()\n    {\n        Logger.Log(\"disabled\");\n    }\n}\n"
        );
    }

    #[test]
    fn collapses_single_line_assert_arguments() {
        let input = "class C\n{\n    void M()\n    {\n        Assert.IsTrue(\n            Parser.TryParse(\"value\", out string result));\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Assert.IsTrue(Parser.TryParse(\"value\", out string result));\n    }\n}\n"
        );
    }

    #[test]
    fn preserves_long_and_verbatim_string_wrapping() {
        let input = "class C\n{\n    string Script = @\"call(\n        value);\";\n    public bool Enabled =>\n        ThisExpressionIsFarTooLongToFitInsideTheConfiguredRightMarginBecauseItContainsManyWords(s_enabled);\n}\n";

        assert_eq!(format_csharp(input, options()), input);
    }

    #[test]
    fn converts_single_statement_member_to_expression_body() {
        let input = "class C\n{\n    public C(IReadOnlyList<string> values)\n    {\n        _values = values ?? throw new ArgumentNullException(nameof(values));\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    public C(IReadOnlyList<string> values) => _values = values ?? throw new ArgumentNullException(nameof(values));\n}\n"
        );
    }

    #[test]
    fn identifies_lines_inside_verbatim_strings() {
        assert_eq!(
            verbatim_string_lines("var xml = @\"<a> \n  <b /> \n</a>\"; \n"),
            vec![true, true, false, false]
        );
    }

    #[test]
    fn rejects_candidate_that_breaks_parseable_csharp() {
        let input = "class C { void M() { } }\n";
        assert_eq!(
            super::preserve_parseable_input(input, "class C { void M( { } }\n".to_string()),
            input
        );
    }

    #[test]
    fn does_not_hide_existing_parse_errors() {
        let input = "class C { void M( { } }\n";
        let candidate = "class C { void M( { int value = 1; } }\n".to_string();
        assert_eq!(
            super::preserve_parseable_input(input, candidate.clone()),
            candidate
        );
    }

    #[test]
    fn collapses_single_property_object_initializer() {
        let input = "class C\n{\n    void M()\n    {\n        Candidate candidate = new Candidate\n        {\n            Field = \"value\"\n        };\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Candidate candidate = new Candidate { Field = \"value\" };\n    }\n}\n"
        );
    }

    #[test]
    fn collapses_short_initializer_items() {
        let input =
            "VirtualParagraph[] Values() => new[]\n{\n    Make(0, 10),\n    Make(10, 20)\n};\n";

        assert_eq!(
            format_csharp(input, options()),
            "VirtualParagraph[] Values() => new[]\n{\n    Make(0, 10), Make(10, 20)\n};\n"
        );
    }

    #[test]
    fn removes_blank_line_before_documented_alias_using() {
        let input = "using System;\n\n// Avoid an ambiguous type name.\nusing Range = Word.Range;\n\nclass C {}\n";

        assert_eq!(
            format_csharp(input, options()),
            "using System;\n// Avoid an ambiguous type name.\nusing Range = Word.Range;\n\nclass C {}\n"
        );
    }

    #[test]
    fn joins_invocation_and_lambda_introduction() {
        let input = "class C\n{\n    void M()\n    {\n        dispatcher.PostAsync(\n            () =>\n                Clear());\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        dispatcher.PostAsync(() =>\n            Clear());\n    }\n}\n"
        );
    }

    #[test]
    fn splits_long_expression_lambda_after_arrow() {
        let input = "class C\n{\n    void M()\n    {\n        await dispatcher.PostAsync(\n            () => controller.ResolveCurrentDocumentAndClearAllDecorations());\n    }\n}\n";
        let mut options = options();
        options.max_line_length = 80;

        assert_eq!(
            format_csharp(input, options),
            "class C\n{\n    void M()\n    {\n        await dispatcher.PostAsync(() =>\n            controller.ResolveCurrentDocumentAndClearAllDecorations());\n    }\n}\n"
        );
    }

    #[test]
    fn aligns_expression_bodied_boolean_chain() {
        let input = "class C\n{\n    public bool Equals(C other) =>\n        Start == other.Start &&\n        End == other.End &&\n        Limit == other.Limit;\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    public bool Equals(C other) => Start == other.Start &&\n                                   End == other.End &&\n                                   Limit == other.Limit;\n}\n"
        );
    }

    #[test]
    fn joins_expression_member_and_assert_lambda() {
        let input = "class C\n{\n    public void Throws() =>\n        Assert.ThrowsException<ArgumentException>(() =>\n            Run());\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    public void Throws() => Assert.ThrowsException<ArgumentException>(() =>\n        Run());\n}\n"
        );
    }

    #[test]
    fn replaces_var_for_apparent_object_creation() {
        let input =
            "class C\n{\n    void M()\n    {\n        var items = new List<object>(4);\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        List<object> items = new List<object>(4);\n    }\n}\n"
        );
    }

    #[test]
    fn replaces_var_for_generic_object_creation() {
        let input = "class C\n{\n    void M()\n    {\n        var values = new Dictionary<string, object>();\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Dictionary<string, object> values = new Dictionary<string, object>();\n    }\n}\n"
        );
    }

    #[test]
    fn collapses_nested_single_argument_invocation() {
        let input = "class C\n{\n    void M()\n    {\n        Call(\n            Inner(\n                value),\n            other);\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Call(\n            Inner(value),\n            other);\n    }\n}\n"
        );
    }

    #[test]
    fn collapses_two_property_anonymous_object_argument() {
        let input = "class C\n{\n    void M()\n    {\n        Serialize(\n            new\n            {\n                type = \"event\",\n                payload = new { error }\n            }));\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Serialize(\n            new { type = \"event\", payload = new { error } }));\n    }\n}\n"
        );
    }

    #[test]
    fn collapses_invocation_lambda_introduction() {
        let input = "class C\n{\n    object M()\n    {\n        return values.Select(\n                value => Build(\n                    value.Name,\n                    value.Offset))\n            .ToList();\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    object M()\n    {\n        return values.Select(value => Build(\n                value.Name,\n                value.Offset))\n            .ToList();\n    }\n}\n"
        );
    }

    #[test]
    fn removes_named_argument_verified_against_local_method() {
        let input = "class C\n{\n    void M()\n    {\n        Send(loading: true);\n    }\n\n    private void Send(\n        bool loading = false,\n        string message = null)\n    {\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Send(true);\n    }\n\n    private void Send(\n        bool loading = false,\n        string message = null)\n    {\n    }\n}\n"
        );
    }

    #[test]
    fn preserves_named_argument_when_local_overloads_disagree() {
        let input = "class C\n{\n    void M()\n    {\n        Send(loading: true);\n    }\n\n    private void Send(bool loading) {}\n    private void Send(string message) {}\n}\n";

        assert_eq!(format_csharp(input, options()), input);
    }

    #[test]
    fn removes_global_qualifier_for_declared_root_namespace() {
        let input = "namespace Elsa.Forms\n{\n    class C\n    {\n        global::Elsa.Services.IService service;\n        global::Elsa.Forms.Dialog dialog;\n    }\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "namespace Elsa.Forms\n{\n    class C\n    {\n        Elsa.Services.IService service;\n        Dialog dialog;\n    }\n}\n"
        );
    }

    #[test]
    fn removes_known_framework_constructor_argument_name() {
        let input = "using System.Threading;\n\nclass C\n{\n    CancellationToken token = new CancellationToken(canceled: true);\n}\n";

        assert_eq!(
            format_csharp(input, options()),
            "using System.Threading;\n\nclass C\n{\n    CancellationToken token = new CancellationToken(true);\n}\n"
        );
    }

    #[test]
    fn preserves_short_multi_property_initializer_without_semantic_layout() {
        let input = "class C\n{\n    void M()\n    {\n        Item item = new Item\n        {\n            Field = \"field\",\n            Role = \"role\",\n            Label = \"label\"\n        };\n    }\n}\n";

        assert_eq!(format_csharp(input, options()), input);
    }

    #[test]
    fn preserves_multi_property_initializer_with_long_declaration() {
        let input = "class C\n{\n    void M()\n    {\n        VeryLongCandidateName candidate = new VeryLongCandidateName\n        {\n            Field = \"field\",\n            Role = \"role\",\n            Label = \"label\"\n        };\n    }\n}\n";
        let mut options = options();
        options.max_line_length = 90;

        assert_eq!(format_csharp(input, options), input);
    }

    #[test]
    fn removes_global_qualifier_from_framework_alias() {
        let input = "using Word = global::Microsoft.Office.Interop.Word;\n\nclass C {}\n";
        assert_eq!(
            format_csharp(input, options()),
            "using Word = Microsoft.Office.Interop.Word;\n\nclass C {}\n"
        );
    }

    #[test]
    fn imports_known_qualified_framework_type() {
        let input = "using System.Text;\n\nclass C\n{\n    object Value = System.StringComparison.Ordinal;\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "using System;\nusing System.Text;\n\nclass C\n{\n    object Value = StringComparison.Ordinal;\n}\n"
        );
    }

    #[test]
    fn removes_null_forgiving_after_not_null_assertion() {
        let input = "class C\n{\n    void M()\n    {\n        PropertyInfo value = Find();\n        Assert.IsNotNull(value);\n        value!.SetValue(this, 1);\n    }\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        PropertyInfo value = Find();\n        Assert.IsNotNull(value);\n        value.SetValue(this, 1);\n    }\n}\n"
        );
    }

    #[test]
    fn expands_inline_array_argument_and_joins_items() {
        let input = "class C\n{\n    void M()\n    {\n        Replace(new[]\n        {\n            Make(0, 10),\n            Make(10, 20)\n        });\n    }\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        Replace(\n            new[]\n            {\n                Make(0, 10), Make(10, 20)\n            });\n    }\n}\n"
        );
    }

    #[test]
    fn removes_clearly_unused_system_using() {
        let input = "using System;\nusing System.Collections.Generic;\n\nclass C\n{\n    Queue<int> Values;\n}\n";
        let mut options = options();
        options.remove_clearly_unused_usings = true;
        assert_eq!(
            format_csharp(input, options),
            "using System.Collections.Generic;\n\nclass C\n{\n    Queue<int> Values;\n}\n"
        );
    }

    #[test]
    fn preserves_system_using_for_root_type() {
        let input = "using System;\n\nclass C : IDisposable\n{\n    public void Dispose() {}\n}\n";
        let mut options = options();
        options.remove_clearly_unused_usings = true;
        assert_eq!(format_csharp(input, options), input);
    }

    #[test]
    fn joins_expression_member_switch_and_outdents_arms() {
        let input = "class C\n{\n    private static string Label(string value) =>\n        value switch\n        {\n            \"a\" => \"A\",\n            _ => value\n        };\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    private static string Label(string value) => value switch\n    {\n        \"a\" => \"A\",\n        _ => value\n    };\n}\n"
        );
    }

    #[test]
    fn expands_long_moq_verify_lambda() {
        let input = "class C\n{\n    void M()\n    {\n        server.Verify(s => s.Call(\n            It.IsAny<string>(),\n            It.IsAny<int>()), Times.Once);\n    }\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        server.Verify(\n            s => s.Call(\n                It.IsAny<string>(),\n                It.IsAny<int>()),\n            Times.Once);\n    }\n}\n"
        );
    }

    #[test]
    fn expands_long_object_creation_arguments() {
        let input = "class C\n{\n    void M()\n    {\n        RenderParam renderParam = new RenderParam(OverlayStyle.BlockHighlight, HighlightColors.BackgroundActive);\n    }\n}\n";
        let mut options = options();
        options.max_line_length = 100;
        assert_eq!(
            format_csharp(input, options),
            "class C\n{\n    void M()\n    {\n        RenderParam renderParam = new RenderParam(\n            OverlayStyle.BlockHighlight,\n            HighlightColors.BackgroundActive);\n    }\n}\n"
        );
    }

    #[test]
    fn imports_known_project_type_qualification() {
        let input = "using System;\n\nclass C\n{\n    Elsa.Services.VirtualDocumentService.IVirtualDocumentService Value;\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "using System;\nusing Elsa.Services.VirtualDocumentService;\n\nclass C\n{\n    IVirtualDocumentService Value;\n}\n"
        );
    }

    #[test]
    fn collapses_typed_lambda_introduction() {
        let input = "class C\n{\n    void M()\n    {\n        mock.Returns(\n            (Range range) =>\n                new Result(range));\n    }\n}\n";
        assert_eq!(
            format_csharp(input, options()),
            "class C\n{\n    void M()\n    {\n        mock.Returns((Range range) =>\n            new Result(range));\n    }\n}\n"
        );
    }
}
