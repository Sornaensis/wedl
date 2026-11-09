//! Exercise generated scripts in real shells. Only fixed grammar metadata may
//! be completed; source trees, arbitrary literals and filesystem paths are inert.
#[path = "../src/completion.rs"]
mod generator;

use clap::{Arg, ArgAction, Command as Grammar};
use serde_json::Value as Json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

fn generated(shell: &str) -> String {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("absent-world");
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .current_dir(directory.path())
        .arg("--repo")
        .arg(&missing)
        .args(["--revision", "not-a-commit", "completion", shell])
        .env("GIT_TRACE", directory.path().join("trace-must-not-exist"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!missing.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    String::from_utf8(output.stdout).unwrap()
}

fn bash_script() -> &'static str {
    static SCRIPT: OnceLock<String> = OnceLock::new();
    SCRIPT.get_or_init(|| generated("bash"))
}

fn powershell_script() -> &'static str {
    static SCRIPT: OnceLock<String> = OnceLock::new();
    SCRIPT.get_or_init(|| generated("powershell"))
}

#[test]
fn scripts_are_deterministic_repository_free_and_compact_is_refused() {
    assert_eq!(bash_script(), generated("bash"));
    assert_eq!(powershell_script(), generated("powershell"));
    assert!(bash_script().contains("complete -F _wedl_rs_completion 'wedl-rs'"));
    assert!(powershell_script().contains("Register-ArgumentCompleter -Native"));
    assert!(!powershell_script().contains("Invoke-Expression"));
    let directory = tempfile::tempdir().unwrap();
    for shell in ["bash", "powershell", "zsh"] {
        let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .current_dir(directory.path())
            .args(["--compact", "completion", shell])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Json = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "usage_error");
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[cfg(unix)]
fn bash_complete(script: &str, words: &[&str], line: Option<&str>) -> Vec<String> {
    bash_complete_at(script, words, line, None)
}

#[cfg(unix)]
fn bash_complete_at(
    script: &str,
    words: &[&str],
    line: Option<&str>,
    cursor: Option<usize>,
) -> Vec<String> {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("completion.bash");
    fs::write(&source, script).unwrap();
    fs::write(
        directory.path().join("PRIVATE_AUTHORED_WORLD.md"),
        "private",
    )
    .unwrap();
    let sentinel = directory.path().join("must-not-execute");
    let mut command = Command::new("bash");
    command
        .current_dir(directory.path())
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .env_remove("WEDL_TEST_LINE")
        .env_remove("WEDL_TEST_POINT")
        .env("WEDL_COMPLETION_SENTINEL", &sentinel)
        .args([
            "--noprofile",
            "--norc",
            "-c",
            r#"
set -euo pipefail
export LC_ALL=C
source "$1"
shift
wedl-rs() { printf 'called' > "$WEDL_COMPLETION_SENTINEL"; return 91; }
COMP_WORDS=("$@")
COMP_CWORD=$((${#COMP_WORDS[@]}-1))
if [[ ${WEDL_TEST_LINE+x} ]]; then
    COMP_LINE=$WEDL_TEST_LINE
    COMP_POINT=${WEDL_TEST_POINT:-${#COMP_LINE}}
else
    unset COMP_LINE COMP_POINT
fi
_wedl_rs_completion
if ((${#COMPREPLY[@]})); then printf '%s\0' "${COMPREPLY[@]}"; fi
"#,
            "completion-test",
        ])
        .arg(&source)
        .args(words);
    if let Some(line) = line {
        command.env("WEDL_TEST_LINE", line);
    }
    if let Some(cursor) = cursor {
        command.env("WEDL_TEST_POINT", cursor.to_string());
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "words={words:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!sentinel.exists(), "completion invoked the executable");
    assert_eq!(
        fs::read_to_string(directory.path().join("PRIVATE_AUTHORED_WORLD.md")).unwrap(),
        "private"
    );
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8(value.to_vec()).unwrap())
        .collect()
}

#[cfg(unix)]
#[test]
fn bash_completes_current_grammar_paths_flags_and_fixed_choices_only() {
    let script = bash_script();
    let top = bash_complete(script, &["wedl-rs", ""], None);
    for expected in [
        "authoring",
        "catalogue",
        "completion",
        "generational",
        "spatial",
    ] {
        assert!(
            top.iter().any(|word| word == expected),
            "{expected}: {top:?}"
        );
    }
    assert_eq!(
        bash_complete(script, &["wedl-rs", "authoring", "sch"], None),
        ["schema"]
    );
    assert_eq!(
        bash_complete(script, &["wedl-rs", "authoring", "s"], None),
        ["scaffold", "schema"]
    );
    assert_eq!(
        bash_complete(script, &["wedl-rs", "authoring", "con"], None),
        ["context"]
    );
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "authoring", "scaffold", "--family", "sp"],
            None
        ),
        ["spatial-history"]
    );
    assert_eq!(
        bash_complete(script, &["wedl-rs", "catalogue", "p"], None),
        ["page"]
    );
    assert_eq!(
        bash_complete(script, &["wedl-rs", "completion", "po"], None),
        ["powershell"]
    );
    assert_eq!(
        bash_complete(script, &["wedl-rs", "completion", "bash", ""], None),
        Vec::<String>::new()
    );
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "spatial", "path", "from", "to", "--metric", "d"],
            None
        ),
        ["duration"]
    );
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "causal", "event", "--direction", "d"],
            None
        ),
        ["downstream"]
    );
    let options = bash_complete(script, &["wedl-rs", "catalogue", "page", "--"], None);
    assert!(options.contains(&"--repo".into()) && options.contains(&"--file".into()));
    assert!(bash_complete(script, &["wedl-rs", "state", "PRIVATE"], None).is_empty());
    assert!(
        bash_complete(script, &["wedl-rs", "search", "--mode", ""], None).is_empty(),
        "do not invent choices absent from Clap"
    );
    assert!(
        bash_complete(script, &["wedl-rs", "completion", "*"], None).is_empty(),
        "prefixes are literal, not wildcard patterns"
    );
}

