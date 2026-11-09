//! Exercise the executable's typed authoring boundary, including safe refusal.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use serde_json::Value;
use tempfile::TempDir;
use wedl::source;

const CHARACTER: &str = "char_00000000000000000000000001";
const LOCATION: &str = "loc_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";

fn command(arguments: &[&str], input: Option<&str>) -> (i32, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value = serde_json::from_slice(bytes).unwrap_or_else(|_| {
        panic!(
            "non-JSON command output: {}",
            String::from_utf8_lossy(bytes)
        )
    });
    (output.status.code().unwrap(), value)
}

struct Fixture {
    directory: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("record world");
        let (code, value) = command(
            &[
                "init",
                root.to_str().unwrap(),
                "--title",
                "Record World",
                "--compact",
            ],
            None,
        );
        assert_eq!(code, 0, "{value}");
        // The blank initializer has no state schema. This one test declaration
        // makes the later authored event observable without inventing defaults.
        let world = source::load_world(&root, None).unwrap();
        let mut record = world.world_record().unwrap().clone();
        record.frontmatter["state_keys"] =
            serde_yaml::from_str("object:\n  condition:\n    type: string\n").unwrap();
        let bytes = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&record.frontmatter).unwrap(),
            record.body
        );
        std::fs::write(root.join(&record.source_path), bytes).unwrap();
        Self { directory, root }
    }

    fn call(&self, args: &[&str]) -> (i32, Value) {
        let mut all = vec!["--compact", "--repo", self.root.to_str().unwrap()];
        all.extend_from_slice(args);
        command(&all, None)
    }

    fn fingerprint(&self) -> String {
        source::load_world(&self.root, None).unwrap().fingerprint
    }

    fn request(&self, text: &str) -> PathBuf {
        let file = self.directory.path().join("request.yaml");
        std::fs::write(&file, text).unwrap();
        file
    }

    fn create(&self, text: &str) -> Value {
        let file = self.request(text);
        let fingerprint = self.fingerprint();
        let (code, preview) = self.call(&[
            "record",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
        ]);
        assert_eq!(code, 0, "{preview}");
        assert_eq!(
            self.fingerprint(),
            fingerprint,
            "default record create must only preview"
        );
        let hash = preview["result"]["afterSha256"]
            .as_str()
            .expect("preview afterSha256");
        let (code, applied) = self.call(&[
            "record",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
            "--apply",
            "--expected-after-sha256",
            hash,
        ]);
        assert_eq!(code, 0, "{applied}");
        assert_eq!(applied["protocol"], wedl::CONTRACT);
        applied
    }

    fn update_preview(&self, entity: &str, patch: &str) -> (PathBuf, String, Value) {
        let file = self.request(patch);
        let fingerprint = self.fingerprint();
        let (code, preview) = self.call(&[
            "record",
            "update",
            entity,
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
        ]);
        assert_eq!(code, 0, "{preview}");
        assert_eq!(
            self.fingerprint(),
            fingerprint,
            "default record update must only preview"
        );
        (file, fingerprint, preview)
    }
}

