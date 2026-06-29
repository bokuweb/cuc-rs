# cuc-rs

`cuc` is a prototype fast cleanup and formatter tool driven by `.editorconfig`.

It is not a ReSharper `cleanupcode` replacement yet. The current target is a
fast formatter core. Basic text cleanup is available behind `--text`:

- `trim_trailing_whitespace`
- `insert_final_newline`
- `end_of_line`
- `charset = utf-8` / `utf-8-bom`
- optional leading indentation conversion via `--indent`

Experimental C# formatter passes are available behind `--csharp`.

- `using` directive sorting and duplicate removal
- modifier ordering via `csharp_preferred_modifier_order`
- token spacing for comma, dot, semicolon, selected binary operators, and
  parentheses/brackets
- new lines before `else`, `catch`, and `finally`

Unused `using` removal is intentionally not implemented yet because it needs a
semantic model.

## Usage

```sh
cargo run -- --config ../elsa/.editorconfig --text ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text ../elsa/Elsa --list
cargo run -- --config ../elsa/.editorconfig --text --csharp ../elsa/Elsa --check --list
```

Use `--indent` only when you want to test indentation conversion. It is opt-in
because changing leading whitespace can disturb manually aligned continuation
lines.