#[cfg(unix)]
#[test]
fn bash_consumes_values_positionals_negative_numbers_and_end_of_options() {
    let script = bash_script();
    for words in [
        vec!["wedl-rs", "--repo", "catalogue", "authoring", "sch"],
        vec!["wedl-rs", "--repo=catalogue", "authoring", "sch"],
        vec!["wedl-rs", "--repo", "=", "catalogue", "authoring", "sch"],
        vec![
            "wedl-rs",
            "--repo",
            "a path with spaces",
            "authoring",
            "sch",
        ],
    ] {
        assert_eq!(bash_complete(script, &words, None), ["schema"], "{words:?}");
    }
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "state", "authoring", "--tick", "-1", "--ord"],
            None
        ),
        ["--order"]
    );
    assert_eq!(
        bash_complete(
            script,
            &[
                "wedl-rs",
                "generational",
                "labels",
                "authoring",
                "schema",
                "--tick",
                "-5",
                "--lim"
            ],
            None
        ),
        ["--limit"]
    );
    assert!(bash_complete(script, &["wedl-rs", "--repo", ""], None).is_empty());
    assert!(bash_complete(script, &["wedl-rs", "--", "catalogue", "p"], None).is_empty());
    assert!(bash_complete(script, &["wedl-rs", "state", "--", "--repo", "--"], None).is_empty());
    assert!(bash_complete(script, &["wedl-rs", "state", "catalogue", "p"], None).is_empty());
}

