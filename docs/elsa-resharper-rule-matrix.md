# Elsa ReSharper cleanup rule matrix

This document tracks the behavior that `cuc` must reproduce for Elsa. The
reference command is:

```text
jb cleanupcode Elsa.sln --config-file=.editorconfig \
  --settings=Elsa.sln.DotSettings --no-build --severity=WARNING \
  --exclude=**/*.html
```

No `--profile` is specified and Elsa does not define a custom cleanup profile.
ReSharper therefore uses `Built-in: Full Cleanup`: every available cleanup task
except file-header updates. `.editorconfig` overrides matching ReSharper
settings. `Elsa.sln.DotSettings` only contains HTML/exclusion and migration
settings; it does not narrow the Full Cleanup task set.

## Required task groups

| Task group | Elsa settings or Full Cleanup behavior | cuc status |
| --- | --- | --- |
| Text | trim trailing whitespace; C# final newline; preserve observed BOM/EOL behavior | Implemented |
| C# formatting | Allman braces, four-space indentation, spacing, blank lines, wrapping at the effective margin | Partial |
| C# syntax style | explicit apparent types, expression bodies, modifier order, braces, parentheses, qualifiers, built-in type names, named arguments, defaults, trailing commas | Partial |
| Namespace imports | remove unused, shorten qualified names, System-first sorting, separate import groups | Sorting and selected shortening implemented; semantic removal incomplete |
| File/type layout | apply ReSharper's effective default file-layout pattern | Disabled in production until symbol relationships are resolved safely |
| Redundancies | remove safe redundancies; make fields readonly; simplify properties where valid | Selected proven rewrites only |
| XML doc comments | reformat embedded XML documentation | Not implemented |
| XAML | reformat and collapse empty tags; remove redundant namespace aliases | Partial |
| XML and project files | ReSharper formatting for solution items | Text-only/partial |
| HTML | explicitly excluded and formatter disabled by Elsa settings | Implemented as exclusion |
| Generated code | ReSharper skips generated code | Partial project/file detection |

## Elsa C# settings that materially affect output

- `csharp_preferred_modifier_order`: public, private, protected, internal,
  static, extern, new, virtual, abstract, sealed, override, readonly, unsafe,
  volatile, async.
- `csharp_new_line_before_open_brace = all` and new lines before `else`,
  `catch`, and `finally`.
- Braces are required for `if/else`, `for`, `foreach`, and `while`.
- Existing declaration, invocation, initializer, embedded-statement,
  expression-member, property-pattern, and switch-expression arrangements are
  not preserved where the corresponding ReSharper setting is false.
- Arguments, parameters, array initializers, and chained calls use
  `chop_if_long` wrapping.
- `csharp_style_var_* = false`: prefer explicit types, including apparent
  object creation where the type can be proven.
- Expression-bodied constructors, methods, operators, properties, indexers,
  accessors, lambdas, and local functions are preferred.
- System imports sort first and import groups are separated.
- Namespace imports prefer the global qualifier according to the ReSharper
  namespace-import settings, subject to cleanup's shortening/import rules.

## Acceptance criteria

Complete replacement requires all of the following:

1. A fully restored Elsa solution with no unresolved references in the
   ReSharper oracle.
2. Byte-for-byte equality after independently running pinned ReSharper 2025.1.2
   and `cuc` on controlled dirty-input fixtures for every task group above.
3. Equality on current Elsa and the historical diagnostic corpus, excluding
   commits whose after-image contains non-cleanup edits.
4. Two consecutive runs from both tools produce identical output.
5. The current clean Elsa baseline remains unchanged.

Semantic tasks are not considered implemented merely because they happen to
produce no diff on a clean baseline.
