use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
use wedl::{
    Error,
    consequence_semantics::Time,
    revision_review::{self, Request},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const NEW: &str = "obj_00000000000000000000000002";
const KNOW: &str = "know_00000000000000000000000001";
const KNOW2: &str = "know_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENES: [&str; 3] = [
    "scene_00000000000000000000000001",
    "scene_00000000000000000000000002",
    "scene_00000000000000000000000003",
];
const EVENT: &str = "event_00000000000000000000000001";
const EVENT2: &str = "event_00000000000000000000000002";
const CONCEPT: &str = "concept_00000000000000000000000001";
const FUTURE: &str = "concept_00000000000000000000000002";
const BRIEF: &str = "brief_00000000000000000000000001";

struct Fixture {
    directory: TempDir,
}
impl Fixture {
    fn new() -> Self {
        let result = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        fs::create_dir(result.root().join("story")).unwrap();
        result.git(&["init", "-q"]);
        result.git(&["config", "user.name", "Revision fixture"]);
        result.git(&["config", "user.email", "revision@example.invalid"]);
        result.record("world", WORLD, "World", "capabilities: [lore-core-v1, descriptions-v1, briefs-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nthreads: []\nstate_keys:\n  character:\n    score: {type: integer}\n    number: {type: number}\n    labels: {type: string-set}\nrelationship_metrics: {}\n", "World prose.\n");
        result.record(
            "character",
            ANA,
            "Ana",
            "initial_state: {score: 0, number: 0, labels: [a, b]}\n",
            "Original body.\n",
        );
        result.record("object", OBJECT, "Object", "", "Original object.\n");
        result
    }
    fn root(&self) -> &Path {
        self.directory.path()
    }
    fn path(&self, id: &str) -> PathBuf {
        self.root().join(format!("story/{id}.md"))
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("--no-replace-objects")
            .arg("-C")
            .arg(self.root())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    fn record(&self, kind: &str, id: &str, title: &str, fields: &str, body: &str) {
        fs::write(
            self.path(id),
            format!(
                "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {title}\n{fields}---\n{body}"
            ),
        )
        .unwrap();
    }
    fn edit(&self, id: &str, f: impl FnOnce(&mut Yaml, &mut String)) {
        let path = self.path(id);
        let record =
            source::parse_record(&fs::read(&path).unwrap(), &format!("story/{id}.md")).unwrap();
        let mut frontmatter = record.frontmatter;
        let mut body = record.body;
        f(&mut frontmatter, &mut body);
        fs::write(
            path,
            format!(
                "---\n{}---\n{body}",
                serde_yaml::to_string(&frontmatter).unwrap()
            ),
        )
        .unwrap();
    }
    fn commit(&self) -> String {
        let world = source::load_world(self.root(), None).unwrap();
        let report = validation::validate_world(&world);
        assert!(report.valid, "fixture invalid: {:?}", report.diagnostics);
        self.git(&["add", "story"]);
        self.git(&["commit", "-qm", "revision fixture"]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn branch(&self, base: &str, change: impl FnOnce(&Self)) -> String {
        self.git(&["checkout", "-q", "--detach", base]);
        change(self);
        self.commit()
    }
    fn request(&self, base: &str, left: &str, right: &str) -> Request {
        Request {
            base_revision: base.into(),
            left_revision: left.into(),
            right_revision: right.into(),
            at: Time {
                timeline: "main".into(),
                tick: "10".into(),
                order: "0".into(),
            },
            max_items: 1000,
            max_characters: 1_000_000,
        }
    }
    fn review(&self, base: &str, left: &str, right: &str) -> Json {
        revision_review::review(self.root(), &self.request(base, left, right)).unwrap()
    }
    fn knowledge(&self, id: &str) {
        self.record("knowledge", id, "Knowledge", &format!("knower: {ANA}\nclaim: {{key: fixture, statement: An authored claim., truth: unknown}}\ntransitions:\n  - {{id: shared, time: {{tick: 1}}, state: uncertain, acquisition: heard, confidence: 0.5}}\n  - {{id: future, time: {{tick: 20}}, state: rejected, acquisition: heard, confidence: 0.5}}\n"), "");
    }
    fn scenes(&self) {
        self.record("location", PLACE, "Place", "", "");
        for (index, id) in SCENES.iter().enumerate() {
            self.record("scene", id, "Scene", &format!("status: {}\nlocation: {PLACE}\ntime: {{start: {{tick: 1}}, current: {{tick: 10}}}}\nparticipants: [{{character: {ANA}}}]\n", if index==0 {"active"} else {"planned"}), "");
        }
    }
    fn descriptions(&self, values: Vec<Yaml>) {
        self.edit(ANA, |front, _| {
            front["descriptions"] = Yaml::Sequence(values)
        });
    }
}

fn y(value: Json) -> Yaml {
    serde_yaml::to_value(value).unwrap()
}
fn description(index: usize, markdown: &str, future: bool) -> Yaml {
    y(
        json!({"id":format!("description_{index:026}"),"heading":format!("Section {index}"),"markdown":markdown,
        "valid":if future {json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":20,"order":0}})} else {json!({"kind":"timeless"})},
        "audience":["author","public"],"perspectives":["author","ordinary"]}),
    )
}
fn conflicts(report: &Json, kind: &str) -> Vec<Json> {
    report["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|value| value["kind"] == kind)
        .cloned()
        .collect()
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    result
}
fn assert_original_citations(fixture: &Fixture, report: &Json, pins: &[String]) {
    let mut pending = vec![report];
    let mut count = 0;
    while let Some(value) = pending.pop() {
        match value {
            Json::Object(fields) => {
                if fields.contains_key("sourcePath") && fields.contains_key("blobOid") {
                    let revision = value["revision"].as_str().unwrap();
                    assert!(pins.iter().any(|pin| pin == revision));
                    let blob = fixture.git(&[
                        "rev-parse",
                        &format!("{revision}:{}", value["sourcePath"].as_str().unwrap()),
                    ]);
                    assert_eq!(value["blobOid"], blob);
                    count += 1;
                }
                pending.extend(fields.values());
            }
            Json::Array(values) => pending.extend(values),
            _ => {}
        }
    }
    assert!(count > 5);
}

#[test]
fn closed_request_keeps_literal_pins_exact_time_and_complete_budget() {
    let raw = json!({"baseRevision":"a".repeat(40),"leftRevision":"b".repeat(40),"rightRevision":"c".repeat(40),
        "at":{"timeline":"main","tick":"9007199254740993","order":"-2147483648"},"maxItems":1000,"maxCharacters":1_000_000});
    let decoded = revision_review::decode(&raw.to_string()).unwrap();
    assert_eq!(decoded.at.tick, "9007199254740993");
    for (field, value) in [
        ("extra", json!(true)),
        ("baseRevision", json!("HEAD")),
        ("leftRevision", json!("A".repeat(40))),
        ("maxItems", json!(0)),
        ("maxCharacters", json!(1_000_001)),
    ] {
        let mut invalid = raw.clone();
        invalid[field] = value;
        assert!(revision_review::decode(&invalid.to_string()).is_err());
    }
    for value in [
        json!(9007199254740993u64),
        json!("01"),
        json!("-0"),
        json!("9223372036854775808"),
    ] {
        let mut invalid = raw.clone();
        invalid["at"]["tick"] = value;
        assert!(revision_review::decode(&invalid.to_string()).is_err());
    }
    let duplicate = raw.to_string().replacen('{', "{\"maxItems\":1,", 1);
    assert!(revision_review::decode(&duplicate).is_err());
    let duplicate_time = raw.to_string().replace(
        "\"timeline\":\"main\"",
        "\"timeline\":\"main\",\"timeline\":\"main\"",
    );
    assert!(revision_review::decode(&duplicate_time).is_err());
}

#[test]
fn real_branching_git_review_reports_five_conflict_families_and_never_writes() {
    let fixture = Fixture::new();
    fixture.knowledge(KNOW);
    fixture.scenes();
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, body| {
            front["initial_state"]["score"] = y(json!(1));
            *body = "Left source.\r\n".into();
        });
        f.edit(KNOW, |front, _| {
            front["transitions"][0]["state"] = y(json!("accepted"))
        });
        f.edit(SCENES[0], |front, _| front["status"] = y(json!("planned")));
        f.edit(SCENES[1], |front, _| front["status"] = y(json!("active")));
        f.record("object", NEW, "Left identity", "", "Left introduction.\n");
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, body| {
            front["initial_state"]["score"] = y(json!(2));
            *body = "Right source.\n".into();
        });
        f.edit(KNOW, |front, _| {
            front["transitions"][0]["state"] = y(json!("suspected"))
        });
        f.edit(SCENES[0], |front, _| front["status"] = y(json!("planned")));
        f.edit(SCENES[2], |front, _| front["status"] = y(json!("active")));
        f.record("object", NEW, "Right identity", "", "Right introduction.\n");
    });
    fs::write(
        fixture.path(WORLD),
        "invalid live worktree; immutable review must never parse this",
    )
    .unwrap();
    fs::create_dir(fixture.root().join("private")).unwrap();
    fs::write(
        fixture.root().join("private/draft.md"),
        "unrelated private bytes",
    )
    .unwrap();
    fs::create_dir(fixture.root().join(".wedl-rust")).unwrap();
    fs::write(
        fixture.root().join(".wedl-rust/cache.sqlite"),
        "untouched cache",
    )
    .unwrap();
    let before = files(fixture.root());
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report, fixture.review(&base, &left, &right));
    let kinds: BTreeSet<_> = report["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "active-scene",
            "divergent-transition",
            "duplicate-id",
            "state",
            "text"
        ])
    );
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["canApply"], false);
    assert_eq!(report["validationComplete"], false);
    assert_eq!(conflicts(&report, "active-scene").len(), 2);
    let body = conflicts(&report, "text")
        .into_iter()
        .find(|v| v["position"]["section"] == "bodyMarkdown" && v["id"] == ANA)
        .unwrap();
    assert_eq!(body["left"]["text"], "Left source.\r\n");
    assert_original_citations(&fixture, &report, &[base, left, right]);
    assert_eq!(files(fixture.root()), before);
}

