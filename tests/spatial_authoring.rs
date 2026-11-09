//! Typed spatial drafts use the validated one-record publication boundary.
use std::{fs, path::PathBuf};

use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use wedl::{Error, authoring, model::World, record_edit, source, spatial, validation};

const A: &str = "loc_00000000000000000000000001";
const B: &str = "loc_00000000000000000000000002";
const C: &str = "loc_00000000000000000000000003";
const RETIRED: &str = "loc_00000000000000000000000004";
const MISSING: &str = "loc_00000000000000000000000009";
const ROUTE: &str = "route_00000000000000000000000001";

struct Fixture {
    _directory: tempfile::TempDir,
    world: World,
}

impl Fixture {
    fn new(capabilities: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut world = source::init_world(&directory.path().join("world"), "Routes").unwrap();
        if capabilities {
            let path = world.root.join("story/world.md");
            let source = fs::read_to_string(&path).unwrap();
            assert!(source.contains("capabilities: []"));
            fs::write(
                path,
                source.replace(
                    "capabilities: []",
                    "capabilities: [spatial-core-v1, route-v1]",
                ),
            )
            .unwrap();
            world = source::load_world(&world.root, None).unwrap();
        }
        for (id, title) in [
            (A, "Harbor"),
            (B, "Island"),
            (C, "Town"),
            (RETIRED, "Old Dock"),
        ] {
            let record = record_edit::create(
                &world,
                &format!("kind: location\nid: {id}\ntitle: {title}\n"),
            )
            .unwrap();
            let path = world.root.join(&record.source_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let source = String::from_utf8(record.raw_bytes).unwrap();
            fs::write(
                path,
                if id == RETIRED {
                    source.replace("status: canonical", "status: retired")
                } else {
                    source
                },
            )
            .unwrap();
            world = source::load_world(&world.root, None).unwrap();
        }
        let report = validation::validate_world(&world);
        assert!(report.valid, "{:?}", report.diagnostics);
        Self {
            _directory: directory,
            world,
        }
    }

    fn unchanged(&self) {
        assert_eq!(
            source::load_world(&self.world.root, None)
                .unwrap()
                .fingerprint,
            self.world.fingerprint
        );
    }

    fn create(&mut self, request: &str) -> Json {
        let preview =
            authoring::preview_record_create(&self.world, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], true, "{preview}");
        self.unchanged();
        authoring::apply_record_create(
            &self.world,
            request,
            &self.world.fingerprint,
            preview["afterSha256"].as_str().unwrap(),
        )
        .unwrap();
        self.world = source::load_world(&self.world.root, None).unwrap();
        assert_eq!(preview["candidateFingerprint"], self.world.fingerprint);
        assert_eq!(
            self.world.records[preview["entityId"].as_str().unwrap()].raw_bytes,
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
        preview
    }

    fn update(&mut self, id: &str, request: &str) -> Json {
        let preview =
            authoring::preview_record_update(&self.world, id, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], true, "{preview}");
        self.unchanged();
        authoring::apply_record_update(
            &self.world,
            id,
            request,
            &self.world.fingerprint,
            preview["afterSha256"].as_str().unwrap(),
        )
        .unwrap();
        self.world = source::load_world(&self.world.root, None).unwrap();
        assert_eq!(preview["candidateFingerprint"], self.world.fingerprint);
        preview
    }

    fn refused_create(&self, request: &str) -> Json {
        let preview =
            authoring::preview_record_create(&self.world, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], false, "{request}: {preview}");
        assert!(matches!(
            authoring::apply_record_create(
                &self.world,
                request,
                &self.world.fingerprint,
                preview["afterSha256"].as_str().unwrap()
            ),
            Err(Error::Validation(_))
        ));
        self.unchanged();
        assert!(
            !self
                .world
                .root
                .join(preview["sourcePath"].as_str().unwrap())
                .exists()
        );
        preview
    }

    fn refused_update(&self, id: &str, request: &str) -> Json {
        let preview =
            authoring::preview_record_update(&self.world, id, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], false, "{request}: {preview}");
        assert!(matches!(
            authoring::apply_record_update(
                &self.world,
                id,
                request,
                &self.world.fingerprint,
                preview["afterSha256"].as_str().unwrap()
            ),
            Err(Error::Validation(_))
        ));
        self.unchanged();
        preview
    }
}

fn route_request(id: &str, from: &str, to: &str) -> String {
    format!(
        "kind: route\nid: {id}\ntitle: Ferry\nfrom_location_id: {from}\nto_location_id: {to}\ndirection: one-way\nmodes: [foot, boat]\n"
    )
}