#[cfg(unix)]
#[test]
fn bash_handles_both_actual_equals_word_breaks_and_unsplit_assignments() {
    let script = bash_script();
    let base = ["wedl-rs", "spatial", "path", "from", "to"];
    for (suffix, line, expected) in [
        (
            vec!["--metric", "="],
            "wedl-rs spatial path from to --metric=",
            vec!["duration", "hops", "route_distance", "travel_cost"],
        ),
        (
            vec!["--metric", "=", "du"],
            "wedl-rs spatial path from to --metric=du",
            vec!["duration"],
        ),
        (
            vec!["--metric=du"],
            "wedl-rs spatial path from to --metric=du",
            vec!["--metric=duration"],
        ),
        (
            vec!["--metric", "=", "du"],
            "wedl-rs spatial path from to --metric = du",
            vec![],
        ),
        (
            vec!["--metric", "=", "duration", "--lim"],
            "wedl-rs spatial path from to --metric=duration --lim",
            vec!["--limit"],
        ),
    ] {
        let mut words = base.to_vec();
        words.extend(suffix);
        assert_eq!(
            bash_complete(script, &words, Some(line)),
            expected,
            "{line}"
        );
    }
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "--repo", "=", "authoring", "sch"],
            Some("wedl-rs --repo = authoring sch")
        ),
        ["schema"]
    );
    assert_eq!(
        bash_complete(
            script,
            &["wedl-rs", "--repo", "=", "authoring", "sch"],
            Some("wedl-rs --repo= authoring sch")
        ),
        ["schema"]
    );
}

#[cfg(unix)]
#[test]
fn bash_reassembles_adjacent_literal_fragments_and_decodes_quotes_without_eval() {
    let script = bash_script();
    for (words, line, expected) in [
        (
            vec![
                "wedl-rs", "spatial", "path", "location", ":", "region/a", "location", ":",
                "region/b", "--met",
            ],
            "wedl-rs spatial path location:region/a location:region/b --met",
            "--metric",
        ),
        (
            vec!["wedl-rs", "--repo", "x", "=", "y", "authoring", "sch"],
            "wedl-rs --repo x=y authoring sch",
            "schema",
        ),
        (
            vec!["wedl-rs", "--repo", "a", "@", "b", "authoring", "sch"],
            "wedl-rs --repo a@b authoring sch",
            "schema",
        ),
        (
            vec!["wedl-rs", "'authoring'", "sch"],
            "wedl-rs 'authoring' sch",
            "schema",
        ),
        (
            vec!["wedl-rs", "completion", "'po"],
            "wedl-rs completion 'po",
            "powershell",
        ),
        (
            vec!["wedl-rs", "completion", "\"po"],
            "wedl-rs completion \"po",
            "powershell",
        ),
        (
            vec!["wedl-rs", "completion", "'po'"],
            "wedl-rs completion 'po'",
            "powershell",
        ),
    ] {
        assert_eq!(
            bash_complete(script, &words, Some(line)),
            [expected],
            "{line}"
        );
    }
    assert_eq!(
        bash_complete_at(
            script,
            &["wedl-rs", "completion", "powershell"],
            Some("wedl-rs completion powershell"),
            Some("wedl-rs completion power".len())
        ),
        ["powershell"]
    );
    assert_eq!(
        bash_complete_at(
            script,
            &[
                "wedl-rs", "spatial", "path", "from", "to", "--metric", "=", "duration"
            ],
            Some("wedl-rs spatial path from to --metric=duration"),
            Some("wedl-rs spatial path from to --metric=du".len())
        ),
        ["duration"]
    );
}

fn custom_grammar() -> Grammar {
    Grammar::new("wedl-rs")
        .arg(
            Arg::new("flag")
                .short('f')
                .long("flag")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("mode")
                .short('m')
                .long("mode")
                .visible_alias("strategy")
                .alias("hidden-mode")
                .value_parser([
                    "fast",
                    "slow",
                    "two words",
                    "$(wedl-rs)",
                    "apostrophe's",
                    "__POSITIONS__",
                    "ns:word",
                ])
                .global(true),
        )
        .arg(Arg::new("pair").long("pair").num_args(2).global(true))
        .subcommand(
            Grammar::new("write")
                .visible_alias("make")
                .alias("hidden-write")
                .arg(Arg::new("owner").required(true))
                .arg(
                    Arg::new("format")
                        .value_parser(["json", "yaml"])
                        .required(true),
                ),
        )
}