#[test]
fn moves_are_distinct_and_delete_modify_keeps_original_absence_evidence() {
    let fixture = Fixture::new();
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        fs::rename(f.path(ANA), f.root().join("story/moved-character.md")).unwrap();
        fs::remove_file(f.path(OBJECT)).unwrap();
    });
    let right = fixture.branch(&base, |f| {
        f.edit(OBJECT, |_, body| *body = "Changed after base.\n".into())
    });
    let report = fixture.review(&base, &left, &right);
    let row = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == ANA)
        .unwrap();
    assert_eq!(row["leftChange"], "moved");
    assert_eq!(row["rightChange"], "none");
    assert_eq!(row["positions"], json!([]));
    assert_eq!(
        row["left"]["citation"]["sourcePath"],
        "story/moved-character.md"
    );
    let text = conflicts(&report, "text")
        .into_iter()
        .find(|v| v["id"] == OBJECT)
        .unwrap();
    assert_eq!(text["left"]["present"], false);
    assert_eq!(text["left"]["citation"], Json::Null);
    assert_eq!(text["base"]["text"], "Original object.\n");
}

#[test]
fn identical_introductions_converge_and_member_identity_is_owner_local() {
    let fixture = Fixture::new();
    fixture.knowledge(KNOW);
    fixture.knowledge(KNOW2);
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.record("object", NEW, "Same introduction", "", "Same.\n");
        f.edit(KNOW, |front, _| {
            front["transitions"][0]["state"] = y(json!("accepted"))
        });
    });
    let right = fixture.branch(&base, |f| {
        f.record("object", NEW, "Same introduction", "", "Same.\n");
        f.edit(KNOW2, |front, _| {
            front["transitions"][0]["state"] = y(json!("suspected"))
        });
    });
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report["conflicts"], json!([]));
    let new = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == NEW)
        .unwrap();
    assert_eq!(new["leftChange"], "added");
    assert_eq!(new["rightChange"], "added");
}

