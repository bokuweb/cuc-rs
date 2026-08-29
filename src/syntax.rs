use std::collections::{HashMap, HashSet};

use tree_sitter::{Parser, Tree};

#[derive(Debug, Default)]
pub struct InterfaceLayout {
    interfaces: HashMap<String, InterfaceInfo>,
}

#[derive(Debug, Default)]
struct InterfaceInfo {
    bases: Vec<String>,
    members: Vec<String>,
}

impl InterfaceLayout {
    pub fn from_sources<S>(sources: impl IntoIterator<Item = S>) -> Self
    where
        S: AsRef<str>,
    {
        let mut layout = Self::default();
        for source in sources {
            let source = source.as_ref();
            let Some(tree) = parse_csharp(source) else {
                continue;
            };
            if tree.root_node().has_error() {
                continue;
            }
            collect_interface_declarations(tree.root_node(), source, &mut layout.interfaces);
        }
        layout
    }
}

pub fn parse_csharp(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

/// 型宣言の末尾に残った空行を削除する。
///
/// ReSharper cleanup は最後のメンバーと型を閉じる波括弧の間に空行を置かない。
/// 単純な波括弧の深さだけでは initializer や statement block と区別できないため、
/// tree-sitter が型本体と判定した箇所だけを対象にする。
pub fn remove_blank_lines_before_type_closing_braces(source: &str) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }

    let mut closing_rows = Vec::new();
    collect_type_closing_rows(tree.root_node(), &mut closing_rows);
    let mut lines = source.split('\n').collect::<Vec<_>>();
    closing_rows.sort_unstable();
    closing_rows.dedup();
    for closing_row in closing_rows.into_iter().rev() {
        if closing_row > 0
            && closing_row < lines.len()
            && lines[closing_row].trim_start().starts_with('}')
            && lines[closing_row - 1].trim().is_empty()
        {
            lines.remove(closing_row - 1);
        }
    }
    lines.join("\n")
}

fn collect_type_closing_rows(node: tree_sitter::Node<'_>, rows: &mut Vec<usize>) {
    if matches!(
        node.kind(),
        "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "record_declaration"
            | "enum_declaration"
    ) {
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            if let Some(closing_brace) =
                body.children(&mut cursor).find(|child| child.kind() == "}")
            {
                rows.push(closing_brace.start_position().row);
            };
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_type_closing_rows(child, rows);
    }
}

fn collect_interface_declarations(
    node: tree_sitter::Node<'_>,
    source: &str,
    interfaces: &mut HashMap<String, InterfaceInfo>,
) {
    if node.kind() == "interface_declaration" {
        if let (Some(name), Some(body)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("body"),
        ) {
            let mut cursor = body.walk();
            let members = body
                .named_children(&mut cursor)
                .filter_map(|member| member_key(member, source))
                .collect::<Vec<_>>();
            if let Some(name) = node_text(name, source) {
                let header = source
                    .get(node.start_byte()..body.start_byte())
                    .unwrap_or_default();
                let bases = header
                    .split_once(':')
                    .map(|(_, bases)| {
                        bases
                            .split(',')
                            .filter_map(|base| base.trim().rsplit('.').next())
                            .filter(|base| !base.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                interfaces.insert(name.to_string(), InterfaceInfo { bases, members });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_interface_declarations(child, source, interfaces);
    }
}

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> Option<&'a str> {
    source.get(node.byte_range())
}

fn member_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if !matches!(
        node.kind(),
        "method_declaration"
            | "property_declaration"
            | "event_declaration"
            | "event_field_declaration"
    ) {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .or_else(|| event_field_name(node));
    name.and_then(|name| node_text(name, source))
        .map(str::to_string)
}

fn member_key(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let name = member_name(node, source)?;
    match node.kind() {
        "method_declaration" => {
            let parameter_count = node
                .child_by_field_name("parameters")
                .map(|parameters| {
                    let mut cursor = parameters.walk();
                    parameters.named_children(&mut cursor).count()
                })
                .unwrap_or(0);
            Some(format!("method:{name}/{parameter_count}"))
        }
        "property_declaration" => Some(format!("property:{name}")),
        "event_declaration" | "event_field_declaration" => Some(format!("event:{name}")),
        _ => None,
    }
}

fn event_field_name(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.kind() != "event_field_declaration" {
        return None;
    }
    let mut cursor = node.walk();
    for declaration in node.named_children(&mut cursor) {
        if declaration.kind() != "variable_declaration" {
            continue;
        }
        let mut declaration_cursor = declaration.walk();
        for declarator in declaration.named_children(&mut declaration_cursor) {
            if declarator.kind() == "variable_declarator" {
                return declarator.child_by_field_name("name");
            }
        }
    }
    None
}

#[cfg(test)]
pub fn arrange_interface_implementations(source: &str, layout: &InterfaceLayout) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }
    let lines = source.split('\n').collect::<Vec<_>>();
    let mut regions = Vec::new();
    collect_interface_regions(tree.root_node(), source, &lines, layout, &mut regions);
    if regions.is_empty() {
        return source.to_string();
    }
    let mut output = lines
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| region.start_row);
    for region in regions.into_iter().rev() {
        let replacement = region
            .fields
            .iter()
            .map(|field| field.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        output.splice(
            region.start_row..=region.end_row,
            replacement.split('\n').map(str::to_string),
        );
    }
    output.join("\n")
}

/// ReSharperのdefault member layoutが行う並べ替えのうち、semantic modelなしでも
/// 一意に判定できる「後続interfaceが追加した同名overloadを、primary interface実装群の
/// 直前へ移す」ケースだけを適用する。一般のinterface member並べ替えは誤検出し得るため扱わない。
pub fn arrange_interface_overloads(source: &str, layout: &InterfaceLayout) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }
    let lines = source.split('\n').collect::<Vec<_>>();
    let mut regions = Vec::new();
    collect_interface_overload_regions(tree.root_node(), source, &lines, layout, &mut regions);
    if regions.is_empty() {
        return source.to_string();
    }
    let mut output = lines
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| region.start_row);
    for region in regions.into_iter().rev() {
        let replacement = region
            .fields
            .iter()
            .map(|field| field.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        output.splice(
            region.start_row..=region.end_row,
            replacement.split('\n').map(str::to_string),
        );
    }
    output.join("\n")
}

fn collect_interface_overload_regions(
    node: tree_sitter::Node<'_>,
    source: &str,
    lines: &[&str],
    layout: &InterfaceLayout,
    regions: &mut Vec<FieldRegion>,
) {
    if node.kind() == "class_declaration" {
        collect_interface_overload_region_from_class(node, source, lines, layout, regions);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_interface_overload_regions(child, source, lines, layout, regions);
    }
}