#[cfg(unix)]
#[test]
fn bash_uses_parser_aliases_arity_and_inert_quoted_literals() {
    let script = generator::render(custom_grammar(), generator::Shell::Bash).unwrap();
    assert_eq!(bash_complete(&script, &["wedl-rs", "ma"], None), ["make"]);
    assert!(bash_complete(&script, &["wedl-rs", "hidden"], None).is_empty());
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "hidden-write", "owner", "j"], None),
        ["json"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--strategy", "s"], None),
        ["slow"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "-fmf"], None),
        ["-fmfast"]
    );
    assert_eq!(
        bash_complete(
            &script,
            &["wedl-rs", "--pair", "write", "write", "ma"],
            None
        ),
        ["make"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "write", "make", "y"], None),
        ["yaml"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "\\$\\("], None),
        ["\\$\\(wedl-rs\\)"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "'$("], None),
        ["$(wedl-rs)"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "\"\\$("], None),
        ["\\$(wedl-rs)"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "two"], None),
        ["two\\ words"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "'two"], None),
        ["two words"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "\"two"], None),
        ["two words"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "apostrophe"], None),
        ["apostrophe\\'s"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "'apostrophe"], None),
        ["apostrophe'\\''s"]
    );
    assert_eq!(
        bash_complete(&script, &["wedl-rs", "--mode", "__P"], None),
        ["__POSITIONS__"]
    );
    assert_eq!(
        bash_complete(
            &script,
            &["wedl-rs", "--mode", "ns", ":", "w"],
            Some("wedl-rs --mode ns:w")
        ),
        ["word"]
    );
}

fn pwsh() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("WEDL_TEST_PWSH") {
        return Some(value.into());
    }
    Command::new("pwsh")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ])
        .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from("pwsh"))
}