#[test]
fn future_members_do_not_shift_identity_and_evidence_retains_original_ordinal() {
    let fixture = Fixture::new();
    fixture.descriptions(vec![description(1, "Original", false)]);
    fixture.knowledge(KNOW);
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.descriptions(vec![
            description(9, "FUTURE LEFT SECRET", true),
            description(1, "Left", false),
        ]);
        f.edit(KNOW, |front, _| {
            front["transitions"][1]["state"] = y(json!("accepted"))
        });
    });
    let right = fixture.branch(&base, |f| {
        f.descriptions(vec![
            description(1, "Right", false),
            description(9, "FUTURE RIGHT SECRET", true),
        ]);
        f.edit(KNOW, |front, _| {
            front["transitions"][1]["state"] = y(json!("suspected"))
        });
    });
    let report = fixture.review(&base, &left, &right);
    let encoded = report.to_string();
    assert!(!encoded.contains("FUTURE LEFT SECRET"));
    assert!(!encoded.contains("FUTURE RIGHT SECRET"));
    assert!(
        !report["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["id"] == KNOW)
    );
    let text = conflicts(&report, "text")
        .into_iter()
        .find(|v| v["position"]["section"] == "descriptions")
        .unwrap();
    assert_eq!(text["left"]["citation"]["sourceOrdinal"], 1);
    assert_eq!(text["right"]["citation"]["sourceOrdinal"], 0);
    assert_eq!(
        text["left"]["citation"]["memberId"],
        format!("description_{:026}", 1)
    );
}

