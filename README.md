# cuc-rs

`cuc` is a prototype fast cleanup and formatter tool driven by `.editorconfig`.

> **Warning**
>
> This project is experimental. Do not use it for production formatting or as a
> replacement for CI-enforced ReSharper `cleanupcode`.

It is not a ReSharper `cleanupcode` replacement yet. The current target is a
fast formatter core. `--text` and `--indent` are accepted for rule parsing and
compatibility testing, but they currently preserve existing text bytes instead
of rewriting trailing whitespace, final newlines, EOLs, BOMs, or leading
indentation.

Experimental C# formatter passes are available behind `--csharp`.
Newline-only C# checks are available behind `--csharp-newlines`.

- new lines before `else`, `catch`, and `finally`
- file-header `using` sorting and duplicate removal, excluding generated files,
  local `using` statements, and string/comment contents
- conservative control-keyword spacing such as `if (` / `for (` / `when (`

Modifier ordering and broad token spacing rewrites are implemented as internal
experiments but are not enabled in `--csharp` yet. They need a syntax-aware
implementation before they are safe enough to apply to real C#.

Unused `using` removal is intentionally not implemented yet because it needs a
semantic model.

## Usage

```sh
cargo run -- --config ../elsa/.editorconfig --csharp-newlines ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --csharp ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --indent --csharp ../elsa/Elsa --check --list
```