fn powershell_complete(executable: &Path, script: &str, lines: &[&str]) -> Vec<Vec<String>> {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("completion.ps1");
    let requests = directory.path().join("requests.json");
    let runner = directory.path().join("runner.ps1");
    let sentinel = directory.path().join("must-not-execute");
    fs::write(&source, script).unwrap();
    fs::write(&requests, serde_json::to_vec(&lines).unwrap()).unwrap();
    fs::write(
        directory.path().join("PRIVATE_AUTHORED_WORLD.md"),
        "private",
    )
    .unwrap();
    fs::write(&runner, r#"
$ErrorActionPreference = 'Stop'
. $env:WEDL_COMPLETION_SCRIPT
$lines = ConvertFrom-Json -InputObject (Get-Content -Raw -LiteralPath $env:WEDL_COMPLETION_REQUESTS)
$results = @(
    foreach ($line in $lines) {
        $complete = TabExpansion2 $line $line.Length
        [pscustomobject]@{ values = @($complete.CompletionMatches | ForEach-Object { $_.CompletionText }) }
    }
)
ConvertTo-Json -Depth 5 -Compress -InputObject $results
"#).unwrap();
    let mut command = Command::new(executable);
    command
        .current_dir(directory.path())
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(&runner)
        .env("WEDL_COMPLETION_SCRIPT", &source)
        .env("WEDL_COMPLETION_REQUESTS", &requests)
        .env("WEDL_COMPLETION_SENTINEL", &sentinel)
        .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let stub = directory.path().join("wedl-rs");
        fs::write(
            &stub,
            "#!/bin/sh\nprintf 'called' > \"$WEDL_COMPLETION_SENTINEL\"\nexit 91\n",
        )
        .unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        let paths = std::iter::once(directory.path().to_owned())
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ))
            .collect::<Vec<_>>();
        command.env("PATH", std::env::join_paths(paths).unwrap());
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!sentinel.exists(), "completion invoked WEDL");
    assert_eq!(
        fs::read_to_string(directory.path().join("PRIVATE_AUTHORED_WORLD.md")).unwrap(),
        "private"
    );
    let results: Vec<Json> = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stdout)));
    assert_eq!(results.len(), lines.len());
    results
        .iter()
        .map(|row| {
            row["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

#[test]
fn powershell_generation_and_available_runtime_follow_the_native_grammar() {
    let script = powershell_script();
    assert!(script.contains("StartsWith($prefix,[System.StringComparison]::Ordinal)"));
    let Some(executable) = pwsh() else {
        eprintln!(
            "PowerShell runtime unavailable; generation checked. Set WEDL_TEST_PWSH for runtime coverage."
        );
        return;
    };
    let lines = [
        "wedl-rs authoring sch",
        "wedl-rs authoring sca",
        "wedl-rs authoring con",
        "wedl-rs authoring scaffold --family sp",
        "wedl-rs completion po",
        "wedl-rs completion 'po",
        "wedl-rs --repo 'catalogue' authoring sch",
        "wedl-rs --repo=catalogue authoring sch",
        "wedl-rs state authoring --tick -1 --ord",
        "wedl-rs generational labels authoring schema --tick -5 --lim",
        "wedl-rs spatial path from to --metric du",
        "wedl-rs spatial path from to --metric=du",
        "wedl-rs spatial path from to --metric='du'",
        "wedl-rs spatial path from to --metric \"du\"",
    ];
    let results = powershell_complete(&executable, script, &lines);
    let expected = [
        "schema",
        "scaffold",
        "context",
        "spatial-history",
        "powershell",
        "powershell",
        "schema",
        "schema",
        "--order",
        "--limit",
        "duration",
        "--metric=duration",
        "--metric=duration",
        "duration",
    ];
    for ((line, actual), expected) in lines.iter().zip(results).zip(expected) {
        assert_eq!(actual, [expected], "{line}");
    }
}

#[test]
fn powershell_no_match_preserves_literals_without_path_fallback_or_execution() {
    let script = generator::render(custom_grammar(), generator::Shell::Powershell).unwrap();
    assert!(script.contains("No fixed completion; retain literal input"));
    let Some(executable) = pwsh() else {
        eprintln!(
            "PowerShell runtime unavailable; generation checked. Set WEDL_TEST_PWSH for runtime coverage."
        );
        return;
    };
    let lines = [
        "wedl-rs --mode $(",
        "wedl-rs --mode '$(wedl-rs)'",
        "wedl-rs --mode two",
        "wedl-rs --mode apostrophe",
        "wedl-rs --mode __P",
        "wedl-rs --hidden-mode s",
        "wedl-rs -fmf",
        "wedl-rs --pair write write ma",
        "wedl-rs write make y",
        "wedl-rs write PRIVATE",
        "wedl-rs write ",
        "wedl-rs -- ma",
        "wedl-rs write '*'",
        "wedl-rs write 'two words' ",
        "wedl-rs write 'apostrophe''s'",
        "wedl-rs write 'a`b'",
        "wedl-rs write '$HOME'",
        "wedl-rs write $(wedl-rs)",
        "wedl-rs --mode *",
        "wedl-rs --pair $(wedl-rs) ignored ma",
    ];
    let results = powershell_complete(&executable, &script, &lines);
    let expected = [
        "''",
        "'$(wedl-rs)'",
        "'two words'",
        "'apostrophe''s'",
        "__POSITIONS__",
        "slow",
        "-fmfast",
        "make",
        "yaml",
        "PRIVATE",
        " ",
        "ma",
        "'*'",
        "json",
        "'apostrophe''s'",
        "'a`b'",
        "'$HOME'",
        "''",
        "'*'",
        "ma",
    ];
    for ((line, actual), expected) in lines.iter().zip(results).zip(expected) {
        // Inside a shell expression PowerShell does not invoke the
        // native callback; the generator neither evaluates nor completes it.
        if matches!(*line, "wedl-rs --mode $(" | "wedl-rs write $(wedl-rs)") {
            assert!(actual.is_empty(), "{line}: {actual:?}");
        } else if *line == "wedl-rs write 'two words' " {
            assert_eq!(actual, ["json", "yaml"], "{line}");
        } else {
            assert_eq!(actual, [expected], "{line}");
        }
    }
    assert_eq!(lines.len(), expected.len());
}