#[test]
fn root_locations_gain_an_explicit_parent_and_omitted_parents_are_preserved() {
    let mut fixture = Fixture::new(true);
    let original_path = fixture.world.records[A].source_path.clone();
    assert_eq!(
        spatial::location(&fixture.world, A, None, 100).unwrap()["parentId"],
        Json::Null
    );
    fixture.update(A, &format!("kind: location\nparent_id: {B}\n"));
    fixture.update(B, &format!("kind: location\nparent_id: {C}\n"));
    fixture.update(A, "kind: location\ntitle: New Harbor\nbody: Harbor prose\n");
    let place = spatial::location(&fixture.world, A, None, 100).unwrap();
    assert_eq!(place["parentId"], B);
    assert_eq!(place["ancestors"][0]["locationId"], B);
    assert_eq!(place["ancestors"][1]["locationId"], C);
    assert_eq!(
        spatial::location(&fixture.world, B, None, 100).unwrap()["children"][0]["locationId"],
        A
    );
    assert_eq!(fixture.world.records[A].source_path, original_path);
    assert_eq!(fixture.world.records[A].body, "Harbor prose");
    let bytes = fixture.world.records[A].raw_bytes.clone();
    assert_eq!(
        fixture.update(A, &format!("kind: location\nparent_id: {B}\n"))["changed"],
        false
    );
    assert_eq!(fixture.world.records[A].raw_bytes, bytes);
}

#[test]
fn route_defaults_and_typed_metrics_survive_review_publication_and_reads() {
    let mut fixture = Fixture::new(true);
    let request = format!(
        "{}route_distance: {{value: 18446744073709551615, unit: metres}}\ntravel_cost: {{value: 9223372036854775807, unit: coins}}\nduration: {{value: 1.25, unit: hours}}\nbody: Ferry prose\n",
        route_request(ROUTE, A, B)
    );
    fixture.create(&request);
    let record = &fixture.world.records[ROUTE];
    assert_eq!(record.status(), "canonical");
    assert_eq!(record.string("availability"), Some("open"));
    assert_eq!(record.string("uncertainty"), Some("exact"));
    assert_eq!(record.source_path, format!("story/routes/{ROUTE}.md"));
    assert_eq!(
        record.get("route_distance").unwrap()["value"].as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        record.get("travel_cost").unwrap()["value"].as_i64(),
        Some(i64::MAX)
    );
    let route =
        &spatial::location(&fixture.world, A, Some("foot"), 100).unwrap()["outgoingRoutes"][0];
    assert_eq!(route["routeDistance"]["value"], "18446744073709551615");
    assert_eq!(route["travelCost"]["value"], "9223372036854775807");
    assert_eq!(route["duration"]["value"], 1.25);
    assert_eq!(route["citation"]["fingerprint"], fixture.world.fingerprint);
    let path = spatial::path(&fixture.world, A, B, None, "hops", None, 100).unwrap();
    assert_eq!(path["metric"], "hops");
    assert_eq!(
        path["edges"][0]["routeDistance"]["value"],
        "18446744073709551615"
    );
    fixture.update(
        ROUTE,
        "kind: route\nduration: {value: 0, unit: hours}\nuncertainty: estimated\n",
    );
    assert_eq!(fixture.world.records[ROUTE].body, "Ferry prose");
    let route = &spatial::location(&fixture.world, A, None, 100).unwrap()["outgoingRoutes"][0];
    assert_eq!(route["duration"]["value"], "0");
    assert_eq!(route["routeDistance"]["value"], "18446744073709551615");
    assert_eq!(route["uncertainty"], "estimated");
}