#[test]
fn shared_member_order_ignores_independent_insertions_but_detects_divergent_reorder() {
    let fixture = Fixture::new();
    let original = vec![
        description(1, "One", false),
        description(2, "Two", false),
        description(3, "Three", false),
    ];
    fixture.descriptions(original.clone());
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.descriptions(vec![
            original[0].clone(),
            description(4, "Left insert", false),
            original[1].clone(),
            original[2].clone(),
        ])
    });
    let right = fixture.branch(&base, |f| {
        f.descriptions(vec![
            original[0].clone(),
            original[1].clone(),
            description(5, "Right insert", false),
            original[2].clone(),
        ])
    });
    assert_eq!(fixture.review(&base, &left, &right)["conflicts"], json!([]));
    let left = fixture.branch(&base, |f| {
        f.descriptions(vec![
            original[1].clone(),
            original[0].clone(),
            original[2].clone(),
        ])
    });
    let right = fixture.branch(&base, |f| {
        f.descriptions(vec![
            original[0].clone(),
            original[2].clone(),
            original[1].clone(),
        ])
    });
    let report = fixture.review(&base, &left, &right);
    let text = conflicts(&report, "text");
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["position"]["sourceOrdinal"], Json::Null);
    assert!(
        text[0]["left"]["text"]
            .as_str()
            .unwrap()
            .contains("\"markdown\":\"Two\"")
    );
}

#[test]
fn exact_large_integers_and_numeric_kinds_remain_distinct() {
    let fixture = Fixture::new();
    fixture.edit(ANA, |front, _| {
        front["initial_state"]["score"] = y(json!(9007199254740993i64))
    });
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["score"] = y(json!(9007199254740994i64))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["score"] = y(json!(9007199254740995i64))
        })
    });
    let report = fixture.review(&base, &left, &right);
    let state = conflicts(&report, "state");
    assert_eq!(state.len(), 1);
    assert_eq!(state[0]["base"]["payload"]["valueRaw"], "9007199254740993");
    assert_eq!(state[0]["left"]["payload"]["valueRaw"], "9007199254740994");
    fixture.git(&["checkout", "-q", "--detach", &base]);
    fixture.edit(ANA, |front, _| {
        front["initial_state"]["number"] = y(json!(1))
    });
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["number"] = y(json!(1.0))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["number"] = y(json!(2))
        })
    });
    let report = fixture.review(&base, &left, &right);
    let state = conflicts(&report, "state");
    assert_eq!(state.len(), 1);
    assert_eq!(state[0]["base"]["payload"]["valueRaw"], "1");
    assert_eq!(state[0]["left"]["payload"]["valueRaw"], "1.0");
}

#[test]
fn equal_state_values_suppress_provenance_and_set_order_conflicts() {
    let fixture = Fixture::new();
    let base = fixture.commit();
    let event = |f: &Fixture, id: &str| {
        f.record("event",id,"State event",&format!("time: {{tick: 5}}\neffects:\n  - {{id: effect_00000000000000000000000001, target: {ANA}, key: score, operation: set, value: 9}}\n"),"")
    };
    let left = fixture.branch(&base, |f| {
        event(f, EVENT);
        f.edit(ANA, |front, _| {
            front["initial_state"]["labels"] = y(json!(["b", "a"]))
        });
    });
    let right = fixture.branch(&base, |f| {
        event(f, EVENT2);
        f.edit(ANA, |front, _| {
            front["initial_state"]["labels"] = y(json!(["a", "b"]))
        });
    });
    let report = fixture.review(&base, &left, &right);
    assert!(conflicts(&report, "state").is_empty());
    assert_eq!(report["conflicts"], json!([]));
}

#[test]
fn split_active_fronts_do_not_imply_global_singleton_selection() {
    let fixture = Fixture::new();
    fixture.scenes();
    let other = "char_00000000000000000000000002";
    fixture.record("character", other, "Other character", "", "");
    fixture.edit(WORLD, |front, _| {
        front["current_time"] = y(json!({"timeline":"main","tick":10,"order":0}))
    });
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(SCENES[0], |front, _| {
            front["participants"][0]["character"] = y(json!(other))
        });
        f.edit(SCENES[1], |front, _| front["status"] = y(json!("active")));
    });
    let right = fixture.branch(&base, |f| {
        f.edit(SCENES[0], |front, _| front["status"] = y(json!("planned")));
        f.edit(SCENES[2], |front, _| front["status"] = y(json!("active")));
    });
    let report = fixture.review(&base, &left, &right);
    let active = conflicts(&report, "active-scene");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["scope"], "participation");
}