fn collect_interface_overload_region_from_class(
    class: tree_sitter::Node<'_>,
    source: &str,
    lines: &[&str],
    layout: &InterfaceLayout,
    regions: &mut Vec<FieldRegion>,
) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let header = lines[class.start_position().row..=body.start_position().row].join(" ");
    let Some((_, bases)) = header.split_once(':') else {
        return;
    };
    let interface_names = bases
        .trim_end_matches('{')
        .split(',')
        .filter_map(|base| {
            let name = base.trim().rsplit('.').next()?;
            layout
                .interfaces
                .contains_key(name)
                .then(|| name.to_string())
        })
        .collect::<Vec<_>>();
    if interface_names.len() < 2 {
        return;
    }

    let mut primary_members = Vec::new();
    collect_interface_members(
        &interface_names[0],
        layout,
        &mut primary_members,
        &mut Vec::new(),
        &mut Vec::new(),
    );
    let primary_members = primary_members.into_iter().collect::<HashSet<_>>();
    let mut seen_names = HashMap::<String, HashSet<String>>::new();
    let mut overloads = HashSet::new();
    for interface in &interface_names {
        let mut members = Vec::new();
        collect_interface_members(
            interface,
            layout,
            &mut members,
            &mut Vec::new(),
            &mut Vec::new(),
        );
        for key in &members {
            let Some(name) = method_name_from_key(key) else {
                continue;
            };
            if seen_names
                .get(name)
                .is_some_and(|prior| !prior.is_empty() && !prior.contains(key))
            {
                overloads.insert(key.clone());
            }
        }
        for key in members {
            if let Some(name) = method_name_from_key(&key) {
                seen_names.entry(name.to_string()).or_default().insert(key);
            }
        }
    }
    if overloads.is_empty() {
        return;
    }

    let mut cursor = body.walk();
    let members = body
        .named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "comment" | "attribute_list"))
        .collect::<Vec<_>>();
    let Some(anchor) = members.iter().position(|member| {
        member_key(*member, source).is_some_and(|key| primary_members.contains(&key))
    }) else {
        return;
    };
    let target_indices = members
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            (index > anchor
                && is_public_member(*member, source)
                && member_key(*member, source).is_some_and(|key| overloads.contains(&key)))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let Some(last_overload) = target_indices.last().copied() else {
        return;
    };
    let target_indices = target_indices.into_iter().collect::<HashSet<_>>();
    let misplaced_dispose = members
        .iter()
        .enumerate()
        .find(|(index, member)| {
            *index > last_overload
                && is_public_member(**member, source)
                && member_key(**member, source).is_some_and(|key| key == "method:Dispose/0")
                && members[last_overload + 1..*index].iter().any(|candidate| {
                    candidate.kind() == "method_declaration"
                        && !is_public_member(*candidate, source)
                })
        })
        .map(|(index, _)| index);
    let region_end = misplaced_dispose.unwrap_or(last_overload);
    let region_members = &members[anchor..=region_end];
    let mut blocks = Vec::with_capacity(region_members.len());
    let mut previous_end = region_members[0].start_position().row;
    for (offset, member) in region_members.iter().copied().enumerate() {
        let member_end = member.end_position().row;
        let block_start = if offset == 0 {
            member.start_position().row
        } else {
            previous_end
        };
        let absolute_index = anchor + offset;
        let category = if target_indices.contains(&absolute_index) {
            0
        } else if misplaced_dispose.is_some() && is_public_member(member, source) {
            1
        } else if misplaced_dispose.is_some() {
            2
        } else {
            1
        };
        blocks.push((category, lines[block_start..=member_end].join("\n")));
        previous_end = member_end + 1;
    }
    blocks.sort_by_key(|(category, _)| *category);
    let mut fields = blocks
        .into_iter()
        .map(|(_, text)| FieldBlock {
            category: 0,
            name: String::new(),
            text,
        })
        .collect::<Vec<_>>();
    for (index, field) in fields.iter_mut().enumerate() {
        if index == 0 {
            field.text = field.text.trim_start_matches('\n').to_string();
        } else if !field.text.starts_with('\n') {
            field.text.insert(0, '\n');
        }
    }
    regions.push(FieldRegion {
        start_row: region_members[0].start_position().row,
        end_row: region_members
            .last()
            .map(|member| member.end_position().row)
            .unwrap_or(0),
        fields,
    });
}

fn method_name_from_key(key: &str) -> Option<&str> {
    key.strip_prefix("method:")?
        .split_once('/')
        .map(|(name, _)| name)
}

fn is_public_member(member: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut cursor = member.walk();
    let is_public = member.children(&mut cursor).any(|child| {
        child.kind() == "modifier" && node_text(child, source).is_some_and(|text| text == "public")
    });
    is_public
}

pub fn arrange_misplaced_fields(source: &str) -> String {
    let mut output = source.to_string();
    for _ in 0..8 {
        let next = arrange_misplaced_fields_once(&output);
        if next == output {
            break;
        }
        output = next;
    }
    output
}

/// ReSharper の member layout のうち、semantic model なしでも一意に判断できる
/// 「field 群の直後に property だけが続き、その後に private field が 1 個だけ現れる」
/// ケースと、先頭 readonly field 群の単一 name outlier だけを整列する。
pub fn arrange_unambiguous_fields(source: &str) -> String {
    let output = arrange_single_late_private_field(source);
    arrange_single_readonly_prefix_outlier(&output)
}

fn arrange_single_late_private_field(source: &str) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }
    let lines = source.split('\n').collect::<Vec<_>>();
    let mut regions = Vec::new();
    collect_single_late_field_regions(tree.root_node(), source, &lines, &mut regions);
    if regions.is_empty() {
        return source.to_string();
    }
    apply_field_regions(&lines, regions)
}

