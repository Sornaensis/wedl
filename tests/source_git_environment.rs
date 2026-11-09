//! Revision source reads must not let inherited Git settings create files or
//! fetch missing originals. These exercise the public CLI in fresh repositories.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::SystemTime,
};

fn clear_git_environment(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    clear_git_environment(&mut command);
    let output = command
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn fixture() -> (tempfile::TempDir, PathBuf, String) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("world");
    wedl::source::init_world(&root, "Revision environment fixture").unwrap();
    git(&root, &["init", "--quiet"]);
    git(&root, &["add", "--", "story"]);
    git(&root, &["commit", "--quiet", "-m", "Original source"]);
    let revision = git(&root, &["rev-parse", "HEAD"]);
    (directory, root, revision)
}

fn status(root: &Path, revision: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    clear_git_environment(&mut command);
    command
        .arg("--repo")
        .arg(root)
        .args(["--revision", revision, "--compact", "status"]);
    command
}

fn successful_revision(output: &Output, revision: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["revision"], revision);
    assert_eq!(result["result"]["world"], "Revision environment fixture");
}

fn image(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    fs::metadata(entry.path()).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

fn unchanged(root: &Path, before: &BTreeMap<PathBuf, (Vec<u8>, SystemTime)>) {
    let after = image(root);
    let changed: std::collections::BTreeSet<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    assert!(changed.is_empty(), "changed fixture files: {changed:?}");
}

#[test]
fn revision_reads_and_refusals_preserve_inherited_trace_destinations() {
    let (directory, root, revision) = fixture();
    let existing = directory.path().join("trace-existing.txt");
    fs::write(&existing, "untouched caller file\n").unwrap();
    let absent = directory.path().join("trace-new.json");
    let absent_perf = directory.path().join("trace-new.perf");
    let before = image(directory.path());
    for requested in [
        revision.as_str(),
        "0000000000000000000000000000000000000000",
    ] {
        let output = status(&root, requested)
            // Git's normal trace would append even to this live source file.
            .env("GIT_TRACE", root.join("story/world.md"))
            .env("GIT_TRACE2", &existing)
            .env("GIT_TRACE2_EVENT", &absent)
            .env("GIT_TRACE2_PERF", &absent_perf)
            .output()
            .unwrap();
        if requested == revision {
            successful_revision(&output, &revision);
        } else {
            assert!(!output.status.success());
        }
        unchanged(directory.path(), &before);
        assert!(!absent.exists());
        assert!(!absent_perf.exists());
    }
}

#[test]
fn inherited_git_configuration_cannot_change_revision_source_selection() {
    let (directory, root, revision) = fixture();
    let unrelated = directory.path().join("unrelated.gitconfig");
    fs::write(&unrelated, "invalid caller Git configuration\n").unwrap();
    let before = image(directory.path());
    let output = status(&root, &revision)
        .env("GIT_CONFIG_GLOBAL", &unrelated)
        .output()
        .unwrap();
    successful_revision(&output, &revision);
    unchanged(directory.path(), &before);
}

#[test]
fn missing_original_blob_refuses_without_lazy_fetch_or_object_store_writes() {
    let (directory, root, revision) = fixture();
    let remote = directory.path().join("remote.git");
    git(
        &root,
        &[
            "clone",
            "--bare",
            "--no-hardlinks",
            "--quiet",
            root.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    for (key, value) in [
        ("remote.origin.url", remote.to_str().unwrap()),
        ("remote.origin.promisor", "true"),
        ("remote.origin.partialclonefilter", "blob:none"),
        ("extensions.partialClone", "origin"),
    ] {
        git(&root, &["config", key, value]);
    }
    let blob = git(&root, &["rev-parse", &format!("{revision}:story/world.md")]);
    let object = root.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
    fs::remove_file(&object).unwrap();
    let before = image(directory.path());
    let output = status(&root, &revision)
        .env("GIT_NO_LAZY_FETCH", "0")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "missing original unexpectedly became readable: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git read failed"));
    unchanged(directory.path(), &before);
    assert!(!object.exists());
}