#[test]
fn native_author_scope_excludes_future_typed_targets_but_retains_opaque_words() {
    let fixture = Fixture::new();
    fixture.edit(WORLD,|front,_|front["profiles"]=y(json!([{"id":"notes","label":"Notes","fields":{"type":"object","required":{"note":{"type":"string"}},"optional":{"target":{"type":"entity","kinds":["lore-concept"]}}}}])));
    fixture.record("lore-concept",FUTURE,"Future concept","domain: test.future\nprofile: notes\nvalues: {note: Future note}\nstatus: canonical\nvalid: {kind: interval, timeline: main, start: {timeline: main, tick: 20, order: 0}}\naudience: [author]\nperspectives: [author]\n","");
    fixture.record("lore-concept",CONCEPT,"Current concept",&format!("domain: test.current\nprofile: notes\nvalues: {{note: '{FUTURE}', target: {{kind: record, id: {FUTURE}}}}}\nstatus: canonical\nvalid: {{kind: timeless}}\naudience: [author]\nperspectives: [author]\n"),"");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(CONCEPT, |front, _| {
            front["values"]["note"] = y(json!(format!("Left literal {FUTURE}")))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(CONCEPT, |front, _| {
            front["values"]["note"] = y(json!(format!("Right literal {FUTURE}")))
        })
    });
    let report = fixture.review(&base, &left, &right);
    let text = conflicts(&report, "text");
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["position"]["section"], "frontmatter.values.note");
    assert!(text[0]["base"]["text"].as_str().unwrap().contains(FUTURE));
    assert!(
        !report["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == FUTURE)
    );
}

#[test]
fn complete_native_brief_closure_is_reused_and_unavailable_closure_is_explicit() {
    let fixture = Fixture::new();
    fixture.record("context-brief",BRIEF,"Brief",&format!("domain: test.brief\nstatus: canonical\nvalid: {{kind: timeless}}\naudience: [author, public]\nperspectives: [author, ordinary]\nscope: world\nrequired_refs: [{{kind: record, id: {ANA}}}]\n"),"Brief body.\n");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(BRIEF, |_, body| *body = "Left brief.\n".into())
    });
    let right = fixture.branch(&base, |f| {
        f.edit(BRIEF, |_, body| *body = "Right brief.\n".into())
    });
    assert_eq!(
        conflicts(&fixture.review(&base, &left, &right), "text").len(),
        1
    );
    fixture.git(&["checkout", "-q", "--detach", &base]);
    fixture.descriptions(vec![description(1, "Future pin", true)]);
    fixture.edit(BRIEF, |front, _| {
        front["required_refs"] =
            y(json!([{"kind":"section","owner_id":ANA,"id":format!("description_{:026}",1)}]))
    });
    let unavailable = fixture.commit();
    let report = fixture.review(&unavailable, &unavailable, &unavailable);
    assert_eq!(report["changes"], json!([]));
    assert!(matches!(
        revision_review::review(fixture.root(), &fixture.request(&base, &unavailable, &base)),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn unavailable_owner_is_not_reported_as_deletion_and_ancestry_is_required() {
    let fixture = Fixture::new();
    fixture.record(
        "event",
        EVENT,
        "Event",
        "time: {tick: 5}\neffects: []\n",
        "",
    );
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(EVENT, |front, _| front["time"]["tick"] = y(json!(20)))
    });
    let right = fixture.branch(&base, |f| f.edit(ANA, |_, body| *body = "Right.\n".into()));
    assert!(matches!(
        revision_review::review(fixture.root(), &fixture.request(&base, &left, &right)),
        Err(Error::NotFound(_))
    ));
    assert!(
        revision_review::review(fixture.root(), &fixture.request(&left, &base, &right))
            .unwrap_err()
            .to_string()
            .contains("ancestor")
    );
    let mut request = fixture.request(&base, &left, &right);
    request.left_revision = fixture.git(&["rev-parse", &format!("{left}^{{tree}}")]);
    assert!(revision_review::review(fixture.root(), &request).is_err());
}

#[test]
fn duplicate_original_ids_are_rejected_before_id_matching() {
    let fixture = Fixture::new();
    let base = fixture.commit();
    fs::copy(fixture.path(ANA), fixture.root().join("story/duplicate.md")).unwrap();
    fixture.git(&["add", "story"]);
    fixture.git(&["commit", "-qm", "invalid duplicate source"]);
    let invalid = fixture.git(&["rev-parse", "HEAD"]);
    let error = revision_review::review(fixture.root(), &fixture.request(&base, &invalid, &base))
        .unwrap_err();
    assert!(error.to_string().contains("duplicate record ID"));
}

