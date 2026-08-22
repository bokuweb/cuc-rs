pub fn format_xaml(input: &str) -> String {
    let had_final_newline = input.ends_with('\n');
    let mut continuation_column = None;
    let mut in_opaque_section = false;
    let mut output = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim_start();
        let starts_opaque_section = trimmed.contains("<!--") || trimmed.contains("<![CDATA[");
        let formatted = if in_opaque_section || starts_opaque_section {
            if (trimmed.contains("-->") || trimmed.contains("]]>")) && !starts_opaque_section {
                in_opaque_section = false;
            } else if starts_opaque_section && !trimmed.contains("-->") && !trimmed.contains("]]>")
            {
                in_opaque_section = true;
            }
            line.to_string()
        } else if let Some(column) = continuation_column {
            let fragment = normalize_continuation_fragment(trimmed);
            let closes_tag = tag_fragment_closes(&fragment);
            let line = format!("{}{fragment}", " ".repeat(column));
            if closes_tag {
                continuation_column = None;
            }
            line
        } else {
            let line = normalize_complete_tag_segments(line);
            continuation_column = opening_tag_continuation_column(&line);
            line
        };
        output.push(formatted);
    }

    let mut result = output.join("\n");
    if had_final_newline {
        result.push('\n');
    }
    result
}

fn normalize_continuation_fragment(fragment: &str) -> String {
    let mut quote = None;
    for (index, ch) in fragment.char_indices() {
        if let Some(expected) = quote {
            if ch == expected {
                quote = None;
            }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if ch == '>' {
            let end = index + ch.len_utf8();
            return format!(
                "{}{}",
                normalize_tag_fragment(&fragment[..end]),
                &fragment[end..]
            );
        }
    }
    normalize_tag_fragment(fragment)
}

fn normalize_complete_tag_segments(line: &str) -> String {
    let chars = line.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != '<' || matches!(chars.get(index + 1), Some('!' | '?')) {
            output.push(chars[index]);
            index += 1;
            continue;
        }
        let start = index;
        let mut quote = None;
        index += 1;
        while index < chars.len() {
            let ch = chars[index];
            if let Some(expected) = quote {
                if ch == expected {
                    quote = None;
                }
            } else if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if ch == '>' {
                index += 1;
                let segment = chars[start..index].iter().collect::<String>();
                output.push_str(&normalize_tag_fragment(&segment));
                break;
            }
            index += 1;
        }
        if index == chars.len() && chars.last() != Some(&'>') {
            let segment = chars[start..].iter().collect::<String>();
            output.push_str(&normalize_tag_fragment(&segment));
        }
    }
    output
}

fn normalize_tag_fragment(fragment: &str) -> String {
    let chars = fragment.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(fragment.len());
    let mut index = 0usize;
    let mut quote = None;
    let mut pending_space = false;

    while index < chars.len() {
        let ch = chars[index];
        if let Some(expected) = quote {
            output.push(ch);
            if ch == expected {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(ch, '\'' | '"') {
            if pending_space && needs_separator(&output) {
                output.push(' ');
            }
            pending_space = false;
            quote = Some(ch);
            output.push(ch);
            index += 1;
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            index += 1;
            continue;
        }
        if ch == '=' {
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('=');
            pending_space = false;
            index += 1;
            while chars.get(index).is_some_and(|next| next.is_whitespace()) {
                index += 1;
            }
            continue;
        }
        if ch == '>' {
            while output.ends_with(' ') {
                output.pop();
            }
            if output.ends_with('/') && !output.ends_with(" />") {
                output.pop();
                while output.ends_with(' ') {
                    output.pop();
                }
                output.push_str(" /");
            }
            output.push('>');
            pending_space = false;
            index += 1;
            continue;
        }
        if pending_space && needs_separator(&output) && ch != '/' {
            output.push(' ');
        }
        pending_space = false;
        output.push(ch);
        index += 1;
    }
    output.trim_end().to_string()
}

fn needs_separator(output: &str) -> bool {
    !output.is_empty() && !output.ends_with(['<', '/', '='])
}

fn tag_fragment_closes(fragment: &str) -> bool {
    let mut quote = None;
    for ch in fragment.chars() {
        if let Some(expected) = quote {
            if ch == expected {
                quote = None;
            }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if ch == '>' {
            return true;
        }
    }
    false
}

fn opening_tag_continuation_column(line: &str) -> Option<usize> {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    if !trimmed.starts_with('<')
        || trimmed.starts_with("</")
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("<![")
        || trimmed.starts_with("<?")
        || tag_fragment_closes(trimmed)
    {
        return None;
    }
    let name_end = trimmed
        .char_indices()
        .skip(1)
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| index)?;
    Some(leading + name_end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_resharper_attribute_spacing_and_alignment() {
        let input = "<Window   x:Class = \"Example.MainWindow\"\n  xmlns = \"urn:example\"\n  Width = \"320\">\n    <ColumnDefinition   Width = \"320\"/>\n</Window>";
        let expected = "<Window x:Class=\"Example.MainWindow\"\n        xmlns=\"urn:example\"\n        Width=\"320\">\n    <ColumnDefinition Width=\"320\" />\n</Window>";
        assert_eq!(format_xaml(input), expected);
    }

    #[test]
    fn preserves_attribute_values_and_text_content() {
        let input = "<TextBlock Text=\"a  b = c\">keep   text</TextBlock>\n";
        assert_eq!(format_xaml(input), input);
    }

    #[test]
    fn preserves_comments_and_final_newline_state() {
        let input = "<!-- <Grid   Width = \"1\"/> -->\n<Grid />\n";
        assert_eq!(format_xaml(input), input);
    }

    #[test]
    fn preserves_text_after_multiline_opening_tag() {
        let input = "<TextBlock\n    Text=\"value\">keep   text</TextBlock>\n";
        assert_eq!(format_xaml(input), input);
    }
}