fn collect_single_late_field_regions(
    node: tree_sitter::Node<'_>,
    source: &str,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    if node.kind() == "declaration_list" {
        let mut cursor = node.walk();
        let members = node
            .named_children(&mut cursor)
            .filter(|member| {
                !matches!(
                    member.kind(),
                    "comment" | "attribute_list" | "preproc_region" | "preproc_endregion"
                )
            })
            .collect::<Vec<_>>();
        let prefix_len = members
            .iter()
            .take_while(|member| {
                matches!(
                    member.kind(),
                    "field_declaration" | "constructor_declaration"
                )
            })
            .count();
        let late_fields = members
            .iter()
            .enumerate()
            .skip(prefix_len)
            .filter(|(_, member)| member.kind() == "field_declaration")
            .collect::<Vec<_>>();
        if prefix_len > 0 && late_fields.len() == 1 {
            let (late_index, late_field) = late_fields[0];
            let between_is_properties = members[prefix_len..late_index]
                .iter()
                .all(|member| member.kind() == "property_declaration");
            let declaration = node_text(*late_field, source).unwrap_or_default();
            if between_is_properties && declaration.trim_start().starts_with("private ") {
                let region_members = &members[..=late_index];
                let region_start = region_members[0].start_position().row;
                let region_end = late_field.end_position().row;
                let mut blocks = member_blocks(region_members, lines, region_start);
                let late_block = blocks.remove(late_index);
                let late_key = (late_block.category, late_block.name.as_str());
                let insertion = blocks[..prefix_len]
                    .iter()
                    .position(|block| (block.category, block.name.as_str()) > late_key)
                    .unwrap_or(prefix_len);
                blocks.insert(insertion, late_block);
                normalize_field_block_spacing(&mut blocks);
                regions.push(FieldRegion {
                    start_row: region_start,
                    end_row: region_end,
                    fields: blocks,
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_single_late_field_regions(child, source, lines, regions);
    }
}

fn arrange_single_readonly_prefix_outlier(source: &str) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }
    let lines = source.split('\n').collect::<Vec<_>>();
    let mut regions = Vec::new();
    collect_readonly_prefix_outlier_regions(tree.root_node(), &lines, &mut regions);
    if regions.is_empty() {
        return source.to_string();
    }
    apply_field_regions(&lines, regions)
}

fn collect_readonly_prefix_outlier_regions(
    node: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    if node.kind() == "declaration_list" {
        let mut cursor = node.walk();
        let fields = node
            .named_children(&mut cursor)
            .filter(|member| member.kind() != "comment")
            .take_while(|member| member.kind() == "field_declaration")
            .collect::<Vec<_>>();
        if fields.len() >= 3 {
            let region_start = fields[0].start_position().row;
            let mut blocks = member_blocks(&fields, lines, region_start);
            let readonly_indices = blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| block.category == 2)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if readonly_indices.len() >= 3 {
                let names = readonly_indices
                    .iter()
                    .map(|index| blocks[*index].name.as_str())
                    .collect::<Vec<_>>();
                let candidates = (0..names.len())
                    .filter(|removed| {
                        names
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| index != removed)
                            .map(|(_, name)| *name)
                            .collect::<Vec<_>>()
                            .windows(2)
                            .all(|pair| pair[0] <= pair[1])
                    })
                    .collect::<Vec<_>>();
                if candidates.len() == 1 {
                    let moved_index = readonly_indices[candidates[0]];
                    let moved = blocks.remove(moved_index);
                    let insertion = blocks
                        .iter()
                        .position(|block| block.category == 2 && block.name > moved.name)
                        .or_else(|| {
                            blocks
                                .iter()
                                .rposition(|block| block.category == 2)
                                .map(|index| index + 1)
                        })
                        .unwrap_or(blocks.len());
                    blocks.insert(insertion, moved);
                    normalize_field_block_spacing(&mut blocks);
                    regions.push(FieldRegion {
                        start_row: region_start,
                        end_row: fields
                            .last()
                            .map(|field| field.end_position().row)
                            .unwrap_or(region_start),
                        fields: blocks,
                    });
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_readonly_prefix_outlier_regions(child, lines, regions);
    }
}

fn member_blocks(
    members: &[tree_sitter::Node<'_>],
    lines: &[&str],
    region_start: usize,
) -> Vec<FieldBlock> {
    let mut blocks = Vec::with_capacity(members.len());
    let mut block_start = region_start;
    for (index, member) in members.iter().copied().enumerate() {
        let end = member.end_position().row;
        let (category, name) = if member.kind() == "field_declaration" {
            field_sort_key(&lines[member.start_position().row..=end].join("\n"))
                .unwrap_or((250, format!("{index:08}")))
        } else if member.kind() == "constructor_declaration" {
            (251, format!("{index:08}"))
        } else {
            (252, format!("{index:08}"))
        };
        blocks.push(FieldBlock {
            category,
            name,
            text: lines[block_start..=end].join("\n"),
        });
        block_start = end + 1;
    }
    blocks
}

fn normalize_field_block_spacing(blocks: &mut [FieldBlock]) {
    for (index, block) in blocks.iter_mut().enumerate() {
        if index == 0 {
            block.text = block.text.trim_start_matches('\n').to_string();
        } else if !block.text.starts_with('\n') {
            block.text.insert(0, '\n');
        }
    }
}

fn apply_field_regions(lines: &[&str], mut regions: Vec<FieldRegion>) -> String {
    if regions.is_empty() {
        return lines.join("\n");
    }
    let mut output = lines
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| region.start_row);
    for region in regions.into_iter().rev() {
        let replacement = region
            .fields
            .iter()
            .map(|field| field.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        output.splice(
            region.start_row..=region.end_row,
            replacement.split('\n').map(str::to_string),
        );
    }
    output.join("\n")
}

fn arrange_misplaced_fields_once(source: &str) -> String {
    let Some(tree) = parse_csharp(source) else {
        return source.to_string();
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }

    let lines = source.split('\n').collect::<Vec<_>>();
    let mut regions = Vec::new();
    collect_field_regions(tree.root_node(), &lines, &mut regions);
    if regions.is_empty() {
        return source.to_string();
    }

    let mut output = lines
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| region.start_row);
    for region in regions.into_iter().rev() {
        let replacement = region
            .fields
            .iter()
            .map(|field| field.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        output.splice(
            region.start_row..=region.end_row,
            replacement.split('\n').map(str::to_string),
        );
    }
    output.join("\n")
}

#[derive(Debug)]
struct FieldRegion {
    start_row: usize,
    end_row: usize,
    fields: Vec<FieldBlock>,
}

#[derive(Debug)]
struct FieldBlock {
    category: u8,
    name: String,
    text: String,
}

#[cfg(test)]
fn collect_interface_regions(
    node: tree_sitter::Node<'_>,
    source: &str,
    lines: &[&str],
    layout: &InterfaceLayout,
    regions: &mut Vec<FieldRegion>,
) {
    if node.kind() == "class_declaration" {
        collect_interface_region_from_class(node, source, lines, layout, regions);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_interface_regions(child, source, lines, layout, regions);
    }
}

#[cfg(test)]
fn collect_interface_region_from_class(
    class: tree_sitter::Node<'_>,
    source: &str,
    lines: &[&str],
    layout: &InterfaceLayout,
    regions: &mut Vec<FieldRegion>,
) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let header = lines[class.start_position().row..=body.start_position().row].join(" ");
    let class_name = class
        .child_by_field_name("name")
        .and_then(|name| node_text(name, source))
        .unwrap_or_default();
    let reorder_event_handlers = !header.contains("partial class")
        && !is_nested_class(class)
        && !class_name.starts_with("Mock")
        && !class_name.starts_with("Fake");
    let Some((_, bases)) = header.split_once(':') else {
        return;
    };
    let interface_names = bases
        .trim_end_matches('{')
        .split(',')
        .filter_map(|base| {
            let name = base.trim().rsplit('.').next()?;
            (layout.interfaces.contains_key(name)
                || matches!(name, "IDisposable" | "IAsyncDisposable" | "IWin32Window"))
            .then(|| name.to_string())
        })
        .collect::<Vec<_>>();
    if interface_names.is_empty() {
        return;
    }

    let mut direct_groups = Vec::<Vec<String>>::new();
    let mut disposable_members = Vec::<String>::new();
    for interface in &interface_names {
        let mut members = Vec::new();
        collect_interface_members(
            interface,
            layout,
            &mut members,
            &mut disposable_members,
            &mut Vec::new(),
        );
        members.sort();
        members.dedup();
        if !members.is_empty() {
            direct_groups.push(members);
        }
    }
    disposable_members.sort();
    disposable_members.dedup();
    let mut member_groups = HashMap::<String, usize>::new();
    for (group, members) in direct_groups.iter().enumerate() {
        for member in members {
            member_groups.entry(member.clone()).or_insert(group);
        }
    }
    let disposable_group = direct_groups.len();
    for member in &disposable_members {
        member_groups.insert(member.clone(), disposable_group);
    }

    let mut cursor = body.walk();
    let members = body
        .named_children(&mut cursor)
        .filter(|child| {
            !matches!(
                child.kind(),
                "comment" | "attribute_list" | "preproc_region" | "preproc_endregion"
            )
        })
        .collect::<Vec<_>>();
    let last_prefix = members
        .iter()
        .rposition(|member| {
            matches!(
                member.kind(),
                "field_declaration" | "constructor_declaration"
            )
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    if last_prefix >= members.len() {
        return;
    }
    let tail = &members[last_prefix..];
    let grouped_count = tail
        .iter()
        .filter_map(|member| member_key(*member, source))
        .filter(|name| member_groups.contains_key(name.as_str()))
        .count();
    if grouped_count == 0 {
        return;
    }

    let mut blocks = Vec::with_capacity(tail.len());
    let mut previous_end = tail[0].start_position().row;
    let mut region_start = tail[0].start_position().row;
    for (index, member) in tail.iter().copied().enumerate() {
        let member_start = member.start_position().row;
        let member_end = member.end_position().row;
        let block_start = if index == 0 {
            let mut start = member_start;
            while start > 0 {
                let prior = lines[start - 1].trim_start();
                if prior.starts_with("//") || prior.starts_with("/*") || prior.starts_with('*') {
                    start -= 1;
                } else {
                    break;
                }
            }
            region_start = start;
            start
        } else {
            previous_end
        };
        let group =
            member_key(member, source).and_then(|key| member_groups.get(key.as_str()).copied());
        blocks.push((
            group,
            member_layout_rank(member, source, reorder_event_handlers),
            index,
            lines[block_start..=member_end].join("\n"),
        ));
        previous_end = member_end + 1;
    }

    // ReSharper keeps ordinary members in their original slots. Only the
    // members known to implement interfaces are permuted between those slots.
    let slots = blocks
        .iter()
        .enumerate()
        .filter_map(|(slot, (group, _, _, _))| group.map(|_| slot))
        .collect::<Vec<_>>();
    let mut group_first = HashMap::<usize, usize>::new();
    for slot in &slots {
        if let Some(group) = blocks[*slot].0 {
            if group != disposable_group {
                group_first.entry(group).or_insert(blocks[*slot].2);
            }
        }
    }
    let mut ordered_groups = group_first.into_iter().collect::<Vec<_>>();
    ordered_groups.sort_by_key(|(_, first)| *first);
    let group_order = ordered_groups
        .into_iter()
        .enumerate()
        .map(|(order, (group, _))| (group, order))
        .collect::<HashMap<_, _>>();
    let explicit_disposable = interface_names
        .iter()
        .any(|name| matches!(name.as_str(), "IDisposable" | "IAsyncDisposable"));
    let mut implementations = slots
        .iter()
        .map(|slot| blocks[*slot].clone())
        .collect::<Vec<_>>();
    implementations.sort_by_key(|(group, rank, original, _)| {
        let group = group.unwrap_or(usize::MAX);
        if group == disposable_group {
            (usize::MAX - usize::from(!explicit_disposable), 0, *original)
        } else if *rank == 3 && explicit_disposable {
            (usize::MAX, 0, *original)
        } else {
            (
                group_order.get(&group).copied().unwrap_or(usize::MAX - 2),
                usize::from(*rank == 3),
                *original,
            )
        }
    });
    let original_order = slots.iter().map(|slot| blocks[*slot].2).collect::<Vec<_>>();
    if implementations
        .iter()
        .map(|(_, _, original, _)| *original)
        .eq(original_order.iter().copied())
    {
        return;
    }
    for (slot, implementation) in slots.into_iter().zip(implementations) {
        blocks[slot] = implementation;
    }
    let mut fields = blocks
        .into_iter()
        .map(|(_, _, _, text)| FieldBlock {
            category: 0,
            name: String::new(),
            text,
        })
        .collect::<Vec<_>>();
    for (index, field) in fields.iter_mut().enumerate() {
        if index == 0 {
            field.text = field.text.trim_start_matches('\n').to_string();
        } else if !field.text.starts_with('\n') {
            field.text.insert(0, '\n');
        }
    }
    regions.push(FieldRegion {
        start_row: region_start,
        end_row: tail
            .last()
            .map(|member| member.end_position().row)
            .unwrap_or(0),
        fields,
    });
}

#[cfg(test)]
fn member_layout_rank(
    member: tree_sitter::Node<'_>,
    source: &str,
    reorder_event_handlers: bool,
) -> usize {
    match member.kind() {
        "property_declaration" => {
            let text = node_text(member, source).unwrap_or_default();
            usize::from(
                text.contains("set;")
                    || text.contains("set =>")
                    || text.contains("set {")
                    || text.contains("init;"),
            )
        }
        "method_declaration" => 2,
        "event_declaration" | "event_field_declaration"
            if reorder_event_handlers
                && node_text(member, source)
                    .unwrap_or_default()
                    .contains("EventHandler") =>
        {
            3
        }
        _ => 4,
    }
}

#[cfg(test)]
fn is_nested_class(class: tree_sitter::Node<'_>) -> bool {
    let mut parent = class.parent();
    while let Some(node) = parent {
        if node.kind() == "class_declaration" {
            return true;
        }
        parent = node.parent();
    }
    false
}

fn collect_interface_members(
    name: &str,
    layout: &InterfaceLayout,
    members: &mut Vec<String>,
    disposable_members: &mut Vec<String>,
    visiting: &mut Vec<String>,
) {
    if visiting.iter().any(|item| item == name) {
        return;
    }
    match name {
        "IDisposable" => {
            disposable_members.push("method:Dispose/0".to_string());
            return;
        }
        "IAsyncDisposable" => {
            disposable_members.push("method:DisposeAsync/0".to_string());
            return;
        }
        "IWin32Window" => {
            members.push("property:Handle".to_string());
            return;
        }
        _ => {}
    }
    let Some(info) = layout.interfaces.get(name) else {
        return;
    };
    visiting.push(name.to_string());
    members.extend(info.members.iter().cloned());
    for base in &info.bases {
        collect_interface_members(base, layout, members, disposable_members, visiting);
    }
    visiting.pop();
}

fn collect_field_regions(
    node: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    if node.kind() == "declaration_list" {
        collect_prefix_member_region(node, lines, regions);
        collect_region_from_declaration_list(node, lines, regions);
        collect_nested_type_region(node, lines, regions);
        collect_method_before_properties_region(node, lines, regions);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_field_regions(child, lines, regions);
    }
}

fn collect_prefix_member_region(
    declaration_list: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    let Some(owner) = declaration_list.parent() else {
        return;
    };
    if owner.kind() != "class_declaration" {
        return;
    }
    let mut cursor = declaration_list.walk();
    let members = declaration_list
        .named_children(&mut cursor)
        .filter(|member| {
            !matches!(
                member.kind(),
                "comment" | "attribute_list" | "preproc_region" | "preproc_endregion"
            )
        })
        .collect::<Vec<_>>();
    let is_prefix = |member: &tree_sitter::Node<'_>| {
        matches!(
            member.kind(),
            "field_declaration" | "constructor_declaration"
        )
    };
    let Some(first_non_prefix) = members.iter().position(|member| !is_prefix(member)) else {
        return;
    };
    let Some(last_misplaced) = members
        .iter()
        .rposition(is_prefix)
        .filter(|index| *index > first_non_prefix)
    else {
        return;
    };
    if members[..=last_misplaced]
        .iter()
        .any(|member| !is_prefix(member) && member.kind() != "method_declaration")
    {
        return;
    }
    let region_members = &members[..=last_misplaced];
    let region_start = region_members[0].start_position().row;
    let region_end = region_members
        .last()
        .map(|member| member.end_position().row)
        .unwrap_or(region_start);
    let mut blocks = Vec::with_capacity(region_members.len());
    let mut block_start = region_start;
    for (index, member) in region_members.iter().enumerate() {
        let end = member.end_position().row;
        blocks.push(FieldBlock {
            category: match member.kind() {
                "field_declaration" => 0,
                "constructor_declaration" => 1,
                _ => 2,
            },
            name: format!("{index:08}"),
            text: lines[block_start..=end].join("\n"),
        });
        block_start = end + 1;
    }
    blocks.sort_by_key(|block| (block.category, block.name.clone()));
    for (index, block) in blocks.iter_mut().enumerate() {
        if index == 0 {
            block.text = block.text.trim_start_matches('\n').to_string();
        } else if !block.text.starts_with('\n') {
            block.text.insert(0, '\n');
        }
    }
    regions.push(FieldRegion {
        start_row: region_start,
        end_row: region_end,
        fields: blocks,
    });
}

fn collect_method_before_properties_region(
    declaration_list: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    let Some(owner) = declaration_list.parent() else {
        return;
    };
    if owner.kind() != "class_declaration"
        || !has_no_base_type_or_only_idisposable(owner, declaration_list, lines)
    {
        return;
    }

    let mut cursor = declaration_list.walk();
    let members = declaration_list
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect::<Vec<_>>();
    let misplaced = members
        .iter()
        .enumerate()
        .filter(|(index, member)| {
            member.kind() == "method_declaration"
                && members[*index + 1..]
                    .iter()
                    .any(|later| later.kind() == "property_declaration")
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if misplaced.len() != 1 {
        return;
    }
    let method_index = misplaced[0];
    let last_property = members
        .iter()
        .rposition(|member| member.kind() == "property_declaration")
        .unwrap_or(method_index);
    if last_property <= method_index
        || members[method_index + 1..=last_property]
            .iter()
            .any(|member| member.kind() != "property_declaration")
        || !members[last_property + 1..]
            .iter()
            .any(|member| member.kind() == "method_declaration")
    {
        return;
    }

    let method = members[method_index];
    let mut method_start = method.start_position().row;
    while method_start > 0 {
        let prior = lines[method_start - 1].trim_start();
        if prior.starts_with("//") || prior.starts_with("/*") || prior.starts_with('*') {
            method_start -= 1;
        } else {
            break;
        }
    }
    let method_end = method.end_position().row;
    let property_end = members[last_property].end_position().row;
    let properties = lines[method_end + 1..=property_end]
        .join("\n")
        .trim_start_matches('\n')
        .to_string();
    let method_text = lines[method_start..=method_end].join("\n");
    regions.push(FieldRegion {
        start_row: method_start,
        end_row: property_end,
        fields: vec![
            FieldBlock {
                category: 0,
                name: String::new(),
                text: properties,
            },
            FieldBlock {
                category: 0,
                name: String::new(),
                text: format!("\n{method_text}"),
            },
        ],
    });
}

fn has_no_base_type_or_only_idisposable(
    owner: tree_sitter::Node<'_>,
    declaration_list: tree_sitter::Node<'_>,
    lines: &[&str],
) -> bool {
    let header =
        lines[owner.start_position().row..=declaration_list.start_position().row].join(" ");
    let Some((_, bases)) = header.split_once(':') else {
        return true;
    };
    let bases = bases.trim_end_matches('{').trim();
    !bases.contains(',') && bases.rsplit('.').next() == Some("IDisposable")
}

fn collect_nested_type_region(
    declaration_list: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    let Some(owner) = declaration_list.parent() else {
        return;
    };
    if !matches!(
        owner.kind(),
        "class_declaration" | "struct_declaration" | "record_declaration"
    ) {
        return;
    }

    let mut cursor = declaration_list.walk();
    let members = declaration_list
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect::<Vec<_>>();
    let misplaced = members
        .iter()
        .enumerate()
        .filter(|(index, member)| {
            *index > 0
                && is_movable_nested_type(**member, lines)
                && members[*index + 1..]
                    .iter()
                    .any(|later| !is_type_declaration(later.kind()))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if misplaced.len() != 1 {
        return;
    }
    let nested_index = misplaced[0];
    let nested = members[nested_index];
    let last_non_type = members
        .iter()
        .rposition(|member| !is_type_declaration(member.kind()))
        .unwrap_or(nested_index);
    if last_non_type <= nested_index {
        return;
    }
    let mut nested_start = nested.start_position().row;
    while nested_start > 0 {
        let prior = lines[nested_start - 1].trim_start();
        if prior.starts_with("//") || prior.starts_with("/*") || prior.starts_with('*') {
            nested_start -= 1;
        } else {
            break;
        }
    }
    if nested_start >= 2
        && lines[nested_start - 1].trim().is_empty()
        && is_section_rule_comment(lines[nested_start - 2])
    {
        let mut section_start = nested_start - 2;
        while section_start > 0 && lines[section_start - 1].trim_start().starts_with("//") {
            section_start -= 1;
        }
        if is_section_rule_comment(lines[section_start]) {
            nested_start = section_start;
        }
    }
    let nested_end = nested.end_position().row;
    let region_end = members[last_non_type].end_position().row;
    let following_members = lines[nested_end + 1..=region_end]
        .join("\n")
        .trim_start_matches('\n')
        .to_string();
    let nested_text = lines[nested_start..=nested_end].join("\n");
    regions.push(FieldRegion {
        start_row: nested_start,
        end_row: region_end,
        fields: vec![
            FieldBlock {
                category: 0,
                name: String::new(),
                text: following_members,
            },
            FieldBlock {
                category: 0,
                name: String::new(),
                text: format!("\n{nested_text}"),
            },
        ],
    });
}

fn is_section_rule_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("// -----") && trimmed[3..].chars().all(|ch| ch == '-')
}

fn is_movable_nested_type(node: tree_sitter::Node<'_>, lines: &[&str]) -> bool {
    if node.kind() != "class_declaration" {
        return false;
    }
    let declaration = lines
        .get(node.start_position().row)
        .map(|line| line.trim())
        .unwrap_or_default();
    declaration.contains(" sealed class ") || declaration.starts_with("sealed class ")
}

fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
    )
}

fn collect_region_from_declaration_list(
    declaration_list: tree_sitter::Node<'_>,
    lines: &[&str],
    regions: &mut Vec<FieldRegion>,
) {
    let mut cursor = declaration_list.walk();
    let children = declaration_list
        .named_children(&mut cursor)
        .collect::<Vec<_>>();
    let field_nodes = children
        .iter()
        .copied()
        .filter(|child| child.kind() == "field_declaration")
        .collect::<Vec<_>>();
    if field_nodes.len() < 2 {
        return;
    }

    let first_field_index = children
        .iter()
        .position(|child| child.kind() == "field_declaration")
        .unwrap_or(0);
    let last_field_index = children
        .iter()
        .rposition(|child| child.kind() == "field_declaration")
        .unwrap_or(first_field_index);
    if children[first_field_index..=last_field_index]
        .iter()
        .any(|child| !matches!(child.kind(), "field_declaration" | "comment"))
    {
        return;
    }

    let mut fields = Vec::with_capacity(field_nodes.len());
    let mut previous_end = field_nodes[0].start_position().row;
    for (field_index, field) in field_nodes.iter().copied().enumerate() {
        let declaration_start = field.start_position().row;
        let declaration_end = field.end_position().row;
        let block_start = if field_index == 0 {
            declaration_start
        } else {
            previous_end
        };
        let declaration = lines[declaration_start..=declaration_end].join("\n");
        let Some((category, name)) = field_sort_key(&declaration) else {
            return;
        };
        fields.push(FieldBlock {
            category,
            name,
            text: lines[block_start..=declaration_end].join("\n"),
        });
        previous_end = declaration_end + 1;
    }

    let mut misplaced_indices = fields
        .iter()
        .enumerate()
        .filter(|(index, field)| {
            field.category == 3 && fields[*index + 1..].iter().any(|later| later.category == 2)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut changed = false;
    if !misplaced_indices.is_empty() {
        let mut misplaced = Vec::with_capacity(misplaced_indices.len());
        for index in misplaced_indices.drain(..).rev() {
            misplaced.push(fields.remove(index));
        }
        misplaced.sort_by(|left, right| left.name.cmp(&right.name));
        for field in misplaced {
            let insertion = fields
                .iter()
                .position(|existing| existing.category == 3 && existing.name > field.name)
                .or_else(|| {
                    fields
                        .iter()
                        .rposition(|existing| existing.category == 3)
                        .map(|index| index + 1)
                })
                .unwrap_or(fields.len());
            fields.insert(insertion, field);
        }
        changed = true;
    }
    if move_single_name_outlier(&mut fields, 2, true) {
        changed = true;
    }
    let owner_is_class = declaration_list
        .parent()
        .is_some_and(|owner| owner.kind() == "class_declaration");
    if owner_is_class && move_single_name_outlier(&mut fields, 3, false) {
        changed = true;
    }
    if !changed {
        return;
    }
    for (index, field) in fields.iter_mut().enumerate() {
        if index == 0 {
            field.text = field.text.trim_start_matches('\n').to_string();
        } else if !field.text.starts_with('\n') {
            field.text.insert(0, '\n');
        }
    }
    regions.push(FieldRegion {
        start_row: field_nodes_start_row(&children),
        end_row: children[last_field_index].end_position().row,
        fields,
    });
}

fn move_single_name_outlier(
    fields: &mut Vec<FieldBlock>,
    category: u8,
    allow_two_field_swap: bool,
) -> bool {
    let indices = fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.category == category)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let minimum = if allow_two_field_swap { 2 } else { 3 };
    if indices.len() < minimum
        || indices
            .iter()
            .any(|index| fields[*index].text.trim_start().starts_with('['))
    {
        return false;
    }
    let names = indices
        .iter()
        .map(|index| fields[*index].name.as_str())
        .collect::<Vec<_>>();
    if names.windows(2).all(|pair| pair[0] <= pair[1]) {
        return false;
    }
    if indices.len() == 2 {
        fields.swap(indices[0], indices[1]);
        return true;
    }
    let candidates = (0..names.len())
        .filter(|removed| {
            names
                .iter()
                .enumerate()
                .filter(|(index, _)| index != removed)
                .map(|(_, name)| *name)
                .collect::<Vec<_>>()
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return false;
    }
    let field = fields.remove(indices[candidates[0]]);
    let insertion = fields
        .iter()
        .position(|existing| existing.category == category && existing.name > field.name)
        .or_else(|| {
            fields
                .iter()
                .rposition(|existing| existing.category == category)
                .map(|index| index + 1)
        })
        .unwrap_or(fields.len());
    fields.insert(insertion, field);
    true
}

fn field_nodes_start_row(children: &[tree_sitter::Node<'_>]) -> usize {
    children
        .iter()
        .find(|child| child.kind() == "field_declaration")
        .map(|field| field.start_position().row)
        .unwrap_or(0)
}

fn field_sort_key(declaration: &str) -> Option<(u8, String)> {
    let declaration_lines = declaration.lines().collect::<Vec<_>>();
    let declaration_start = declaration_lines.iter().position(|line| {
        let trimmed = line.trim_start();
        ["public ", "private ", "protected ", "internal "]
            .iter()
            .any(|modifier| trimmed.starts_with(modifier))
    })?;
    let declaration_body = declaration_lines[declaration_start..].join(" ");
    let declaration_line = declaration_body.trim();
    let padded = format!(" {declaration_line} ");
    let is_const = padded.contains(" const ");
    let is_static = padded.contains(" static ");
    let is_readonly = padded.contains(" readonly ");
    let category = match (is_const, is_static, is_readonly) {
        (true, _, _) => 0,
        (false, true, _) => 1,
        (false, false, true) => 2,
        (false, false, false) => 3,
    };
    let before_initializer = declaration_line.split(['=', ';']).next()?.trim_end();
    if contains_top_level_comma(before_initializer) {
        return None;
    }
    let name = before_initializer
        .split_whitespace()
        .last()?
        .trim_end_matches('?');
    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        .then(|| (category, name.to_string()))
}

fn contains_top_level_comma(declaration: &str) -> bool {
    let mut parentheses = 0i32;
    let mut brackets = 0i32;
    let mut angles = 0i32;
    for ch in declaration.chars() {
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '<' => angles += 1,
            '>' => angles -= 1,
            ',' if parentheses == 0 && brackets == 0 && angles == 0 => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::parse_csharp;

    #[test]
    fn parses_csharp_source() {
        let tree = parse_csharp("namespace N { class C { private int _value; } }").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn moves_mutable_field_after_readonly_fields() {
        let input = "namespace N\n{\n    class C\n    {\n        private readonly object _a;\n\n        private int _z;\n\n        private readonly object _b;\n\n        private int _m;\n    }\n}\n";
        assert_eq!(
            super::arrange_misplaced_fields(input),
            "namespace N\n{\n    class C\n    {\n        private readonly object _a;\n\n        private readonly object _b;\n\n        private int _m;\n\n        private int _z;\n    }\n}\n"
        );
    }

    #[test]
    fn moves_single_readonly_name_outlier() {
        let input = "class C\n{\n    private readonly object _list;\n\n    private readonly object _refresh;\n\n    private readonly object _host;\n\n    private readonly object _service;\n}\n";
        assert_eq!(
            super::arrange_misplaced_fields(input),
            "class C\n{\n    private readonly object _host;\n\n    private readonly object _list;\n\n    private readonly object _refresh;\n\n    private readonly object _service;\n}\n"
        );
    }

    #[test]
    fn sorts_two_readonly_fields_by_name() {
        let input = "class C\n{\n    private readonly object _paragraphs;\n\n    private readonly object _detailCache;\n}\n";
        assert_eq!(
            super::arrange_misplaced_fields(input),
            "class C\n{\n    private readonly object _detailCache;\n\n    private readonly object _paragraphs;\n}\n"
        );
    }

    #[test]
    fn moves_late_private_field_to_prefix_and_sorts_readonly_outlier() {
        let input = "class C\n{\n    private readonly int _max;\n\n    private readonly object _paragraphLru;\n\n    private readonly object _paragraphs;\n\n    public int Count => 0;\n\n    // Identity index.\n    private readonly object _paragraphKeysByIdentity;\n\n    public void Clear() {}\n}\n";
        let expected = "class C\n{\n    private readonly int _max;\n\n    // Identity index.\n    private readonly object _paragraphKeysByIdentity;\n\n    private readonly object _paragraphLru;\n\n    private readonly object _paragraphs;\n\n    public int Count => 0;\n\n    public void Clear() {}\n}\n";

        assert_eq!(super::arrange_unambiguous_fields(input), expected);
    }

    #[test]
    fn sorts_mutable_fields_by_name_with_comments() {
        let input = "class C\n{\n    private int _alpha;\n\n    private (int X, int Y) _delta;\n\n    private int _epsilon;\n\n    // beta details\n    private int _beta;\n\n    private int _gamma;\n}\n";
        let expected = "class C\n{\n    private int _alpha;\n\n    // beta details\n    private int _beta;\n\n    private (int X, int Y) _delta;\n\n    private int _epsilon;\n\n    private int _gamma;\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), expected);
    }

    #[test]
    fn preserves_multi_variable_field_declarations() {
        let input = "class C\n{\n    private int _alpha;\n    private int _delta;\n    private int _epsilon;\n    private int _beta, _beta2;\n    private int _gamma;\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), input);
    }

    #[test]
    fn moves_nested_type_after_methods() {
        let input = "class C\n{\n    void First() {}\n\n    // Helper details.\n    private sealed class Helper {}\n\n    void Last() {}\n}\n";
        assert_eq!(
            super::arrange_misplaced_fields(input),
            "class C\n{\n    void First() {}\n\n    void Last() {}\n\n    // Helper details.\n    private sealed class Helper {}\n}\n"
        );
    }

    #[test]
    fn moves_section_banner_with_nested_type() {
        let input = "class C\n{\n    void First() {}\n\n    // ------------------\n    // helper types\n    // ------------------\n\n    /// Helper details.\n    private sealed class Helper {}\n\n    void Last() {}\n}\n";
        let expected = "class C\n{\n    void First() {}\n\n    void Last() {}\n\n    // ------------------\n    // helper types\n    // ------------------\n\n    /// Helper details.\n    private sealed class Helper {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), expected);
    }

    #[test]
    fn moves_method_after_following_properties_for_idisposable_class() {
        let input = "class C : IDisposable\n{\n    /// Finds a value.\n    public object Find() { return null; }\n\n    public object First { get; }\n\n    public object Second { get; }\n\n    public void Dispose() {}\n}\n";
        let expected = "class C : IDisposable\n{\n    public object First { get; }\n\n    public object Second { get; }\n\n    /// Finds a value.\n    public object Find() { return null; }\n\n    public void Dispose() {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), expected);
    }

    #[test]
    fn preserves_member_order_when_class_implements_domain_interface() {
        let input = "class C : IService\n{\n    public object Find() { return null; }\n    public object Value { get; }\n    public void Dispose() {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), input);
    }

    #[test]
    fn reorders_only_interface_implementation_slots() {
        let interfaces = "interface IPlugin : IDisposable\n{\n    bool IsSlow { get; }\n    void Update();\n}\ninterface IController\n{\n    void Set();\n    void Clear();\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class C : IPlugin, IController\n{\n    private int _value;\n\n    public C() {}\n\n    public event EventHandler Changed;\n\n    public bool IsSlow { get; }\n\n    public void Update() {}\n\n    public void Dispose() {}\n\n    public void Set() {}\n\n    public void Clear() {}\n}\n";
        let expected = "class C : IPlugin, IController\n{\n    private int _value;\n\n    public C() {}\n\n    public event EventHandler Changed;\n\n    public bool IsSlow { get; }\n\n    public void Update() {}\n\n    public void Set() {}\n\n    public void Clear() {}\n\n    public void Dispose() {}\n}\n";
        assert_eq!(
            super::arrange_interface_implementations(input, &layout),
            expected
        );
    }

    #[test]
    fn preserves_attributes_while_reordering_interface_events() {
        let interfaces = "interface IIdle : IDisposable\n{\n    event EventHandler Idle;\n    void Start();\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class C : IDisposable, IIdle\n{\n    public void Dispose() {}\n\n    public event EventHandler Idle;\n\n    public void Start() {}\n\n    private void Stop() {}\n\n    #region Native\n\n    [StructLayout(LayoutKind.Sequential)]\n    private struct S {}\n\n    #endregion\n}\n";
        let expected = "class C : IDisposable, IIdle\n{\n    public void Start() {}\n\n    public void Dispose() {}\n\n    public event EventHandler Idle;\n\n    private void Stop() {}\n\n    #region Native\n\n    [StructLayout(LayoutKind.Sequential)]\n    private struct S {}\n\n    #endregion\n}\n";
        assert_eq!(
            super::arrange_interface_implementations(input, &layout),
            expected
        );
    }

    #[test]
    fn moves_public_secondary_interface_overload_before_primary_members() {
        let interfaces = "interface IPlugin\n{\n    bool IsSlow { get; }\n    void Draw(int start);\n}\ninterface IDocumentEndProvider\n{\n    void Draw(int start, int documentEnd);\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public bool IsSlow => false;\n\n    public void Draw(int start) {}\n\n    public void Draw(int start, int documentEnd) {}\n}\n";
        let expected = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public void Draw(int start, int documentEnd) {}\n\n    public bool IsSlow => false;\n\n    public void Draw(int start) {}\n}\n";
        let output = super::arrange_interface_overloads(input, &layout);
        assert_eq!(output, expected);
        assert_eq!(super::arrange_interface_overloads(&output, &layout), output);
    }

    #[test]
    fn builds_interface_layout_from_owned_sources() {
        let sources = vec![
            "interface IPlugin\n{\n    void Draw(int start);\n}\n".to_string(),
            "interface IDocumentEndProvider\n{\n    void Draw(int start, int documentEnd);\n}\n"
                .to_string(),
        ];
        let layout = super::InterfaceLayout::from_sources(sources);
        let input = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public void Draw(int start) {}\n\n    public void Draw(int start, int documentEnd) {}\n}\n";
        let expected = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public void Draw(int start, int documentEnd) {}\n\n    public void Draw(int start) {}\n}\n";

        assert_eq!(super::arrange_interface_overloads(input, &layout), expected);
    }

    #[test]
    fn preserves_private_helper_matching_secondary_interface_overload() {
        let interfaces = "interface IService\n{\n    object Parse(string text, CancellationToken token);\n}\ninterface IOwnerService\n{\n    object Parse(string text, string owner, CancellationToken token);\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class Service : IService, IOwnerService\n{\n    public object Parse(string text, CancellationToken token) => null;\n\n    private object Parse(string text, string owner, CancellationToken token) => null;\n}\n";
        assert_eq!(super::arrange_interface_overloads(input, &layout), input);
    }

    #[test]
    fn keeps_dispose_with_public_interface_members_when_moving_overload() {
        let interfaces = "interface IPlugin : IDisposable\n{\n    bool IsSlow { get; }\n    void Draw(int start);\n}\ninterface IDocumentEndProvider\n{\n    void Draw(int start, int documentEnd);\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public bool IsSlow => false;\n\n    public void Draw(int start) {}\n\n    public void Draw(int start, int documentEnd) {}\n\n    private void DrawCore() {}\n\n    public void Dispose() {}\n}\n";
        let expected = "class Plugin : IPlugin, IDocumentEndProvider\n{\n    public void Draw(int start, int documentEnd) {}\n\n    public bool IsSlow => false;\n\n    public void Draw(int start) {}\n\n    public void Dispose() {}\n\n    private void DrawCore() {}\n}\n";
        let output = super::arrange_interface_overloads(input, &layout);
        assert_eq!(output, expected);
        assert_eq!(super::arrange_interface_overloads(&output, &layout), output);
    }

    #[test]
    fn moves_fields_and_constructor_before_test_lifecycle_methods() {
        let input = "class C\n{\n    [SetUp]\n    public void SetUp() {}\n\n    [TearDown]\n    public void TearDown() {}\n\n    private object _app;\n\n    private object _doc;\n\n    public C() {}\n\n    private void Open() {}\n}\n";
        let expected = "class C\n{\n    private object _app;\n\n    private object _doc;\n\n    public C() {}\n\n    [SetUp]\n    public void SetUp() {}\n\n    [TearDown]\n    public void TearDown() {}\n\n    private void Open() {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), expected);
    }

    #[test]
    fn preserves_constructor_after_nested_type() {
        let input = "class C\n{\n    public enum State { Ready }\n\n    public C() {}\n\n    public void Start() {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), input);
    }

    #[test]
    fn preserves_resolved_interface_property_and_overload_order() {
        let interfaces = "interface IService\n{\n    bool IsRunning { get; }\n    Uri Endpoint { get; }\n    int? ProcessId { get; }\n}\ninterface IToast\n{\n    bool TryShow(int a, int b, int c);\n    bool TryShow(int a, int b, int c, Action action);\n    bool TryShow(int a, int b, int c, string text, Action action);\n}\n";
        let layout = super::InterfaceLayout::from_sources([interfaces]);
        let input = "class Service : IService\n{\n    public bool IsRunning { get; set; } = true;\n    public Uri Endpoint => new Uri(\"http://127.0.0.1\");\n    public int? ProcessId => null;\n}\nclass Toast : IToast\n{\n    public bool TryShow(int a, int b, int c) => true;\n    public bool TryShow(int a, int b, int c, Action? action) => true;\n    public bool TryShow(int a, int b, int c, string text, Action action) => true;\n}\n";
        assert_eq!(
            super::arrange_interface_implementations(input, &layout),
            input
        );
    }

    #[test]
    fn preserves_nested_enum_and_non_sealed_helper() {
        let input = "class C\n{\n    public enum State { Ready }\n\n    void First() {}\n\n    private class Helper {}\n\n    void Last() {}\n}\n";
        assert_eq!(super::arrange_misplaced_fields(input), input);
    }

    #[test]
    fn stabilizes_overlapping_nested_type_and_field_moves() {
        let input = "class C\n{\n    void First() {}\n\n    private sealed class Helper\n    {\n        private readonly object _paragraphs;\n\n        private readonly object _detailCache;\n    }\n\n    void Last() {}\n}\n";
        let expected = "class C\n{\n    void First() {}\n\n    void Last() {}\n\n    private sealed class Helper\n    {\n        private readonly object _detailCache;\n\n        private readonly object _paragraphs;\n    }\n}\n";
        let output = super::arrange_misplaced_fields(input);
        assert_eq!(output, expected);
        assert_eq!(super::arrange_misplaced_fields(&output), output);
    }
}