#[test]
fn complete_result_bounds_refuse_without_truncation_and_hash_binds_budget() {
    let fixture = Fixture::new();
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| f.edit(ANA, |_, body| *body = "Left.\n".into()));
    let right = fixture.branch(&base, |f| f.edit(ANA, |_, body| *body = "Right.\n".into()));
    let mut request = fixture.request(&base, &left, &right);
    let before = files(fixture.root());
    request.max_items = 1;
    assert!(
        revision_review::review(fixture.root(), &request)
            .unwrap_err()
            .to_string()
            .contains("maxItems")
    );
    request.max_items = 1000;
    request.max_characters = 256;
    assert!(
        revision_review::review(fixture.root(), &request)
            .unwrap_err()
            .to_string()
            .contains("maxCharacters")
    );
    request.max_characters = 1_000_000;
    let full = revision_review::review(fixture.root(), &request).unwrap();
    request.max_characters = 999_999;
    let other = revision_review::review(fixture.root(), &request).unwrap();
    assert_ne!(full["requestSha256"], other["requestSha256"]);
    assert_ne!(full["reportSha256"], other["reportSha256"]);
    assert_eq!(full["changes"], other["changes"]);
    assert_eq!(full["conflicts"], other["conflicts"]);
    assert_eq!(files(fixture.root()), before);
}

#[test]
fn complete_source_position_bound_is_checked_even_when_revisions_are_equal() {
    let fixture = Fixture::new();
    fixture.edit(WORLD, |front, _| {
        front["tags"] = y(json!(
            (0..4097).map(|i| format!("tag{i}")).collect::<Vec<_>>()
        ))
    });
    let base = fixture.commit();
    let error =
        revision_review::review(fixture.root(), &fixture.request(&base, &base, &base)).unwrap_err();
    assert!(error.to_string().contains("positions"), "{error}");
}

#[test]
fn author_union_admits_independently_visible_existence_endpoints() {
    let fixture = Fixture::new();
    fixture.edit(WORLD, |front, _| {
        front["capabilities"] = y(json!([
            "generational-core-v1",
            "lore-core-v1",
            "descriptions-v1",
            "existence-v1",
            "briefs-v1"
        ]))
    });
    fixture.edit(ANA, |front, _| {
        front["audience"] = y(json!(["public"]));
        front["perspectives"] = y(json!(["ordinary"]));
    });
    let episode = "existence_00000000000000000000000001";
    fixture.record("existence-episode",episode,"Existence",&format!("domain: test.existence\ncharacter_id: {ANA}\nepisode_kind: unknown\nvalid: {{kind: interval, timeline: main}}\nstart_evidence: []\naudience: [author]\nperspectives: [author]\n"),"Base episode.\n");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(episode, |_, body| *body = "Left episode.\n".into())
    });
    let right = fixture.branch(&base, |f| {
        f.edit(episode, |_, body| *body = "Right episode.\n".into())
    });
    let report = fixture.review(&base, &left, &right);
    let text = conflicts(&report, "text");
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["id"], episode);
}

#[test]
fn brief_closure_follows_assertion_endpoint_withholding_to_a_fixed_point() {
    let fixture = Fixture::new();
    fixture.edit(WORLD, |front, _| {
        front["capabilities"] = y(json!([
            "lore-core-v1",
            "descriptions-v1",
            "assertions-v1",
            "briefs-v1"
        ]));
        front["predicates"] =
            y(json!([{"id":"account","subject_kinds":["event"],"value_type":{"type":"string"}}]));
    });
    fixture.record(
        "event",
        EVENT,
        "Future event",
        "time: {tick: 20}\neffects: []\n",
        "",
    );
    let assertion = "assertion_00000000000000000000000001";
    fixture.record("assertion",assertion,"Assertion",&format!("domain: test.assertion\nsubject: {{kind: record, id: {EVENT}}}\npredicate: account\nvalue: Opaque words\nclassification: author-fact\nvalid: {{kind: timeless}}\nevidence: []\naudience: [author]\nperspectives: [author]\n"),"Base assertion.\n");
    fixture.record("context-brief",BRIEF,"Brief",&format!("domain: test.brief\nvalid: {{kind: timeless}}\naudience: [author]\nperspectives: [author]\nscope: world\nrequired_refs: [{{kind: record, id: {assertion}}}]\n"),"Base brief.\n");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(BRIEF, |_, body| *body = "Withheld left brief.\n".into())
    });
    let right = fixture.branch(&base, |f| {
        f.edit(assertion, |_, body| {
            *body = "Withheld right assertion.\n".into()
        })
    });
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report["changes"], json!([]));
    assert_eq!(report["conflicts"], json!([]));
}

