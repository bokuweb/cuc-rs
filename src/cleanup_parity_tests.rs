use crate::csharp::{format_csharp, CSharpOptions};
use crate::editorconfig::Properties;

fn options() -> CSharpOptions {
    CSharpOptions::from_properties(&Properties::from_pairs(&[
        ("max_line_length", "80"),
        ("resharper_csharp_wrap_parameters_style", "chop_if_long"),
        (
            "resharper_keep_existing_declaration_parens_arrangement",
            "false",
        ),
        ("resharper_wrap_after_declaration_lpar", "true"),
        ("resharper_keep_existing_initializer_arrangement", "false"),
    ]))
}

fn assert_cleanup(input: &str, expected: &str) {
    let output = format_csharp(input, options());
    assert_eq!(output, expected);
    assert_eq!(format_csharp(&output, options()), expected);
}

#[test]
fn chops_long_declaration_parameters_without_splitting_generic_commas() {
    let input = "class C\n{\n    void AMethodWithALongDescriptiveName(\n        Dictionary<string, int> values, string name, bool enabled)\n    {\n    }\n}\n";
    let expected = "class C\n{\n    void AMethodWithALongDescriptiveName(\n        Dictionary<string, int> values,\n        string name,\n        bool enabled)\n    {\n    }\n}\n";
    assert_cleanup(input, expected);
}

#[test]
fn packs_simple_object_members_and_joins_only_when_the_whole_line_fits() {
    let input = "class C\n{\n    void M()\n    {\n        Item item = new Item\n        {\n            Key = key,\n            Value = value\n        };\n        LongNamedItem longNamedItem = new LongNamedItem\n        {\n            First = first,\n            Second = second,\n            Third = third\n        };\n    }\n}\n";
    let expected = "class C\n{\n    void M()\n    {\n        Item item = new Item { Key = key, Value = value };\n        LongNamedItem longNamedItem = new LongNamedItem\n        {\n            First = first, Second = second, Third = third\n        };\n    }\n}\n";
    assert_cleanup(input, expected);
}

#[test]
fn wraps_long_literal_assignment_without_changing_literal_bytes() {
    let input = "class C\n{\n    void M()\n    {\n        const string sample = \"{\\\"message\\\":\\\"A deliberately long fixture string with escapes and punctuation,()\\\"}\";\n    }\n}\n";
    let expected = "class C\n{\n    void M()\n    {\n        const string sample =\n            \"{\\\"message\\\":\\\"A deliberately long fixture string with escapes and punctuation,()\\\"}\";\n    }\n}\n";
    assert_cleanup(input, expected);
}

#[test]
fn preserves_comments_nested_values_and_trailing_commas() {
    for members in [
        "            Key = key, // explanation\n            Value = value",
        "            Key = Make(first, second),\n            Value = value",
        "            Key = key,\n            Value = value,",
    ] {
        let input = format!("class C\n{{\n    void M()\n    {{\n        Item item = new Item\n        {{\n{members}\n        }};\n    }}\n}}\n");
        assert_cleanup(&input, &input);
    }
}

#[test]
fn honors_keep_existing_configuration() {
    let input = "class C\n{\n    void AMethodWithALongDescriptiveName(\n        string first, string second, string third, string fourth)\n    {\n        Item item = new Item\n        {\n            Key = first,\n            Value = second\n        };\n    }\n}\n";
    let options = CSharpOptions::from_properties(&Properties::from_pairs(&[
        ("max_line_length", "80"),
        ("resharper_csharp_wrap_parameters_style", "chop_if_long"),
        (
            "resharper_keep_existing_declaration_parens_arrangement",
            "true",
        ),
        ("resharper_keep_existing_initializer_arrangement", "true"),
    ]));
    assert_eq!(format_csharp(input, options), input);
}

#[test]
fn leaves_namespace_imports_to_a_reference_aware_cleanup() {
    let input = "class C\n{\n    void M()\n    {\n        object value = Newtonsoft.Json.JsonConvert.DeserializeObject<object>(s);\n    }\n}\n";
    assert_cleanup(input, input);
}

#[test]
fn does_not_rewrite_multiline_literal_contents_or_commented_parameters() {
    let input = "class C\n{\n    void AMethodWithALongDescriptiveName(\n        string first, /* separator */ string second, string third, string fourth)\n    {\n        string value = @\"\n        Item item = new Item\n        {\n            Key = key,\n            Value = value\n        };\n\";\n    }\n}\n";
    assert_cleanup(input, input);
}

#[test]
fn wraps_literal_with_the_default_margin_when_editorconfig_omits_it() {
    let literal = "x".repeat(160);
    let input = format!("class C\n{{\n    void M()\n    {{\n        const string sample = \"{literal}\";\n    }}\n}}\n");
    let expected = format!("class C\n{{\n    void M()\n    {{\n        const string sample =\n            \"{literal}\";\n    }}\n}}\n");
    assert_eq!(
        format_csharp(
            &input,
            CSharpOptions::from_properties(&Properties::from_pairs(&[]))
        ),
        expected
    );
}
