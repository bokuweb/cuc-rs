# cuc-rs

`cuc` is a prototype fast cleanup and formatter tool driven by `.editorconfig`.

> **Warning**
>
> This project is experimental. Do not use it for production formatting or as a
> replacement for CI-enforced ReSharper `cleanupcode`.

It is not a ReSharper `cleanupcode` replacement yet. The current target is a
fast formatter core. `--text` applies trailing-whitespace, final-newline, and
EOL rules from `.editorconfig` while preserving the existing UTF-8 BOM state.
When combined with a C# mode, text cleanup is limited to C# files. `--indent` is
accepted for rule parsing and compatibility testing, but currently preserves
leading indentation.

Experimental C# formatter passes are available behind `--csharp`.
Newline-only C# checks are available behind `--csharp-newlines`.

- new lines before `else`, `catch`, and `finally`
- file-header `using` sorting and duplicate removal, excluding generated files,
  local `using` statements, and string/comment contents
- conservative control-keyword spacing such as `if (` / `for (` / `when (`
- conservative folding of simple two-line expressions, argument lists,
  initializers, expression-bodied members, and selected lambda layouts when
  ReSharper is configured not to preserve the existing arrangement
- replacement of `var` for simple object creation when the configured style
  prefers an apparent explicit type
- syntax-tree-guarded relocation of misplaced fields, constructors, nested
  types, and interface implementation members
- conservative removal of clearly unused `using System;` directives
- conservative XAML tag/attribute spacing and continuation alignment, including
  SDK-style WPF projects' implicit `Page` and `ApplicationDefinition` items

Modifier ordering and broad token spacing rewrites are implemented as internal
experiments but are not enabled in `--csharp` yet. They need a syntax-aware
implementation before they are safe enough to apply to real C#.

General unused `using` removal is intentionally not implemented yet because it
needs a semantic model. The enabled pass only removes a small set of usages it
can prove locally.

## Usage

```sh
cargo run -- --config ../elsa/.editorconfig --csharp-newlines ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --csharp ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --indent --csharp ../elsa/Elsa --check --list
cargo run -- --config ../elsa/.editorconfig --text --csharp ../elsa/Elsa.sln --check --list
```

Passing a solution limits cleanup to C# files included by its projects, matching
`cleanupcode Elsa.sln` more closely than recursive directory traversal.

Committed ReSharper cleanup results can be replayed as an oracle. For every
UTF-8 file changed by the commit, cuc formats the parent version in memory and reports
whether it exactly matches the committed result:

```sh
cargo run -- --config ../elsa/.editorconfig --text --csharp \
  --compare-repo ../elsa --compare-commit 3c9b17764 --list
```

The oracle classifies each changed UTF-8 file as `exact`, `partial`, or
`missed`. It is a regression corpus, not the final authority: a commit can mix
manual edits with cleanup output, and output can differ between ReSharper
versions. As of 2026-08-22, the selected Elsa corpus is 35/40 exact, 3 partial,
and 2 missed. On the current 887-file Elsa solution, a macOS diagnostic run
matches 29 of the 32 C# bodies changed by ReSharper and all XAML/XML output.
The three remaining C# differences are tied to unresolved references: one
ReSharper transformation changes exception behavior, while the other two
member-order changes disappear in an equivalent fully resolved project. The
Windows/VSTO result is therefore the authority rather than that macOS output.

Replacement readiness requires byte-for-byte agreement with the pinned
ReSharper version on independently formatted Windows worktrees and two
consecutive idempotent runs from both tools. macOS cannot resolve Elsa's VSTO
and .NET Framework references and is therefore useful for syntax/layout
diagnostics, but not for final semantic-cleanup parity.

On Windows, historical inputs can also be formatted independently by cuc and
the real ReSharper executable, then compared byte-for-byte:

```powershell
cargo build --release
./scripts/compare-elsa-cleanup.ps1 -ElsaRepo ../elsa -CurrentOnly
# Final certification (current HEAD plus all historical inputs):
./scripts/compare-elsa-cleanup.ps1 -ElsaRepo ../elsa
```

This requires `jb` from `JetBrains.ReSharper.GlobalTools`. Each comparison uses
detached temporary worktrees, runs both formatters twice to verify idempotency,
and does not modify the Elsa working tree. By default it checks the parents of
all 18 commits in the documented 40-file regression corpus plus the current
Elsa `HEAD`; `-CurrentOnly` is the faster iteration gate.
The manually triggered `elsa-parity` GitHub Actions job runs the same comparison
on Windows with the same ReSharper CLI version pinned by Elsa CI (2025.1.2),
accepts the Elsa ref and current/full scope as inputs, and uploads separate cuc
and cleanupcode diffs when parity or idempotency fails.
