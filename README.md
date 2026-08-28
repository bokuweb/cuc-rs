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

Unused `using` removal and general member-layout reordering are intentionally
not enabled because both require a fully resolved semantic model. Earlier local
heuristics are retained as testable experiments, but an authoritative
Windows/VSTO run showed that enabling them could rewrite already-clean Elsa
sources. Solution mode performs one narrower layout fix: when declarations prove
that a later interface adds a public method overload, that overload is moved in
front of the primary interface implementation group. Private methods are never
treated as interface implementations.

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
is useful for regression discovery but is not certification: ReSharper runs
without restored VSTO, .NET Framework, and NuGet references can remove imports
or rearrange members incorrectly. Ten former matches were deliberately given up
after those semantic guesses produced false positives under an authoritative
Windows/VSTO run.

On Elsa `main` at `1645ba170fb91c8bc4488e5e6c1ffda0a2eca9f4`, the existing
Windows cleanup job (ReSharper 2025.1.2 with restored VSTO/.NET Framework
references) completed with zero diff. A second, local macOS oracle run restored
all 11 solution projects, supplied the .NET Framework and VSTO reference
assemblies, reported no unresolved references, and also completed with zero
source diff in 17 min 58.85 s. cuc checked the same 1,107 files twice with zero
diff in 4.45 s and 4.54 s in the final cold local run (earlier warm runs were
about 2 s). This proves clean-baseline agreement for that revision, not full
replacement parity on arbitrary dirty inputs.

Replacement readiness requires byte-for-byte agreement with the pinned
ReSharper version on independently formatted Windows worktrees and two
consecutive idempotent runs from both tools. A macOS diagnostic run is valid
only when every solution project has been restored and the .NET Framework,
VSTO, and Office reference assemblies are supplied explicitly; the Windows/VSTO
run remains the release authority.

On Windows, historical inputs can also be formatted independently by cuc and
the real ReSharper executable, then compared byte-for-byte:

```powershell
cargo build --release
./scripts/compare-elsa-cleanup.ps1 -ElsaRepo ../elsa -CurrentOnly
# Final certification (current HEAD plus all historical inputs):
./scripts/compare-elsa-cleanup.ps1 -ElsaRepo ../elsa
```

This requires `jb` from `JetBrains.ReSharper.GlobalTools`, `MSBuild`, the VSTO
build tools, and the same authenticated local NuGet sources used to build Elsa.
Each comparison uses detached temporary worktrees, performs a locked-mode
solution restore before ReSharper analysis, runs both formatters twice to verify
idempotency, and does not modify the Elsa working tree. By default it checks the
parents of all 18 commits in the documented 40-file regression corpus plus the
current Elsa `HEAD`; `-CurrentOnly` is the faster iteration gate. Elsa parity is
intentionally a local-only check: the cuc-rs GitHub Actions workflow neither
clones Elsa nor requires an Elsa repository secret.