#[test]
fn creates_five_record_kinds_then_queries_event_state_and_knowledge() {
    let fixture = Fixture::new();
    fixture.create(&format!(
        "kind: character\nid: {CHARACTER}\ntitle: Ada\naliases: [The Witness]\nrole: witness\n"
    ));
    fixture.create(&format!(
        "kind: location\nid: {LOCATION}\ntitle: Harbor\nbody: Quiet docks.\n"
    ));
    fixture.create(&format!("kind: object\nid: {OBJECT}\ntitle: Brass Gate\ntags: [harbor]\ninitial_state:\n  condition: closed\nbody: |-\n  # Gate\n  Original exact prose.\n"));
    fixture.create(&format!("kind: event\nid: {EVENT}\ntitle: Gate Opens\ntime: {{tick: 10, order: 0}}\nlocation: {LOCATION}\nparticipants:\n- character: {CHARACTER}\n  role: witness\neffects:\n- id: effect_00000000000000000000000001\n  target: {OBJECT}\n  key: condition\n  operation: set\n  value: open\n"));
    fixture.create(&format!("kind: knowledge\nid: {KNOWLEDGE}\ntitle: Ada Knows the Gate Opened\nknower: {CHARACTER}\nclaim:\n  key: harbor.gate-open\n  statement: The brass gate opened.\n  object: {{entity: {OBJECT}}}\ntransitions:\n- id: learned-gate\n  time: {{tick: 10, order: 1}}\n  state: accepted\n  confidence: 1.0\n  acquisition: authored\n  causing_event: {EVENT}\n"));

    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records.len(), 6);
    for (id, kind) in [
        (CHARACTER, "character"),
        (LOCATION, "location"),
        (OBJECT, "object"),
        (EVENT, "event"),
        (KNOWLEDGE, "knowledge"),
    ] {
        assert_eq!(world.records[id].kind, kind);
        assert_eq!(
            world.records[id].string("schema"),
            Some(source::SOURCE_SCHEMA)
        );
    }
    let (code, before) = fixture.call(&["state", "Brass Gate", "--tick", "9"]);
    assert_eq!(code, 0, "{before}");
    assert_eq!(before["result"]["state"]["condition"], "closed");
    let (code, after) = fixture.call(&["state", OBJECT, "--tick", "10"]);
    assert_eq!(code, 0, "{after}");
    assert_eq!(after["result"]["state"]["condition"], "open");
    let (code, hidden) = fixture.call(&["knowledge", CHARACTER, "--tick", "10", "--order", "0"]);
    assert_eq!(code, 0, "{hidden}");
    assert_eq!(hidden["result"]["items"].as_array().unwrap().len(), 0);
    let (code, known) = fixture.call(&["knowledge", "The Witness", "--tick", "10"]);
    assert_eq!(code, 0, "{known}");
    assert_eq!(known["result"]["items"][0]["claimKey"], "harbor.gate-open");

    let original = world.records[OBJECT].clone();
    let (file, fingerprint, preview) =
        fixture.update_preview(OBJECT, "kind: object\ntitle: Brass Gate Renamed\n");
    let hash = preview["result"]["afterSha256"].as_str().unwrap();
    // Global flags remain valid after nested command-specific arguments.
    let (code, updated) = command(
        &[
            "record",
            "update",
            OBJECT,
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
            "--apply",
            "--expected-after-sha256",
            hash,
            "--compact",
            "--repo",
            fixture.root.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 0, "{updated}");
    let world = source::load_world(&fixture.root, None).unwrap();
    let record = &world.records[OBJECT];
    assert_eq!(record.title, "Brass Gate Renamed");
    assert_eq!(record.body, original.body);
    assert_eq!(record.get("tags"), original.get("tags"));
    assert_eq!(record.get("initial_state"), original.get("initial_state"));
    assert_eq!(record.source_path, original.source_path);
}

