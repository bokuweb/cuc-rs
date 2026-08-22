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
- conservative XAML tag/attribute spacing and continuation alignment, including
  SDK-style WPF projects' implicit `Page` and `ApplicationDefinition` items

Modifier ordering and broad token spacing rewrites are implemented as internal
experiments but are not enabled in `--csharp` yet. They need a syntax-aware
implementation before they are safe enough to apply to real C#.

Unused `using` removal and member-layout reordering are intentionally not
enabled because both require a fully resolved semantic model. Earlier local
heuristics are retained as testable experiments, but an authoritative
Windows/VSTO run showed that enabling them could rewrite already-clean Elsa
sources.

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
versions. As of 2026-08-22, the safety-first formatter matches 25/40 files in
the selected historical macOS corpus, with 8 partial and 7 missed. That corpus
is useful for regression discovery but is not certification: missing VSTO and
.NET Framework references can make ReSharper remove imports or rearrange
members differently. Ten former matches were deliberately given up after those
semantic guesses produced false positives under Windows. The Windows/VSTO
result is therefore the authority rather than macOS output.

On Elsa `main` at `1645ba170fb91c8bc4488e5e6c1ffda0a2eca9f4`, the existing
Windows cleanup job (ReSharper 2025.1.2 with restored VSTO/.NET Framework
references) completed with zero diff. cuc checked the same 1,107 files twice
with zero diff in 2.18 s and 1.99 s respectively; the Windows cleanup job's
cleanup step took 15 min 20 s after setup. This proves clean-baseline
agreement for that revision, not full replacement parity on arbitrary dirty
inputs.

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
and cleanupcode diffs when parity or idempotency fails. Because Elsa is private,
the cuc-rs repository must define an `ELSA_REPO_TOKEN` Actions secret containing
a fine-grained token with read-only `Contents` access to `jlsi/elsa`; do not use
a broad personal token.
