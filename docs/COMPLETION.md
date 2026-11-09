# Shell completion

`wedl-rs completion bash` and `wedl-rs completion powershell` print deterministic
scripts derived from the executable's actual Clap grammar. They run before world
loading, including when `--repo` names an absent directory or `--revision` is
unavailable. Output is shell source; combining completion with `--compact` is an
error. Generation does not create a world, cache, Git object or configuration file.

## Bash

```bash
wedl-rs completion bash > wedl-rs-completion.bash
source ./wedl-rs-completion.bash
```

## PowerShell

```powershell
wedl-rs completion powershell | Set-Content -Encoding utf8 ./wedl-rs-completion.ps1
. ./wedl-rs-completion.ps1
```

The PowerShell script registers native completers for both `wedl-rs` and
`wedl-rs.exe`. Runtime checks in this iteration used PowerShell 7.4.22.
Regenerate either script after changing the CLI grammar.

## Behavior

- Complete command paths, visible options/aliases and fixed choices declared by
  Clap. Hidden aliases remain recognized for traversal without being suggested.
  Custom value parsers without a fixed-choice catalogue remain freeform; help
  prose and defaults do not invent choices.
- Consume option values and positional arguments before considering another
  command. This includes negative values, repeated/variadic operands, attached
  option values and `--`. A literal equal to a command name stays a value.
- Bash reconstructs adjacent Readline fragments such as `a:b`, `x=y` and
  `--metric=duration`, respecting the cursor position and literal quoting.
  Single/double quotes and backslash escapes are decoded without evaluating
  variables, substitutions or glob expansions.
- PowerShell inspects literal syntax nodes and bare option tokens without
  evaluating expressions. Matching is case-sensitive and treats wildcard
  characters as literal text.
- Pressing Tab does not invoke WEDL, query authored records or list files. The
  native PowerShell callback suppresses its normal filename fallback with a
  no-op result when no fixed choice matches: retain a safely quoted literal, or
  add harmless whitespace for a blank token. An argument that cannot be treated
  as a literal receives an empty quoted token. PowerShell expression contexts
  that do not invoke the native callback remain controlled by PowerShell itself.

Completion is a typing aid, not validation of every combination of arguments.
The ordinary CLI parser and source validation remain authoritative. No authored
entity names, filesystem paths or source declarations are embedded in a script.

The [completion regressions](../tests/completion.rs) exercise the generated Bash
functions and actual PowerShell `TabExpansion2`, including quoting, hidden
aliases, fixed choices, source independence and filename/execution sentinels.
PowerShell runtime checks run when `pwsh` is on `PATH`, or when
`WEDL_TEST_PWSH` names its executable; otherwise script generation is still
checked and the missing runtime is reported by the test.