#[test]
fn invalid_typed_requests_and_incomplete_apply_flags_never_change_source() {
    let fixture = Fixture::new();
    fixture.create(&format!("kind: character\nid: {CHARACTER}\ntitle: Ada\n"));
    let fingerprint = fixture.fingerprint();
    for text in [
        format!("kind: object\nid: {OBJECT}\ntitel: Typo\n"),
        format!("kind: object\nid: {OBJECT}\ntitle: Typo\nunknown_field: true\n"),
        format!(
            "kind: event\nid: {EVENT}\ntitle: Missing Place\ntime: {{tick: 1}}\nlocation: loc_00000000000000000000000099\n"
        ),
    ] {
        let file = fixture.request(&text);
        let (code, result) = fixture.call(&[
            "record",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
        ]);
        assert!(
            code != 0 || result["result"]["validation"]["valid"] == false,
            "invalid draft was accepted: {result}"
        );
        assert_eq!(fixture.fingerprint(), fingerprint);
        let (code, result) = fixture.call(&[
            "record",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
            "--apply",
            "--expected-after-sha256",
            &"0".repeat(64),
        ]);
        assert_eq!(code, 2, "{result}");
        assert_eq!(fixture.fingerprint(), fingerprint);
    }
    for patch in [
        "kind: character\ntitel: Bad\n",
        "kind: character\nrole: null\n",
        "kind: character\nid: char_00000000000000000000000002\n",
    ] {
        let file = fixture.request(patch);
        let (code, result) = fixture.call(&[
            "record",
            "update",
            CHARACTER,
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
        ]);
        assert_eq!(code, 2, "{result}");
        assert_eq!(fixture.fingerprint(), fingerprint);
    }
    let file = fixture.request(&format!("kind: location\nid: {LOCATION}\ntitle: Harbor\n"));
    for extra in [
        vec!["--apply"],
        vec!["--expected-after-sha256", "not-a-preview"],
    ] {
        let mut args = vec![
            "record",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--expected-fingerprint",
            &fingerprint,
        ];
        args.extend(extra);
        let (code, error) = fixture.call(&args);
        assert_eq!(code, 2);
        assert_eq!(error["code"], "usage_error");
    }
    assert_eq!(fixture.fingerprint(), fingerprint);
}

#[test]
fn changed_request_and_stale_snapshot_cannot_reuse_preview_hash() {
    let fixture = Fixture::new();
    fixture.create(&format!("kind: character\nid: {CHARACTER}\ntitle: Ada\n"));
    let (file, fingerprint, preview) =
        fixture.update_preview(CHARACTER, "kind: character\nrole: navigator\n");
    let hash = preview["result"]["afterSha256"].as_str().unwrap();
    std::fs::write(&file, "kind: character\nrole: captain\n").unwrap();
    let (code, error) = fixture.call(&[
        "record",
        "update",
        CHARACTER,
        "--file",
        file.to_str().unwrap(),
        "--expected-fingerprint",
        &fingerprint,
        "--apply",
        "--expected-after-sha256",
        hash,
    ]);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "conflict");
    assert_eq!(fixture.fingerprint(), fingerprint);

    let stale_patch = fixture.directory.path().join("stale.yaml");
    std::fs::write(&stale_patch, "kind: character\nrole: navigator\n").unwrap();
    fixture.create(&format!("kind: location\nid: {LOCATION}\ntitle: Harbor\n"));
    let fresh = fixture.fingerprint();
    let (code, error) = fixture.call(&[
        "record",
        "update",
        CHARACTER,
        "--file",
        stale_patch.to_str().unwrap(),
        "--expected-fingerprint",
        &fingerprint,
        "--apply",
        "--expected-after-sha256",
        hash,
    ]);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "conflict");
    assert_eq!(fixture.fingerprint(), fresh);
}

#[test]
fn typed_request_accepts_stdin_and_bounds_file_size() {
    let fixture = Fixture::new();
    let fingerprint = fixture.fingerprint();
    let draft = format!("kind: character\nid: {CHARACTER}\ntitle: Ada\n");
    let (code, value) = command(
        &[
            "record",
            "create",
            "--file",
            "-",
            "--expected-fingerprint",
            &fingerprint,
            "--repo",
            fixture.root.to_str().unwrap(),
            "--compact",
        ],
        Some(&draft),
    );
    assert_eq!(code, 0, "{value}");
    assert!(value["result"]["afterSha256"].is_string());
    assert_eq!(fixture.fingerprint(), fingerprint);
    let file = fixture.request(&"a".repeat(1_048_577));
    let (code, error) = fixture.call(&[
        "record",
        "create",
        "--file",
        file.to_str().unwrap(),
        "--expected-fingerprint",
        &fingerprint,
    ]);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "usage_error");
    assert_eq!(error["message"], "input exceeds 1 MiB");
    assert_eq!(fixture.fingerprint(), fingerprint);
}