#[test]
fn route_edits_change_directed_adjacency_and_mode_and_availability_filters() {
    let mut fixture = Fixture::new(true);
    fixture.create(&route_request(ROUTE, A, B));
    assert_eq!(
        spatial::path(&fixture.world, A, B, Some("foot"), "hops", None, 100).unwrap()["found"],
        true
    );
    assert_eq!(
        spatial::path(&fixture.world, B, A, None, "hops", None, 100).unwrap()["found"],
        false
    );
    fixture.update(ROUTE, "kind: route\ndirection: two-way\nmodes: [boat]\n");
    let reverse = spatial::path(&fixture.world, B, A, Some("boat"), "hops", None, 100).unwrap();
    assert_eq!(reverse["found"], true);
    assert_eq!(reverse["edges"][0]["fromLocationId"], B);
    assert_eq!(reverse["edges"][0]["toLocationId"], A);
    assert_eq!(
        spatial::path(&fixture.world, A, B, Some("foot"), "hops", None, 100).unwrap()["found"],
        false
    );
    for availability in ["closed", "restricted", "unknown"] {
        fixture.update(
            ROUTE,
            &format!("kind: route\navailability: {availability}\n"),
        );
        assert_eq!(
            spatial::path(&fixture.world, A, B, None, "hops", None, 100).unwrap()["found"],
            false
        );
        assert!(
            spatial::location(&fixture.world, B, None, 100).unwrap()["outgoingRoutes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    fixture.update(
        ROUTE,
        "kind: route\navailability: open\ndirection: one-way\n",
    );
    assert_eq!(
        spatial::path(&fixture.world, A, B, None, "hops", None, 100).unwrap()["found"],
        true
    );
    assert_eq!(
        spatial::path(&fixture.world, B, A, None, "hops", None, 100).unwrap()["found"],
        false
    );
}

#[test]
fn route_no_ops_body_only_edits_and_partial_edits_preserve_unsubmitted_source() {
    let mut fixture = Fixture::new(true);
    fixture.create(&route_request(ROUTE, A, B));
    let original = &fixture.world.records[ROUTE];
    let prefix = std::str::from_utf8(&original.raw_bytes)
        .unwrap()
        .strip_suffix("---\n")
        .unwrap();
    let header = format!("{prefix}x-author-note: {{value: null, numbers: [1, 1.0, '1']}}\n# Authored route note.\n---\n").replace('\n', "\r\n");
    fs::write(
        fixture.world.root.join(&original.source_path),
        format!("{header}\r\n  Original prose — α  \n\t"),
    )
    .unwrap();
    fixture.world = source::load_world(&fixture.world.root, None).unwrap();
    let original = fixture.world.records[ROUTE].clone();
    assert_eq!(
        fixture.update(
            ROUTE,
            "kind: route\navailability: open\nmodes: [foot, boat]\n"
        )["changed"],
        false
    );
    assert_eq!(fixture.world.records[ROUTE].raw_bytes, original.raw_bytes);
    let replacement = "New route prose\r\n\n  Keep spacing\t";
    let preview = fixture.update(
        ROUTE,
        &format!(
            "kind: route\nbody: {}\n",
            serde_json::to_string(replacement).unwrap()
        ),
    );
    assert_eq!(
        preview["afterSource"].as_str().unwrap(),
        format!("{header}{replacement}")
    );
    fixture.update(
        ROUTE,
        &format!("kind: route\nto_location_id: {C}\naliases: [Main Ferry]\n"),
    );
    let record = &fixture.world.records[ROUTE];
    assert_eq!(record.source_path, original.source_path);
    assert_eq!(record.body, replacement);
    assert_eq!(record.get("x-author-note"), original.get("x-author-note"));
    assert_eq!(record.get("modes"), original.get("modes"));
    assert_eq!(record.string("to_location_id"), Some(C));
}

#[test]
fn malformed_spatial_requests_fail_before_candidate_review() {
    let mut fixture = Fixture::new(true);
    let request = route_request(ROUTE, A, B);
    for required in [
        "kind",
        "id",
        "title",
        "from_location_id",
        "to_location_id",
        "direction",
        "modes",
    ] {
        let incomplete = request
            .lines()
            .filter(|line| !line.starts_with(&format!("{required}:")))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            authoring::preview_record_create(
                &fixture.world,
                &incomplete,
                &fixture.world.fingerprint
            )
            .is_err(),
            "missing {required}"
        );
    }
    fixture.create(&request);
    for fields in [
        "status: draft",
        "parent_id: location:harbor",
        "direction: both",
        "direction: true",
        "modes: []",
        "modes: [boat, boat]",
        "modes: ['']",
        "modes: [1]",
        "modes: boat",
        "availability: available",
        "uncertainty: probable",
        "from_location_id: Harbor",
        "to_location_id: char_00000000000000000000000001",
        "to_location_id: null",
        "route_distance: {value: -1, unit: metres}",
        "route_distance: {value: .nan, unit: metres}",
        "route_distance: {value: .inf, unit: metres}",
        "route_distance: {value: '1', unit: metres}",
        "route_distance: {value: true, unit: metres}",
        "route_distance: {value: 1, unit: ''}",
        "route_distance: {value: 1, unit: 3}",
        "route_distance: {value: 1}",
        "route_distance: {unit: metres}",
        "route_distance: {value: 1, unit: metres, extra: 1}",
        "duration: null",
        "travel_cost: [1, coins]",
        "route_distance: {value: !number 1, unit: metres}",
    ] {
        let patch = format!("kind: route\n{fields}\n");
        assert!(
            authoring::preview_record_update(
                &fixture.world,
                ROUTE,
                &patch,
                &fixture.world.fingerprint
            )
            .is_err(),
            "accepted {patch}"
        );
    }
    for fields in [
        "parent: location:harbor",
        "parent_id: null",
        "parent_id: Harbor",
        "parent_id: 1",
        "parent_id: {entity: location:harbor}",
    ] {
        let patch = format!("kind: location\n{fields}\n");
        assert!(
            authoring::preview_record_update(&fixture.world, A, &patch, &fixture.world.fingerprint)
                .is_err(),
            "accepted {patch}"
        );
    }
    fixture.unchanged();
}

#[test]
fn semantic_spatial_failures_are_reviewable_and_never_write_source() {
    let mut fixture = Fixture::new(true);
    for parent in [A, MISSING, RETIRED] {
        fixture.refused_update(A, &format!("kind: location\nparent_id: {parent}\n"));
    }
    fixture.update(A, &format!("kind: location\nparent_id: {B}\n"));
    fixture.refused_update(B, &format!("kind: location\nparent_id: {A}\n"));
    for to in [A, MISSING, RETIRED] {
        fixture.refused_create(&route_request(ROUTE, A, to));
    }
    fixture.create(&route_request(ROUTE, A, B));
    for to in [A, MISSING, RETIRED] {
        fixture.refused_update(ROUTE, &format!("kind: route\nto_location_id: {to}\n"));
    }
    let without_capabilities = Fixture::new(false);
    let preview = without_capabilities.refused_create(&route_request(ROUTE, A, B));
    assert!(preview.to_string().contains("NAR-CAPABILITY-001"));
}

#[test]
fn spatial_ids_produce_safe_stable_paths_and_cannot_escape_the_world() {
    let mut fixture = Fixture::new(true);
    let id = "route:harbor/main-ferry";
    let preview = fixture.create(&route_request(id, A, B));
    let expected = format!(
        "story/routes/route-{}.md",
        hex::encode(Sha256::digest(id.as_bytes()))
    );
    assert_eq!(preview["sourcePath"], expected);
    fixture.update(id, "kind: route\ntitle: Renamed Ferry\n");
    assert_eq!(fixture.world.records[id].source_path, expected);
    for bad in [
        "route:../outside",
        "route:town/../../outside",
        "route:/absolute",
        "route:town\\outside",
        "route:Town",
        "route:con",
        "route:town//ferry",
    ] {
        assert!(
            record_edit::create(&fixture.world, &route_request(bad, A, B)).is_err(),
            "accepted {bad}"
        );
    }
    let location_id = "location:town/harbor";
    fixture.create(&format!(
        "kind: location\nid: {location_id}\ntitle: Harbor Annex\n"
    ));
    let path = fixture.world.records[location_id].source_path.clone();
    assert_eq!(
        path,
        format!(
            "story/locations/location-{}.md",
            hex::encode(Sha256::digest(location_id.as_bytes()))
        )
    );
    fixture.update(location_id, &format!("kind: location\nparent_id: {A}\n"));
    assert_eq!(fixture.world.records[location_id].source_path, path);

    // A directory occupying the deterministic filename must not be replaced.
    let request = route_request(ROUTE, A, B);
    let preview =
        authoring::preview_record_create(&fixture.world, &request, &fixture.world.fingerprint)
            .unwrap();
    let target = fixture
        .world
        .root
        .join(preview["sourcePath"].as_str().unwrap());
    fs::create_dir(&target).unwrap();
    assert!(
        authoring::apply_record_create(
            &fixture.world,
            &request,
            &fixture.world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        )
        .is_err()
    );
    assert!(target.is_dir());
    fixture.unchanged();
}

#[test]
fn every_packaged_location_and_route_supports_an_exact_byte_no_op() {
    let mut counts = (0, 0);
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name);
        let world = source::load_world(&root, None).unwrap();
        for record in world
            .records
            .values()
            .filter(|record| matches!(record.kind.as_str(), "location" | "route"))
        {
            let request = format!(
                "kind: {}\ntitle: {}\n",
                record.kind,
                serde_json::to_string(&record.title).unwrap()
            );
            let result = record_edit::update(&world, &record.id, &request).unwrap();
            assert_eq!(result.raw_bytes, record.raw_bytes, "{}", record.source_path);
            assert_eq!(result.source_path, record.source_path);
            assert_eq!(result.blob_oid, record.blob_oid);
            if record.kind == "location" {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
    }
    assert_eq!(counts, (58, 2));
}