#[test]
fn native_single_lens_brief_union_gap_is_explicitly_unsupported() {
    // The former unsupported union now composes one complete author closure.
    let fixture = Fixture::new();
    fixture.edit(ANA, |front, _| front["audience"] = y(json!(["author"])));
    fixture.record("context-brief",BRIEF,"Brief",&format!("domain: test.brief\nvalid: {{kind: timeless}}\naudience: [public]\nperspectives: [author]\nscope: world\nrequired_refs: [{{kind: record, id: {ANA}}}]\n"),"");
    let base = fixture.commit();
    let report =
        revision_review::review(fixture.root(), &fixture.request(&base, &base, &base)).unwrap();
    assert_eq!(report["changes"], json!([]));
    assert_eq!(report["conflicts"], json!([]));
}

#[test]
fn source_path_expansion_is_bounded_before_reference_indexing() {
    let fixture = Fixture::new();
    fixture.edit(OBJECT, |front, _| front["x".repeat(4097)] = y(json!(1)));
    let base = fixture.commit();
    let error =
        revision_review::review(fixture.root(), &fixture.request(&base, &base, &base)).unwrap_err();
    assert!(error.to_string().contains("source paths"), "{error}");
}

#[test]
fn profile_bound_state_keeps_booleans_numbers_and_strings_distinct() {
    let fixture = Fixture::new();
    fixture.edit(WORLD,|front,_|{
        front["state_keys"]["character"]["bundle"]=y(json!({"type":"string"}));
        front["state_profile_bindings"]=y(json!({"character":{"bundle":"bundle"}}));
        front["profiles"]=y(json!([{"id":"bundle","label":"Typed bundle","fields":{"type":"object","required":{"choice":{"type":"enum","values":[1,true,"1"]}},"optional":{}}}]));
    });
    fixture.edit(ANA, |front, _| {
        front["initial_state"]["bundle"] = y(json!({"choice":1}))
    });
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["bundle"]["choice"] = y(json!(true))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["initial_state"]["bundle"]["choice"] = y(json!("1"))
        })
    });
    let report = fixture.review(&base, &left, &right);
    let state = conflicts(&report, "state");
    assert_eq!(state.len(), 1);
    assert_eq!(state[0]["base"]["payload"]["valueRaw"], "{\"choice\":1}");
    assert_eq!(state[0]["left"]["payload"]["valueRaw"], "{\"choice\":true}");
    assert_eq!(
        state[0]["right"]["payload"]["valueRaw"],
        "{\"choice\":\"1\"}"
    );
}

#[test]
fn applicable_description_correction_history_retains_predecessor_source() {
    let fixture = Fixture::new();
    let original = description(1, "Original account", false);
    let mut correction = description(2, "Corrected account", false);
    correction["replaces"] = y(json!(format!("description_{:026}", 1)));
    fixture.descriptions(vec![original, correction]);
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["descriptions"][0]["markdown"] = y(json!("Left predecessor words"))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, _| {
            front["descriptions"][0]["markdown"] = y(json!("Right predecessor words"))
        })
    });
    let report = fixture.review(&base, &left, &right);
    let text = conflicts(&report, "text");
    assert_eq!(text.len(), 1);
    assert_eq!(
        text[0]["position"]["memberId"],
        format!("description_{:026}", 1)
    );
    assert!(
        text[0]["left"]["text"]
            .as_str()
            .unwrap()
            .contains("Left predecessor words")
    );
}

#[test]
fn native_generational_initialization_uses_transition_identity_and_future_members_are_excluded() {
    let fixture = Fixture::new();
    fixture.edit(WORLD, |front, _| {
        front["capabilities"] = y(json!([
            "generational-core-v1",
            "lore-core-v1",
            "descriptions-v1",
            "briefs-v1"
        ]))
    });
    let parent = "char_00000000000000000000000002";
    let parentage = "kinship_00000000000000000000000001";
    fixture.record("character", parent, "Parent", "", "");
    fixture.record("parentage",parentage,"Parentage",&format!("child_id: {ANA}\nparent_id: {parent}\ninitialization:\n  transition_id: transition_00000000000000000000000001\n  transition_kind: parentage-initialize\n  applicability: {{applicability_kind: instant, point: {{timeline: main, tick: 1, order: 0}}}}\n  payload: {{basis: biological}}\ntransitions:\n  - transition_id: transition_00000000000000000000000002\n    transition_kind: parentage-confirm\n    applicability: {{applicability_kind: instant, point: {{timeline: main, tick: 20, order: 0}}}}\n    payload: {{basis: biological}}\n"),"");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(parentage, |front, _| {
            front["initialization"]["payload"]["basis"] = y(json!("adoptive"));
            front["transitions"][0]["payload"]["basis"] = y(json!("adoptive"));
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(parentage, |front, _| {
            front["initialization"]["applicability"]["point"]["tick"] = y(json!(2));
            front["transitions"][0]["applicability"]["point"]["tick"] = y(json!(30));
        })
    });
    let report = fixture.review(&base, &left, &right);
    let transitions = conflicts(&report, "divergent-transition");
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0]["ownerId"], parentage);
    assert_eq!(
        transitions[0]["member"]["id"],
        "transition_00000000000000000000000001"
    );
    assert!(
        !report
            .to_string()
            .contains("transition_00000000000000000000000002")
    );
}

