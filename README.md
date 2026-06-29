# cuc-rs

`cuc` is a prototype fast cleanup and formatter tool driven by `.editorconfig`.

> **Warning**
>
> This project is experimental. Do not use it for production formatting or as a
> replacement for CI-enforced ReSharper `cleanupcode`.

It is not a ReSharper `cleanupcode` replacement yet. The current target is a
fast formatter core. Basic text cleanup is available behind `--text`:

- `trim_trailing_whitespace`
- `insert_final_newline`
- `end_of_line`
- `charset = utf-8` / `utf-8-bom`
- optional leading indentation conversion via `--indent`

Experimental C# formatter passes are available behind `--csharp`.
Newline-only C# checks are available behind `--csharp-newlines`.

- new lines before `else`, `catch`, and `finally`

Using sorting, modifier ordering, and token spacing rewrites are implemented as
internal experiments but are not enabled in `--csharp` yet. They need a
syntax-aware implementation before they are safe enough to apply to real C#.

Unused `using` removal is intentionally not implemented yet because it needs a
semantic model.

## Usage

```sh
cargo run -- --config ../elsa/.editorconfig --text ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text ../elsa/Elsa --list
cargo run -- --config ../elsa/.editorconfig --csharp-newlines ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --csharp ../elsa/Elsa --check --list
```

Use `--indent` only when you want to test indentation conversion. It is opt-in
because changing leading whitespace can disturb manually aligned continuation
lines.