#[test]
fn original_author_state_scope_is_independent_of_narrative_owner_labels() {
    let fixture = Fixture::new();
    fixture.edit(ANA, |front, _| front["audience"] = y(json!(["gm"])));
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(ANA, |front, body| {
            front["initial_state"]["score"] = y(json!(1));
            *body = "WITHHELD LEFT NARRATIVE".into();
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(ANA, |front, body| {
            front["initial_state"]["score"] = y(json!(2));
            *body = "WITHHELD RIGHT NARRATIVE".into();
        })
    });
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report["changes"], json!([]));
    assert_eq!(conflicts(&report, "state").len(), 1);
    assert!(!report.to_string().contains("WITHHELD LEFT NARRATIVE"));
    assert!(!report.to_string().contains("WITHHELD RIGHT NARRATIVE"));
    assert_eq!(
        report["semantics"]["stateScope"],
        "complete-original-author-worlds"
    );
}

#[test]
fn original_author_state_replay_keeps_canonical_effects_despite_narrative_labels() {
    let fixture = Fixture::new();
    fixture.record("event",EVENT,"Narratively withheld event",&format!("audience: [gm]\ntime: {{tick: 5}}\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: score, operation: set, value: 0}}]\n"),"");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(EVENT, |front, _| front["effects"][0]["value"] = y(json!(1)))
    });
    let right = fixture.branch(&base, |f| {
        f.edit(EVENT, |front, _| front["effects"][0]["value"] = y(json!(2)))
    });
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report["changes"], json!([]));
    let state = conflicts(&report, "state");
    assert_eq!(state.len(), 1);
    assert_eq!(state[0]["left"]["payload"]["valueRaw"], "1");
    assert_eq!(state[0]["left"]["citations"][0]["revision"], left);
    assert_eq!(state[0]["right"]["citations"][0]["revision"], right);
}

#[test]
fn future_entity_operand_policy_is_explicit_native_state_not_source_visibility() {
    let fixture = Fixture::new();
    fixture.edit(WORLD, |front, _| {
        front["state_keys"]["character"]["destination"] =
            y(json!({"type":"entity","kind":"environment"}))
    });
    let targets = [
        "env_00000000000000000000000001",
        "env_00000000000000000000000002",
        "env_00000000000000000000000003",
    ];
    for id in targets {
        fixture.record(
            "environment",
            id,
            "Future environment",
            &format!("targets: [{ANA}]\ntime: {{start: {{tick: 20}}, end: {{tick: 30}}}}\n"),
            "FUTURE ENVIRONMENT NARRATIVE",
        );
    }
    fixture.record("event",EVENT,"Current event",&format!("time: {{tick: 5}}\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: destination, operation: set, value: {{entity: {}}}}}]\n",targets[0]),"");
    let base = fixture.commit();
    let left = fixture.branch(&base, |f| {
        f.edit(EVENT, |front, _| {
            front["effects"][0]["value"]["entity"] = y(json!(targets[1]))
        })
    });
    let right = fixture.branch(&base, |f| {
        f.edit(EVENT, |front, _| {
            front["effects"][0]["value"]["entity"] = y(json!(targets[2]))
        })
    });
    let report = fixture.review(&base, &left, &right);
    assert_eq!(report["changes"], json!([]));
    // All three explicit entity operands are beyond this horizon. Their
    // authored differences cannot create a semantic conflict before replay.
    assert!(conflicts(&report, "state").is_empty());
    assert_eq!(report["semantics"]["futureEntityTargetPruning"], true);
    assert_eq!(
        report["semantics"]["stateReferences"],
        "native-values-with-scalar-entity-effect-pruning"
    );
    for target in targets {
        assert!(!report.to_string().contains(target));
    }
    assert!(!report.to_string().contains("FUTURE ENVIRONMENT NARRATIVE"));
}
