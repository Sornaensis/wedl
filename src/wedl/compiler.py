from __future__ import annotations

from contextlib import closing
from dataclasses import dataclass
from functools import lru_cache
from itertools import combinations
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
from typing import Any

from . import COMPILED_SOURCE_SCHEMAS, SOURCE_SCHEMA, SQLITE_SCHEMA, __version__
from .conversation import turn_time
from .errors import CompileRequired, UsageError, ValidationFailed
from .model import Record, StoryTime, World
from .repository import Repository
from .profiles import CompilationProfile, resolve_profile
from .search import build_documents
from .vectors import VectorModel, blob_to_vector, build_vectors, embed_documents, model_from_row, normalize_vector, vector_to_blob
from .semantics import canonical_events, current_knowledge, effective_time, evaluate_all_story_points, event_time, relationships_from, resolve_state
from .source import extract_entity_refs, markdown_entity_links
from .util import canonical_json, sha256_bytes
from .validation import validate_world
from .chronology_index import build_chronology_projection, insert_chronology_index
from .v07 import SOURCE_SCHEMA as V07_SOURCE_SCHEMA
from .spatial_index import build_spatial_projection, insert_spatial_index
from .generational_index import insert_generational_index


# Bump when search-document construction changes without a SQLite DDL change.
# The token is persisted inside compiler_fingerprint and is checked before the
# early cache hit path as well as require_database's compatibility gate.
DOCUMENT_GENERATION_TOKEN = "wedl-document-generation/v4"
CHRONOLOGY_INDEX_GENERATION_TOKEN = "wedl-chronology-index/v3"
# The spatial projection is latent until the explicit v0.7 migration task
# enables generic compilation.  It still participates in the cache contract:
# a database created before its DDL must never be mistaken for a compatible
# read model by a later opt-in caller.
SPATIAL_INDEX_GENERATION_TOKEN = "wedl-spatial-index/v4"
GENERATIONAL_INDEX_GENERATION_TOKEN = "wedl-generational-index/v8"
COMPILER_FINGERPRINT_PREFIX = f"{DOCUMENT_GENERATION_TOKEN}:{CHRONOLOGY_INDEX_GENERATION_TOKEN}:{SPATIAL_INDEX_GENERATION_TOKEN}:{GENERATIONAL_INDEX_GENERATION_TOKEN}:"

DDL = r"""
PRAGMA foreign_keys=ON;
CREATE TABLE revision(head_commit TEXT NOT NULL,tree_oid TEXT NOT NULL,source_schema TEXT NOT NULL,sqlite_schema TEXT NOT NULL,compiler_version TEXT NOT NULL,compiler_fingerprint TEXT NOT NULL,search_profile TEXT NOT NULL,profile_json TEXT NOT NULL,vector_models_json TEXT NOT NULL,record_count INTEGER NOT NULL,compiled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,build_mode TEXT NOT NULL);
CREATE TABLE compile_metric(stage TEXT PRIMARY KEY,milliseconds REAL NOT NULL,details_json TEXT NOT NULL);
CREATE TABLE entity(id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,domain TEXT NOT NULL,status TEXT NOT NULL,source_path TEXT NOT NULL UNIQUE,blob_oid TEXT,body_markdown TEXT NOT NULL,frontmatter_json TEXT NOT NULL);
CREATE TABLE entity_tag(entity_id TEXT,tag TEXT,PRIMARY KEY(entity_id,tag));
CREATE TABLE entity_alias(entity_id TEXT,alias TEXT,PRIMARY KEY(entity_id,alias));
CREATE TABLE entity_ref(source_id TEXT,target_id TEXT,source_field TEXT,PRIMARY KEY(source_id,target_id,source_field));
CREATE TABLE narrative_thread(id TEXT PRIMARY KEY,label TEXT NOT NULL);
CREATE TABLE record_thread(record_id TEXT NOT NULL REFERENCES entity(id),thread_id TEXT NOT NULL REFERENCES narrative_thread(id),PRIMARY KEY(record_id,thread_id));
CREATE TABLE event(entity_id TEXT PRIMARY KEY,timeline TEXT,tick INTEGER,ordering INTEGER,location_id TEXT,event_status TEXT);
CREATE TABLE event_participant(event_id TEXT,character_id TEXT,role TEXT,ordinal INTEGER,PRIMARY KEY(event_id,ordinal));
CREATE TABLE state_effect(effect_id TEXT PRIMARY KEY,event_id TEXT,target_id TEXT,state_key TEXT,operation TEXT,value_json TEXT,ordinal INTEGER);
CREATE TABLE knowledge(entity_id TEXT PRIMARY KEY,knower_id TEXT,claim_key TEXT,statement TEXT,claim_json TEXT);
CREATE TABLE knowledge_transition(transition_id TEXT PRIMARY KEY,knowledge_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,state TEXT,confidence REAL,acquisition TEXT,causing_event_id TEXT,source_entity_id TEXT,note TEXT,ordinal INTEGER);
CREATE TABLE relationship(entity_id TEXT PRIMARY KEY,from_id TEXT,to_id TEXT,relationship_kind TEXT,inverse_id TEXT);
CREATE TABLE relationship_transition(transition_id TEXT PRIMARY KEY,relationship_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,status TEXT,metrics_json TEXT,facets_json TEXT,causing_event_id TEXT,note TEXT,ordinal INTEGER);
CREATE TABLE scene(entity_id TEXT PRIMARY KEY,status TEXT,timeline TEXT,start_tick INTEGER,start_order INTEGER,current_tick INTEGER,current_order INTEGER,end_tick INTEGER,end_order INTEGER,location_id TEXT);
CREATE TABLE scene_participant(scene_id TEXT,character_id TEXT,role TEXT,point_of_view INTEGER,from_tick INTEGER,from_order INTEGER,to_tick INTEGER,to_order INTEGER,PRIMARY KEY(scene_id,character_id));
CREATE TABLE scene_observation(observation_id TEXT PRIMARY KEY,scene_id TEXT,audience_json TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,until_tick INTEGER,until_order INTEGER,text TEXT,salience REAL);
CREATE TABLE conversation(entity_id TEXT PRIMARY KEY,status TEXT,scene_id TEXT,location_id TEXT,timeline TEXT,start_tick INTEGER,start_order INTEGER,end_tick INTEGER,end_order INTEGER);
CREATE TABLE conversation_participant(conversation_id TEXT,character_id TEXT,role TEXT,from_tick INTEGER,from_order INTEGER,to_tick INTEGER,to_order INTEGER,PRIMARY KEY(conversation_id,character_id));
CREATE TABLE conversation_turn(turn_id TEXT PRIMARY KEY,conversation_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,speaker_id TEXT,text_verbatim TEXT,delivery TEXT,audience_json TEXT,metadata_json TEXT);
CREATE TABLE conversation_recollection(recollection_id TEXT PRIMARY KEY,conversation_id TEXT,character_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,state TEXT,summary TEXT,interpretation TEXT,emotional_impression TEXT,confidence REAL,exact_turns_json TEXT,remembered_quotes_json TEXT);
CREATE TABLE story_point(entity_id TEXT PRIMARY KEY,initial_state TEXT,activation_policy TEXT,priority INTEGER,repeat_policy TEXT,trigger_json TEXT,dependencies_json TEXT);
CREATE TABLE story_point_transition(transition_id TEXT PRIMARY KEY,story_point_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,state TEXT,causing_event_id TEXT,note TEXT,ordinal INTEGER);
CREATE TABLE search_document(document_id TEXT UNIQUE NOT NULL,entity_id TEXT NOT NULL,document_kind TEXT NOT NULL,heading TEXT,audience_kind TEXT NOT NULL,audience_character_id TEXT,scene_id TEXT,timeline TEXT,from_tick INTEGER,from_order INTEGER,until_tick INTEGER,until_order INTEGER,text TEXT NOT NULL,metadata_json TEXT NOT NULL,chunk_hash TEXT NOT NULL);
CREATE VIRTUAL TABLE search_fts USING fts5(title,aliases,heading,text,domain,tags,tokenize='porter unicode61 remove_diacritics 2');
CREATE TABLE vector_model(scope TEXT PRIMARY KEY,model_id TEXT NOT NULL,provider TEXT NOT NULL,model_name TEXT NOT NULL,dimensions INTEGER NOT NULL,normalized INTEGER NOT NULL,corpus_hash TEXT,config_json TEXT NOT NULL,model_blob BLOB);
CREATE TABLE vector_embedding(vector_id INTEGER PRIMARY KEY,model_id TEXT NOT NULL,input_hash TEXT NOT NULL,dimensions INTEGER NOT NULL,norm REAL NOT NULL,vector BLOB NOT NULL,UNIQUE(model_id,input_hash));
CREATE TABLE document_vector(document_id TEXT PRIMARY KEY,vector_id INTEGER NOT NULL);
CREATE TABLE current_state(entity_id TEXT,state_key TEXT,value_json TEXT,citation_json TEXT,PRIMARY KEY(entity_id,state_key));
CREATE TABLE current_knowledge(knowledge_id TEXT PRIMARY KEY,knower_id TEXT,claim_key TEXT,state TEXT,confidence REAL,value_json TEXT);
CREATE TABLE current_relationship(relationship_id TEXT PRIMARY KEY,from_id TEXT,to_id TEXT,status TEXT,metrics_json TEXT,facets_json TEXT,value_json TEXT);
CREATE TABLE story_point_current(story_point_id TEXT PRIMARY KEY,stored_state TEXT,derived_state TEXT,eligible INTEGER,value_json TEXT);
CREATE TABLE character_interaction(event_id TEXT,first_character_id TEXT,second_character_id TEXT,timeline TEXT,tick INTEGER,ordering INTEGER,value_json TEXT,PRIMARY KEY(event_id,first_character_id,second_character_id));
CREATE TABLE chronology_calendar(id TEXT PRIMARY KEY,source_ordinal INTEGER NOT NULL UNIQUE,label TEXT NOT NULL,basis_id TEXT NOT NULL,has_epoch INTEGER NOT NULL,definition_json TEXT NOT NULL);
CREATE TABLE chronology_capability(available INTEGER NOT NULL CHECK(available IN (0,1)));
CREATE TABLE chronology_era(id TEXT PRIMARY KEY,source_ordinal INTEGER NOT NULL UNIQUE,calendar_id TEXT NOT NULL REFERENCES chronology_calendar(id),label TEXT NOT NULL,basis_id TEXT NOT NULL,definition_json TEXT NOT NULL);
CREATE TABLE chronology_anchor(id TEXT PRIMARY KEY,source_ordinal INTEGER NOT NULL UNIQUE,axis_day INTEGER NOT NULL,timeline TEXT NOT NULL,tick INTEGER NOT NULL,ordering INTEGER NOT NULL,provenance_json TEXT NOT NULL);
CREATE TABLE chronology_annotation(record_id TEXT NOT NULL REFERENCES entity(id),record_ordinal INTEGER NOT NULL,annotation_id TEXT NOT NULL,source_ordinal INTEGER NOT NULL,role TEXT,display TEXT,provenance_json TEXT NOT NULL,value_kind TEXT NOT NULL,calendar_id TEXT REFERENCES chronology_calendar(id),era_id TEXT REFERENCES chronology_era(id),precision TEXT NOT NULL,basis_id TEXT,lower_day INTEGER,upper_day INTEGER,lower_unbounded INTEGER NOT NULL,upper_unbounded INTEGER NOT NULL,comparison_kind TEXT NOT NULL,exclusion_reason TEXT,unknown_basis INTEGER NOT NULL CHECK(unknown_basis IN (0,1)),value_json TEXT NOT NULL,PRIMARY KEY(record_id,annotation_id),UNIQUE(record_id,source_ordinal));
CREATE TABLE chronology_annotation_basis_scope(record_id TEXT NOT NULL,annotation_id TEXT NOT NULL,basis_id TEXT NOT NULL,PRIMARY KEY(record_id,annotation_id,basis_id),FOREIGN KEY(record_id,annotation_id) REFERENCES chronology_annotation(record_id,annotation_id));
CREATE TABLE chronology_annotation_era_scope(record_id TEXT NOT NULL,annotation_id TEXT NOT NULL,era_id TEXT NOT NULL,PRIMARY KEY(record_id,annotation_id,era_id),FOREIGN KEY(record_id,annotation_id) REFERENCES chronology_annotation(record_id,annotation_id));
CREATE TABLE spatial_capability(name TEXT PRIMARY KEY,source_ordinal INTEGER NOT NULL UNIQUE);
CREATE TABLE spatial_map(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,crs TEXT NOT NULL,axis_first TEXT NOT NULL,axis_second TEXT NOT NULL,unit TEXT NOT NULL,z_policy TEXT NOT NULL,min_x NUMERIC NOT NULL,min_y NUMERIC NOT NULL,min_z NUMERIC,max_x NUMERIC NOT NULL,max_y NUMERIC NOT NULL,max_z NUMERIC,definition_json TEXT NOT NULL);
CREATE TABLE spatial_location(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,has_spatial INTEGER NOT NULL CHECK(has_spatial IN (0,1)),parent_id TEXT REFERENCES spatial_location(id) DEFERRABLE INITIALLY DEFERRED,map_id TEXT REFERENCES spatial_map(id),geometry_kind TEXT,min_x NUMERIC,min_y NUMERIC,min_z NUMERIC,max_x NUMERIC,max_y NUMERIC,max_z NUMERIC,geometry_json TEXT,definition_json TEXT NOT NULL);
CREATE TABLE spatial_location_vertex(location_id TEXT NOT NULL REFERENCES spatial_location(id),vertex_ordinal INTEGER NOT NULL,x NUMERIC NOT NULL,y NUMERIC NOT NULL,z NUMERIC,PRIMARY KEY(location_id,vertex_ordinal));
CREATE TABLE spatial_hierarchy(location_id TEXT PRIMARY KEY REFERENCES spatial_location(id),parent_id TEXT NOT NULL REFERENCES spatial_location(id),source_ordinal INTEGER NOT NULL);
CREATE TABLE spatial_location_link(location_id TEXT NOT NULL REFERENCES spatial_location(id),target_location_id TEXT NOT NULL REFERENCES spatial_location(id),source_ordinal INTEGER NOT NULL,definition_json TEXT NOT NULL,PRIMARY KEY(location_id,source_ordinal),UNIQUE(location_id,target_location_id));
CREATE TABLE spatial_route(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,from_location_id TEXT NOT NULL REFERENCES spatial_location(id),to_location_id TEXT NOT NULL REFERENCES spatial_location(id),direction TEXT NOT NULL,modes_json TEXT NOT NULL,availability TEXT NOT NULL,uncertainty TEXT NOT NULL,route_distance NUMERIC,route_distance_unit TEXT,travel_cost NUMERIC,travel_cost_unit TEXT,duration NUMERIC,duration_unit TEXT,definition_json TEXT NOT NULL);
CREATE TABLE spatial_route_edge(route_id TEXT NOT NULL REFERENCES spatial_route(id),from_location_id TEXT NOT NULL REFERENCES spatial_location(id),to_location_id TEXT NOT NULL REFERENCES spatial_location(id),reverse_of_authored INTEGER NOT NULL CHECK(reverse_of_authored IN (0,1)),PRIMARY KEY(route_id,reverse_of_authored));
CREATE TABLE spatial_route_mode(route_id TEXT NOT NULL REFERENCES spatial_route(id),mode TEXT NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(route_id,mode));
CREATE TABLE spatial_anchor(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,from_map_id TEXT NOT NULL REFERENCES spatial_map(id),from_coordinates_json TEXT NOT NULL,to_map_id TEXT NOT NULL REFERENCES spatial_map(id),to_coordinates_json TEXT NOT NULL,conversion TEXT,definition_json TEXT NOT NULL);
CREATE TABLE spatial_portal(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,from_location_id TEXT NOT NULL REFERENCES spatial_location(id),target_kind TEXT NOT NULL,target_location_id TEXT REFERENCES spatial_location(id),target_map_id TEXT REFERENCES spatial_map(id),target_coordinates_json TEXT,modes_json TEXT NOT NULL,definition_json TEXT NOT NULL,CHECK((target_kind='location' AND target_location_id IS NOT NULL AND target_map_id IS NULL AND target_coordinates_json IS NULL) OR (target_kind='position' AND target_location_id IS NULL AND target_map_id IS NOT NULL AND target_coordinates_json IS NOT NULL)));
CREATE TABLE spatial_portal_mode(portal_id TEXT NOT NULL REFERENCES spatial_portal(id),mode TEXT NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(portal_id,mode));
CREATE TABLE spatial_overlay(id TEXT PRIMARY KEY REFERENCES entity(id),source_ordinal INTEGER NOT NULL UNIQUE,lifecycle TEXT NOT NULL,audience_json TEXT NOT NULL,perspectives_json TEXT NOT NULL,timeline TEXT,start_tick INTEGER,start_order INTEGER,end_tick INTEGER,end_order INTEGER,definition_json TEXT NOT NULL);
CREATE TABLE spatial_overlay_location(overlay_id TEXT NOT NULL REFERENCES spatial_overlay(id),location_id TEXT NOT NULL REFERENCES spatial_location(id),source_ordinal INTEGER NOT NULL,PRIMARY KEY(overlay_id,location_id));
CREATE TABLE spatial_overlay_audience(overlay_id TEXT NOT NULL REFERENCES spatial_overlay(id),audience TEXT NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(overlay_id,audience));
CREATE TABLE spatial_overlay_perspective(overlay_id TEXT NOT NULL REFERENCES spatial_overlay(id),perspective TEXT NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(overlay_id,perspective));
CREATE TABLE spatial_overlay_lens_location(location_id TEXT NOT NULL REFERENCES spatial_location(id),audience TEXT NOT NULL,perspective TEXT NOT NULL,overlay_id TEXT NOT NULL REFERENCES spatial_overlay(id),lifecycle TEXT NOT NULL,timeline TEXT,start_tick INTEGER,start_order INTEGER,end_tick INTEGER,end_order INTEGER,PRIMARY KEY(location_id,audience,perspective,overlay_id));
CREATE TABLE spatial_overlay_scope_key(id INTEGER PRIMARY KEY,location_id TEXT NOT NULL REFERENCES spatial_location(id),audience TEXT NOT NULL,perspective TEXT NOT NULL,timeline TEXT NOT NULL,UNIQUE(location_id,audience,perspective,timeline));
CREATE TABLE generational_record(id TEXT PRIMARY KEY REFERENCES entity(id),kind TEXT NOT NULL,source_ordinal INTEGER NOT NULL UNIQUE,source_path TEXT NOT NULL,blob_oid TEXT,status TEXT NOT NULL,capability TEXT NOT NULL,timeline TEXT NOT NULL,audience_json TEXT NOT NULL,perspectives_json TEXT NOT NULL);
CREATE TABLE generational_organization(id TEXT PRIMARY KEY REFERENCES generational_record(id),organization_kind TEXT NOT NULL,parent_id TEXT,location_id TEXT);
CREATE TABLE generational_parentage(id TEXT PRIMARY KEY REFERENCES generational_record(id),child_id TEXT NOT NULL,parent_id TEXT NOT NULL,timeline TEXT NOT NULL,start_tick INTEGER NOT NULL,start_order INTEGER NOT NULL,source_ordinal INTEGER NOT NULL);
CREATE TABLE generational_union(id TEXT PRIMARY KEY REFERENCES generational_record(id));
CREATE TABLE generational_union_participant(union_id TEXT NOT NULL REFERENCES generational_union(id),transition_id TEXT NOT NULL REFERENCES generational_transition(id),participant_id TEXT NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(transition_id,participant_id));
CREATE TABLE generational_affiliation(id TEXT PRIMARY KEY REFERENCES generational_record(id),character_id TEXT NOT NULL,organization_id TEXT NOT NULL);
CREATE TABLE generational_legacy(id TEXT PRIMARY KEY REFERENCES generational_record(id),legacy_kind TEXT NOT NULL,organization_id TEXT);
CREATE TABLE generational_tenure(id TEXT PRIMARY KEY REFERENCES generational_record(id),legacy_id TEXT NOT NULL,predecessor_tenure_id TEXT,successor_tenure_id TEXT);
CREATE TABLE generational_claim(id TEXT PRIMARY KEY REFERENCES generational_record(id),legacy_id TEXT NOT NULL,claimant_id TEXT NOT NULL);
CREATE TABLE generational_vital(id TEXT PRIMARY KEY REFERENCES generational_record(id),character_id TEXT NOT NULL,disclosure TEXT NOT NULL);
CREATE TABLE generational_transition(id TEXT PRIMARY KEY,record_id TEXT NOT NULL REFERENCES generational_record(id),source_ordinal INTEGER NOT NULL,transition_kind TEXT NOT NULL,applicability_kind TEXT NOT NULL,timeline TEXT NOT NULL,start_tick INTEGER NOT NULL,start_order INTEGER NOT NULL,end_tick INTEGER,end_order INTEGER,payload_json TEXT NOT NULL,cause_event_id TEXT,cause_citation_json TEXT,replaces_transition_id TEXT,citation_json TEXT NOT NULL,UNIQUE(record_id,source_ordinal));
CREATE TABLE generational_current(record_id TEXT PRIMARY KEY REFERENCES generational_record(id),timeline TEXT NOT NULL,at_tick INTEGER NOT NULL,at_order INTEGER NOT NULL,state TEXT NOT NULL,value_json TEXT NOT NULL);
CREATE TABLE generational_candidate(record_id TEXT NOT NULL REFERENCES generational_record(id),transition_id TEXT NOT NULL REFERENCES generational_transition(id),source_ordinal INTEGER NOT NULL,capability TEXT NOT NULL,timeline TEXT NOT NULL,start_tick INTEGER NOT NULL,start_order INTEGER NOT NULL,end_tick INTEGER,end_order INTEGER,audience_json TEXT NOT NULL,perspectives_json TEXT NOT NULL,citation_json TEXT NOT NULL,structural_json TEXT NOT NULL,PRIMARY KEY(record_id,transition_id));
CREATE TABLE generational_search_prefix(prefix TEXT NOT NULL,audience TEXT NOT NULL,perspective TEXT NOT NULL,timeline TEXT NOT NULL,start_tick INTEGER NOT NULL,start_order INTEGER NOT NULL,source_ordinal INTEGER NOT NULL,record_id TEXT NOT NULL REFERENCES generational_record(id),transition_id TEXT NOT NULL REFERENCES generational_transition(id),PRIMARY KEY(prefix,audience,perspective,timeline,record_id));
CREATE TABLE generational_discovery_name(audience TEXT NOT NULL,perspective TEXT NOT NULL,timeline TEXT NOT NULL,name_key TEXT NOT NULL,entity_id TEXT NOT NULL REFERENCES entity(id),kind TEXT NOT NULL,name TEXT NOT NULL,title TEXT NOT NULL,start_tick INTEGER NOT NULL,start_order INTEGER NOT NULL,source_ordinal INTEGER NOT NULL,PRIMARY KEY(audience,perspective,timeline,name_key,entity_id));
CREATE TABLE generational_discovery_time(audience TEXT NOT NULL,perspective TEXT NOT NULL,timeline TEXT NOT NULL,tick INTEGER NOT NULL,ordering INTEGER NOT NULL,time_rank INTEGER NOT NULL,PRIMARY KEY(audience,perspective,timeline,tick,ordering),UNIQUE(audience,perspective,timeline,time_rank));
CREATE TABLE generational_discovery_segment(audience TEXT NOT NULL,perspective TEXT NOT NULL,timeline TEXT NOT NULL,kind TEXT NOT NULL,node INTEGER NOT NULL,name_key TEXT NOT NULL,entity_id TEXT NOT NULL REFERENCES entity(id),name TEXT NOT NULL,title TEXT NOT NULL,PRIMARY KEY(audience,perspective,timeline,kind,node,name_key,entity_id));
CREATE TABLE generational_discovery_lens(audience TEXT NOT NULL,perspective TEXT NOT NULL,PRIMARY KEY(audience,perspective));
"""
INDEX_DDL = r"""
CREATE INDEX entity_kind_idx ON entity(kind,status,title);
CREATE INDEX entity_ref_target_idx ON entity_ref(target_id);
CREATE INDEX narrative_thread_label_idx ON narrative_thread(label,id);
CREATE INDEX record_thread_thread_idx ON record_thread(thread_id,record_id);
CREATE INDEX event_time_idx ON event(timeline,tick,ordering);
CREATE INDEX knowledge_knower_idx ON knowledge(knower_id,claim_key);
CREATE INDEX relationship_source_idx ON relationship(from_id,to_id);
CREATE INDEX conversation_turn_idx ON conversation_turn(conversation_id,timeline,tick,ordering);
CREATE INDEX recollection_character_idx ON conversation_recollection(character_id,timeline,tick,ordering);
CREATE INDEX search_access_idx ON search_document(audience_kind,audience_character_id,scene_id,timeline,from_tick,entity_id);
CREATE INDEX vector_embedding_input_idx ON vector_embedding(model_id,input_hash);
CREATE INDEX document_vector_idx ON document_vector(vector_id,document_id);
CREATE INDEX chronology_era_calendar_idx ON chronology_era(calendar_id,id);
CREATE INDEX chronology_anchor_axis_idx ON chronology_anchor(axis_day,id);
CREATE INDEX chronology_anchor_story_idx ON chronology_anchor(timeline,tick,ordering,id);
CREATE INDEX chronology_annotation_record_idx ON chronology_annotation(record_id,source_ordinal);
CREATE INDEX chronology_annotation_value_idx ON chronology_annotation(value_kind,calendar_id,era_id,record_id,source_ordinal);
CREATE INDEX chronology_annotation_basis_lower_idx ON chronology_annotation(basis_id,lower_day,record_ordinal,source_ordinal,record_id,annotation_id);
CREATE INDEX chronology_annotation_basis_upper_idx ON chronology_annotation(basis_id,upper_day,record_ordinal,source_ordinal,record_id,annotation_id);
CREATE INDEX chronology_annotation_era_basis_order_idx ON chronology_annotation(era_id,basis_id,record_ordinal,source_ordinal,record_id,annotation_id);
CREATE INDEX chronology_annotation_exclusion_idx ON chronology_annotation(comparison_kind,era_id,calendar_id,record_ordinal,source_ordinal,record_id,annotation_id);
CREATE INDEX chronology_annotation_basis_scope_idx ON chronology_annotation_basis_scope(basis_id,record_id,annotation_id);
CREATE INDEX chronology_annotation_era_scope_idx ON chronology_annotation_era_scope(era_id,record_id,annotation_id);
CREATE INDEX spatial_location_parent_idx ON spatial_location(parent_id,id);
CREATE INDEX spatial_location_map_bounds_idx ON spatial_location(map_id,min_x,max_x,min_y,max_y,id);
CREATE INDEX spatial_location_vertex_location_idx ON spatial_location_vertex(location_id,vertex_ordinal);
CREATE INDEX spatial_hierarchy_parent_idx ON spatial_hierarchy(parent_id,location_id);
CREATE INDEX spatial_location_link_target_idx ON spatial_location_link(target_location_id,location_id,source_ordinal);
CREATE INDEX spatial_route_from_idx ON spatial_route(from_location_id,to_location_id,id);
CREATE INDEX spatial_route_edge_from_idx ON spatial_route_edge(from_location_id,to_location_id,route_id,reverse_of_authored);
CREATE INDEX spatial_route_edge_to_idx ON spatial_route_edge(to_location_id,from_location_id,route_id,reverse_of_authored);
CREATE INDEX spatial_route_mode_mode_idx ON spatial_route_mode(mode,route_id);
CREATE INDEX spatial_anchor_maps_idx ON spatial_anchor(from_map_id,to_map_id,id);
CREATE INDEX spatial_portal_from_idx ON spatial_portal(from_location_id,id);
CREATE INDEX spatial_portal_target_location_idx ON spatial_portal(target_location_id,id);
CREATE INDEX spatial_portal_target_map_idx ON spatial_portal(target_map_id,id);
CREATE INDEX spatial_portal_mode_mode_idx ON spatial_portal_mode(mode,portal_id);
CREATE INDEX spatial_overlay_candidate_idx ON spatial_overlay(timeline,start_tick,start_order,end_tick,end_order,id);
CREATE INDEX spatial_overlay_reverse_idx ON spatial_overlay(timeline,end_tick,end_order,start_tick,start_order,id);
CREATE INDEX spatial_overlay_location_location_idx ON spatial_overlay_location(location_id,overlay_id);
CREATE INDEX spatial_overlay_audience_audience_idx ON spatial_overlay_audience(audience,overlay_id);
CREATE INDEX spatial_overlay_perspective_perspective_idx ON spatial_overlay_perspective(perspective,overlay_id);
CREATE INDEX spatial_overlay_static_lens_idx ON spatial_overlay_lens_location(location_id,audience,perspective,overlay_id) WHERE lifecycle='static';
CREATE INDEX generational_record_kind_idx ON generational_record(kind,timeline,source_ordinal,id);
CREATE INDEX generational_organization_parent_idx ON generational_organization(parent_id,id);
CREATE INDEX generational_parentage_child_idx ON generational_parentage(child_id,parent_id,id);
CREATE INDEX generational_parentage_parent_idx ON generational_parentage(parent_id,child_id,id);
CREATE INDEX generational_parentage_child_time_idx ON generational_parentage(child_id,timeline,start_tick,start_order,source_ordinal,id,parent_id);
CREATE INDEX generational_parentage_parent_time_idx ON generational_parentage(parent_id,timeline,start_tick,start_order,source_ordinal,id,child_id);
CREATE INDEX generational_union_participant_idx ON generational_union_participant(participant_id,union_id);
CREATE INDEX generational_legacy_organization_idx ON generational_legacy(organization_id,id);
CREATE INDEX generational_affiliation_organization_idx ON generational_affiliation(organization_id,character_id,id);
CREATE INDEX generational_tenure_legacy_idx ON generational_tenure(legacy_id,id);
CREATE INDEX generational_claim_legacy_idx ON generational_claim(legacy_id,id);
CREATE INDEX generational_vital_character_idx ON generational_vital(character_id,id);
CREATE INDEX generational_transition_asof_idx ON generational_transition(record_id,timeline,start_tick,start_order,source_ordinal,id);
CREATE INDEX generational_candidate_asof_idx ON generational_candidate(timeline,start_tick,start_order,record_id,source_ordinal);
CREATE INDEX generational_search_lookup_idx ON generational_search_prefix(audience,perspective,timeline,prefix,start_tick,start_order,source_ordinal,record_id);
CREATE INDEX generational_discovery_key_idx ON generational_discovery_name(audience,perspective,timeline,kind,name_key,entity_id,start_tick,start_order);
CREATE INDEX generational_discovery_id_idx ON generational_discovery_name(entity_id,audience,perspective,timeline,start_tick,start_order);
CREATE INDEX generational_discovery_label_idx ON generational_discovery_name(entity_id,audience,perspective,timeline,start_tick,start_order,source_ordinal) WHERE name=title;
CREATE INDEX generational_discovery_segment_idx ON generational_discovery_segment(audience,perspective,timeline,kind,node,name_key,entity_id);
"""

_REQUIRED_SPATIAL_TABLES = frozenset({
    "spatial_capability", "spatial_map", "spatial_location",
    "spatial_location_vertex", "spatial_hierarchy", "spatial_location_link",
    "spatial_route", "spatial_route_edge", "spatial_route_mode",
    "spatial_anchor", "spatial_portal", "spatial_portal_mode",
    "spatial_overlay", "spatial_overlay_location", "spatial_overlay_audience",
    "spatial_overlay_perspective", "spatial_overlay_lens_location", "spatial_overlay_scope_key",
})
_REQUIRED_SPATIAL_INDEXES = frozenset({
    "spatial_location_parent_idx", "spatial_location_map_bounds_idx",
    "spatial_location_vertex_location_idx", "spatial_hierarchy_parent_idx",
    "spatial_location_link_target_idx", "spatial_route_from_idx",
    "spatial_route_edge_from_idx", "spatial_route_edge_to_idx", "spatial_route_mode_mode_idx",
    "spatial_anchor_maps_idx", "spatial_portal_from_idx",
    "spatial_portal_target_location_idx", "spatial_portal_target_map_idx",
    "spatial_portal_mode_mode_idx", "spatial_overlay_candidate_idx",
    "spatial_overlay_reverse_idx", "spatial_overlay_location_location_idx",
    "spatial_overlay_audience_audience_idx",
    "spatial_overlay_perspective_perspective_idx", "spatial_overlay_static_lens_idx",
})
_REQUIRED_GENERATIONAL_TABLES = frozenset({
    "generational_record", "generational_organization", "generational_parentage",
    "generational_union", "generational_union_participant", "generational_affiliation",
    "generational_legacy", "generational_tenure", "generational_claim",
    "generational_vital", "generational_transition", "generational_current",
    "generational_candidate", "generational_search_prefix", "generational_discovery_name",
    "generational_discovery_lens", "generational_discovery_time", "generational_discovery_segment",
})
_REQUIRED_GENERATIONAL_INDEXES = frozenset({
    "generational_record_kind_idx", "generational_organization_parent_idx",
    "generational_parentage_child_idx", "generational_parentage_parent_idx",
    "generational_parentage_child_time_idx", "generational_parentage_parent_time_idx",
    "generational_union_participant_idx",
    "generational_legacy_organization_idx",
    "generational_affiliation_organization_idx", "generational_tenure_legacy_idx",
    "generational_claim_legacy_idx", "generational_vital_character_idx",
    "generational_transition_asof_idx", "generational_candidate_asof_idx",
    "generational_search_lookup_idx", "generational_discovery_key_idx",
    "generational_discovery_id_idx", "generational_discovery_label_idx",
    "generational_discovery_segment_idx",
})


def _generational_shapes(connection: sqlite3.Connection) -> tuple[dict[str, tuple[Any, ...]], dict[str, tuple[Any, ...]]]:
    """Read complete table/foreign-key and index-column shapes by name."""
    tables = {
        name: (
            tuple(tuple(row) for row in connection.execute(f"PRAGMA table_xinfo({name})")),
            tuple(tuple(row) for row in connection.execute(f"PRAGMA foreign_key_list({name})")),
        )
        for name in _REQUIRED_GENERATIONAL_TABLES
    }
    indexes = {
        name: (
            tuple(connection.execute(
                "SELECT tbl_name,sql FROM sqlite_schema WHERE type='index' AND name=?", (name,)
            ).fetchone()),
            tuple(tuple(row) for row in connection.execute(f"PRAGMA index_xinfo({name})")),
        )
        for name in _REQUIRED_GENERATIONAL_INDEXES
    }
    return tables, indexes


@lru_cache(maxsize=1)
def _expected_generational_shapes() -> tuple[dict[str, tuple[Any, ...]], dict[str, tuple[Any, ...]]]:
    """Generate the required signatures once from the compiler's own DDL."""
    with closing(sqlite3.connect(":memory:")) as connection:
        connection.executescript(DDL)
        connection.executescript(INDEX_DDL)
        return _generational_shapes(connection)



def connect(path: Path, read_only: bool = False) -> sqlite3.Connection:
    if read_only:
        connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=30.0)
        connection.execute("PRAGMA query_only=ON")
    else:
        connection = sqlite3.connect(path, timeout=30.0)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys=ON")
    connection.execute("PRAGMA busy_timeout=30000")
    return connection


def _bootstrap_compiled_connection(connection: sqlite3.Connection) -> None:
    """Initialize a supplied compiled-database connection without a filesystem path."""
    connection.execute("PRAGMA journal_mode=OFF")
    connection.execute("PRAGMA synchronous=OFF")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("PRAGMA cache_size=-65536")
    connection.executescript(DDL)


def _verify_compiled_connection(connection: sqlite3.Connection) -> None:
    if connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
        raise RuntimeError("SQLite integrity check failed")
    if connection.execute("PRAGMA foreign_key_check").fetchone() is not None:
        raise RuntimeError("SQLite foreign-key check failed")


@dataclass(frozen=True)
class AuthoringByteResult:
    """Bounded, file-free compiled cache material for authoring transactions."""

    status: str
    report: dict[str, Any]
    world_bytes: bytes | None = None
    revision_bytes: bytes | None = None
    vector_bytes: bytes | None = None


_MIB = 1024 * 1024
_AUTHORING_DATABASE_LIMIT = 64 * _MIB
_AUTHORING_AGGREGATE_LIMIT = 96 * _MIB
_AUTHORING_PROCESS_LIMIT = 512 * _MIB
# At vector serialization, both capped SQLite page stores and the completed
# world image remain live alongside vector serialization and its verification
# connection. Allow three 64 MiB image/probe generations plus 64 MiB for
# projection rows and connection machinery; caller graphs are added separately.
_AUTHORING_PRECONSTRUCTION_FIXED = (2 * 64 + 3 * 64 + 64) * _MIB
_AUTHORING_SOURCE_GRAPH_COPIES = 4
_AUTHORING_TREE_ENTRY_LIMIT = 4096
_AUTHORING_TREE_COUNT_LIMIT = 131072
_AUTHORING_TREE_OUTPUT_LIMIT = 8 * _MIB


def _authoring_source_metadata_bytes(repository: Repository, resolved: str) -> int | None:
    """Estimate source bytes from metadata without constructing source records."""
    try:
        if repository.is_git and resolved != "WORKTREE":
            total = 0
            count = 0
            output_bytes = 0
            buffer = bytearray()
            process = subprocess.Popen(
                ["git", "-C", str(repository.root), "ls-tree", "-r", "-l", "-z", resolved, "--", repository.source_root],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            )
            try:
                assert process.stdout is not None
                while chunk := process.stdout.read(65536):
                    output_bytes += len(chunk)
                    if output_bytes > _AUTHORING_TREE_OUTPUT_LIMIT:
                        return None
                    buffer.extend(chunk)
                    while (separator := buffer.find(0)) >= 0:
                        if separator > _AUTHORING_TREE_ENTRY_LIMIT:
                            return None
                        entry = bytes(buffer[:separator])
                        del buffer[:separator + 1]
                        count += 1
                        if count > _AUTHORING_TREE_COUNT_LIMIT:
                            return None
                        metadata, raw_path = entry.split(b"\t", 1)
                        fields = metadata.split()
                        if len(fields) != 4:
                            return None
                        if fields[1] == b"blob" and raw_path.endswith(b".md"):
                            total += int(fields[3]) + len(raw_path)
                            if total > _AUTHORING_PROCESS_LIMIT:
                                return total
                    if len(buffer) > _AUTHORING_TREE_ENTRY_LIMIT:
                        return None
                if buffer or process.wait() != 0:
                    return None
                # Repository._tree_entries retains stdout and split entries.
                # Price that representation as well as the source graph.
                return total + 8 * output_bytes
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                if process.stdout is not None:
                    process.stdout.close()
        root = repository.root / repository.source_root
        total = 0
        for path in root.rglob("*.md"):
            total += path.stat().st_size + len(path.relative_to(repository.root).as_posix().encode())
            if total > _AUTHORING_PROCESS_LIMIT:
                return total
        return total
    except (OSError, ValueError, UnicodeError):
        # If the source listing cannot prove the bound, defer before allocation.
        return None


def _authoring_preconstruction_bytes(repository: Repository, resolved: str) -> int | None:
    """Price source, profile metadata and projection work before SQLite setup."""
    source_bytes = _authoring_source_metadata_bytes(repository, resolved)
    if source_bytes is None:
        return None
    metadata_bytes = 0
    try:
        for path in (
            repository.root / ".wedl" / "world.sqlite",
            repository.root / ".wedl" / "vector-cache-v2.sqlite",
        ):
            if path.is_file():
                size = path.stat().st_size
                if size > _AUTHORING_DATABASE_LIMIT:
                    return None
                metadata_bytes += size
    except OSError:
        return None
    # Existing profile/vector metadata may be parsed while a new world and its
    # projection rows coexist, so account for both graph and transfer copies.
    return (
        _AUTHORING_PRECONSTRUCTION_FIXED
        + _AUTHORING_SOURCE_GRAPH_COPIES * source_bytes
        + 2 * metadata_bytes
    )


def _authoring_preconstruction_admitted(required_bytes: int | None) -> bool:
    """Require headroom for allocator bookkeeping at the hard process limit."""
    return required_bytes is not None and required_bytes < _AUTHORING_PROCESS_LIMIT


def _serialize_authoring_connection(connection: sqlite3.Connection) -> bytes | None:
    """Return verified SQLite bytes only when both memory APIs are available."""
    serialize = getattr(connection, "serialize", None)
    support_probe = sqlite3.connect(":memory:")
    try:
        deserialize = getattr(support_probe, "deserialize", None)
    finally:
        support_probe.close()
    if not callable(serialize) or not callable(deserialize):
        return None
    try:
        data = serialize()
        probe = sqlite3.connect(":memory:")
        try:
            probe.deserialize(data)
            if probe.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                return None
            if probe.execute("PRAGMA foreign_key_check").fetchone() is not None:
                return None
        finally:
            probe.close()
        return data
    except (sqlite3.Error, MemoryError, OverflowError):
        return None


def compile_world_bytes(repository: Repository, revision: str = "HEAD", *, caller_live_bytes: int = 0) -> AuthoringByteResult:
    """Compile the state projection in memory, or defer when SQLite cannot serialize."""
    # Resolve and price all inputs before constructing either in-memory SQLite
    # database. An exact 512 MiB estimate has no allocator headroom.
    try:
        resolved = repository.resolve(revision)
        required_bytes = _authoring_preconstruction_bytes(repository, resolved)
    except (MemoryError, OverflowError, OSError, sqlite3.Error) as exc:
        return AuthoringByteResult(
            "deferred-to-restart",
            {
                "status": "deferred-to-restart",
                "reason": "authoring-preconstruction-memory-limit",
                "diagnostic": f"{type(exc).__name__}: {exc}",
            },
        )
    if not isinstance(caller_live_bytes, int) or caller_live_bytes < 0 or not _authoring_preconstruction_admitted(
        required_bytes + caller_live_bytes if required_bytes is not None else None
    ):
        return AuthoringByteResult(
            "deferred-to-restart",
            {
                "status": "deferred-to-restart",
                "reason": "authoring-preconstruction-memory-limit",
            },
        )
    connection: sqlite3.Connection | None = None
    try:
        connection = sqlite3.connect(":memory:")
        connection.row_factory = sqlite3.Row
        started = time.perf_counter()
        if not callable(getattr(connection, "serialize", None)) or not callable(getattr(connection, "deserialize", None)):
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"})
        timings: dict[str, float] = {}
        stage = time.perf_counter()
        page_size = int(connection.execute("PRAGMA page_size").fetchone()[0])
        connection.execute(f"PRAGMA max_page_count={_AUTHORING_DATABASE_LIMIT // page_size}")
        timings["resolveRevision"] = (time.perf_counter() - stage) * 1000
        stage = time.perf_counter()
        world = repository.load_world(resolved, cache_write=False)
        timings["loadSource"] = (time.perf_counter() - stage) * 1000
        stage = time.perf_counter()
        diagnostics = validate_world(world)
        timings["validate"] = (time.perf_counter() - stage) * 1000
        errors = [item for item in diagnostics if item["severity"] == "error"]
        if errors:
            raise ValidationFailed(f"world validation failed with {len(errors)} error(s)", diagnostics)
        # Authoring applies are ordinary commits from the compiler's point of
        # view: retain the selected shared-cache profile just as compile_world
        # does.  The existing database is read only; the new database remains
        # entirely in memory until its final surfaces are enrolled.
        previous = database_meta(repository.root / ".wedl" / "world.sqlite")
        if previous.get("profile_json"):
            try:
                previous_profile = json.loads(str(previous["profile_json"]))
                if not isinstance(previous_profile, dict):
                    raise ValueError("invalid cached compilation profile")
                profile = resolve_profile(
                    world,
                    profile_name=str(previous_profile.get("name") or "hybrid"),
                    vector_provider=previous_profile.get("vectorProvider"),
                    vector_model=previous_profile.get("vectorModel"),
                    vector_dimensions=previous_profile.get("vectorDimensions"),
                    vector_max_features=previous_profile.get("vectorMaxFeatures"),
                )
            except (ValueError, TypeError, AttributeError, RecursionError, UsageError):
                return AuthoringByteResult(
                    "deferred-to-restart",
                    {"status": "deferred-to-restart", "reason": "authoring-cache-profile-invalid"},
                )
        else:
            profile = resolve_profile(world)
        build_mode = "full"
        changed: list[str] = []
        if previous.get("head_commit") and repository.is_ancestor(
            str(previous["head_commit"]), world.revision
        ):
            build_mode = "fast-forward-rebuild"
            changed = repository.changed_paths(str(previous["head_commit"]), world.revision)
        stage = time.perf_counter()
        _bootstrap_compiled_connection(connection)
        timings["schema"] = (time.perf_counter() - stage) * 1000
        connection.execute("BEGIN IMMEDIATE")
        vector_connection = sqlite3.connect(":memory:")
        vector_page_size = int(vector_connection.execute("PRAGMA page_size").fetchone()[0])
        vector_connection.execute(f"PRAGMA max_page_count={_AUTHORING_DATABASE_LIMIT // vector_page_size}")
        if not _load_authoring_vector_cache(repository, vector_connection):
            return AuthoringByteResult(
                "deferred-to-restart",
                {"status": "deferred-to-restart", "reason": "authoring-vector-cache-size-limit"},
            )
        current, stats = _populate_full_compiled_connection(
            connection, world, repository, profile, fingerprint(world, profile), build_mode, timings,
            vector_cache_connection=vector_connection,
        )
        connection.commit()
        _verify_compiled_connection(connection)
        pages = int(connection.execute("PRAGMA page_count").fetchone()[0])
        if pages * page_size > _AUTHORING_DATABASE_LIMIT:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "authoring-cache-size-limit"})
        data = _serialize_authoring_connection(connection)
        if data is None:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"})
        if len(data) > _AUTHORING_DATABASE_LIMIT:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "authoring-cache-size-limit"})
        vector_connection.commit()
        vector_data = _serialize_authoring_connection(vector_connection) if profile.vector_enabled else None
        if profile.vector_enabled and vector_data is None:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"})
        vector_pages = int(vector_connection.execute("PRAGMA page_count").fetchone()[0])
        if vector_pages * vector_page_size > _AUTHORING_DATABASE_LIMIT:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "authoring-cache-size-limit"})
        if vector_data is not None and len(vector_data) > _AUTHORING_DATABASE_LIMIT:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "authoring-cache-size-limit"})
        final_buffers = {id(value): value for value in (data, vector_data) if value is not None}
        if sum(len(value) for value in final_buffers.values()) > _AUTHORING_AGGREGATE_LIMIT:
            return AuthoringByteResult("deferred-to-restart", {"status": "deferred-to-restart", "reason": "authoring-cache-aggregate-size-limit"})
        timings["total"] = (time.perf_counter() - started) * 1000
        report = _compiled_report(
            world=world,
            database=repository.root / ".wedl" / "world.sqlite",
            database_bytes=len(data),
            build_mode=build_mode,
            changed=changed,
            profile=profile,
            current=current,
            stats=stats,
            timings=timings,
            source_load=dict(repository.last_load_stats),
        )
        return AuthoringByteResult("compiled", report, data, data, vector_data)
    except (MemoryError, OverflowError, sqlite3.Error) as exc:
        # Cache products are optional during authoring. SQLite's capacity
        # failures must deterministically fall back before any final cache
        # surface is constructed or enrolled.
        return AuthoringByteResult(
            "deferred-to-restart",
            {
                "status": "deferred-to-restart",
                "reason": "authoring-cache-build-capacity-limit",
                "diagnostic": f"{type(exc).__name__}: {exc}",
            },
        )
    finally:
        if connection is not None:
            connection.close()
        if 'vector_connection' in locals():
            vector_connection.close()


def _populate_compiled_connection(connection: sqlite3.Connection, world: World, timings: dict[str, float]) -> StoryTime:
    """Populate the shared base projections on an initialized connection."""
    for name, insert in (("entities", _insert_entities), ("threads", _insert_threads), ("narrative", _insert_narrative)):
        stage = time.perf_counter(); insert(connection, world)
        timings[name] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter(); current = _insert_derived(connection, world)
    timings["derived"] = (time.perf_counter() - stage) * 1000
    return current


def cache_paths(repository: Repository) -> tuple[Path, Path, Path]:
    cache = repository.root / ".wedl"
    revisions = cache / "revisions"
    return cache, cache / "world.sqlite", revisions


def _revision_cache_path(repository: Repository, revisions: Path, revision: str) -> Path:
    return revisions / f"{revision}.sqlite"


def _revision_cache_files(repository: Repository, revisions: Path) -> list[Path]:
    return list(revisions.glob("*.sqlite"))


def vector_cache(repository: Repository) -> sqlite3.Connection:
    """Open the disposable content-addressed normalized-vector cache."""
    cache, _database, _revisions = cache_paths(repository)
    path = cache / "vector-cache-v2.sqlite"
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(path, timeout=30.0)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA synchronous=OFF")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("PRAGMA cache_size=-32768")
    connection.executescript(
        """
        CREATE TABLE IF NOT EXISTS vector_model_cache(
          cache_key TEXT PRIMARY KEY,
          model_id TEXT NOT NULL,
          provider TEXT NOT NULL,
          model_name TEXT NOT NULL,
          dimensions INTEGER NOT NULL,
          normalized INTEGER NOT NULL,
          corpus_hash TEXT,
          config_json TEXT NOT NULL,
          model_blob BLOB
        );
        CREATE TABLE IF NOT EXISTS vector_value_cache(
          cache_key TEXT NOT NULL,
          input_hash TEXT NOT NULL,
          dimensions INTEGER NOT NULL,
          norm REAL NOT NULL,
          vector BLOB NOT NULL,
          PRIMARY KEY(cache_key,input_hash)
        );
        """
    )
    return connection


def _bootstrap_authoring_vector_connection(connection: sqlite3.Connection) -> None:
    """Initialize a file-free vector-cache connection for bounded authoring work."""
    connection.execute("PRAGMA journal_mode=OFF")
    connection.execute("PRAGMA synchronous=OFF")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("PRAGMA cache_size=-32768")
    connection.executescript(
        "CREATE TABLE vector_model_cache(cache_key TEXT PRIMARY KEY,model_id TEXT NOT NULL,provider TEXT NOT NULL,model_name TEXT NOT NULL,dimensions INTEGER NOT NULL,normalized INTEGER NOT NULL,corpus_hash TEXT,config_json TEXT NOT NULL,model_blob BLOB);"
        "CREATE TABLE vector_value_cache(cache_key TEXT NOT NULL,input_hash TEXT NOT NULL,dimensions INTEGER NOT NULL,norm REAL NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(cache_key,input_hash));"
    )


def _load_authoring_vector_cache(
    repository: Repository,
    connection: sqlite3.Connection,
) -> bool:
    """Copy a bounded final vector cache into an in-memory working database.

    The shared cache is opened query-only and is never mutated by the
    authoring path. A failed or oversized copy is a cache deferral signal,
    rather than a reason to create a private file-backed cache.
    """
    connection.row_factory = sqlite3.Row
    path = repository.root / ".wedl" / "vector-cache-v2.sqlite"
    if not path.is_file():
        _bootstrap_authoring_vector_connection(connection)
        return True
    try:
        with closing(connect(path, True)) as source:
            page_size = int(source.execute("PRAGMA page_size").fetchone()[0])
            pages = int(source.execute("PRAGMA page_count").fetchone()[0])
            if pages * page_size > 64 * 1024 * 1024:
                return False
            connection.execute(
                f"PRAGMA max_page_count={64 * 1024 * 1024 // page_size}"
            )
            source.backup(connection)
        # A legacy final cache can have a WAL-format header even though the
        # read-only backup contains its checkpointed rows. Rebuild the memory
        # database after disabling WAL so serialized bytes never require a
        # sibling ``-wal`` file to reopen.
        connection.execute("PRAGMA journal_mode=DELETE")
        connection.execute("VACUUM")
        connection.execute("PRAGMA journal_mode=OFF")
        connection.execute("PRAGMA synchronous=OFF")
        connection.execute("PRAGMA temp_store=MEMORY")
        connection.execute("PRAGMA cache_size=-32768")
        return True
    except sqlite3.Error:
        return False


def fingerprint(world: World, profile: CompilationProfile) -> str:
    material = canonical_json({
        "source": world.schema,
        "capabilities": list(world.world_record.frontmatter.get("capabilities") or ()) if world.schema == V07_SOURCE_SCHEMA else [],
        "sqlite": SQLITE_SCHEMA,
        "version": __version__,
        "profile": profile.as_dict(),
        "worldStateKeys": world.config.get("state_keys") or {},
        "relationshipMetrics": world.config.get("relationship_metrics") or {},
        "threads": [
            {"id": thread.id, "label": thread.label}
            for thread in sorted(world.threads, key=lambda item: item.id)
        ],
        "recordThreads": [
            {"recordId": record.id, "threads": sorted(record.thread_ids)}
            for record in sorted(world.records.values(), key=lambda item: item.id)
            if record.kind not in {"world", "hypothesis"} and record.thread_ids
        ],
    })
    return COMPILER_FINGERPRINT_PREFIX + hashlib.sha256(material.encode()).hexdigest()

def database_meta(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        with closing(connect(path, True)) as connection:
            row = connection.execute("SELECT * FROM revision LIMIT 1").fetchone()
            return dict(row) if row else {}
    except sqlite3.Error:
        return {}


def _compiled_database_issues(path: Path) -> tuple[str, ...]:
    """Return deterministic integrity failures for a disposable read model."""
    if not path.exists():
        return ("databaseMissing",)
    try:
        with closing(connect(path, True)) as connection:
            quick = tuple(str(row[0]) for row in connection.execute("PRAGMA quick_check"))
            if quick != ("ok",):
                return ("databaseIntegrity",)
            tables = {
                str(row[0])
                for row in connection.execute(
                    "SELECT name FROM sqlite_schema WHERE type='table'"
                )
            }
            indexes = {
                str(row[0])
                for row in connection.execute(
                    "SELECT name FROM sqlite_schema WHERE type='index'"
                )
            }
            issues = [f"missingTable:{name}" for name in sorted(_REQUIRED_SPATIAL_TABLES - tables)]
            issues.extend(f"missingIndex:{name}" for name in sorted(_REQUIRED_SPATIAL_INDEXES - indexes))
            issues.extend(f"missingTable:{name}" for name in sorted(_REQUIRED_GENERATIONAL_TABLES - tables))
            issues.extend(f"missingIndex:{name}" for name in sorted(_REQUIRED_GENERATIONAL_INDEXES - indexes))
            if issues:
                return tuple(issues)
            expected_tables, expected_indexes = _expected_generational_shapes()
            actual_tables, actual_indexes = _generational_shapes(connection)
            issues.extend(f"tableShape:{name}" for name in sorted(expected_tables)
                          if actual_tables[name] != expected_tables[name])
            issues.extend(f"indexShape:{name}" for name in sorted(expected_indexes)
                          if actual_indexes[name] != expected_indexes[name])
            if issues:
                return tuple(issues)
            if connection.execute("PRAGMA foreign_key_check").fetchone() is not None:
                issues.append("foreignKeyIntegrity")
            invalid_portal = connection.execute(
                """
                SELECT 1 FROM spatial_portal
                WHERE NOT (
                  (target_kind='location' AND target_location_id IS NOT NULL AND target_map_id IS NULL AND target_coordinates_json IS NULL)
                  OR
                  (target_kind='position' AND target_location_id IS NULL AND target_map_id IS NOT NULL AND target_coordinates_json IS NOT NULL)
                )
                LIMIT 1
                """
            ).fetchone()
            if invalid_portal is not None:
                issues.append("spatialPortalTargetUnion")
            return tuple(issues)
    except sqlite3.Error:
        return ("databaseIntegrity",)


def cache_readiness(repository: Repository, revision: str = "HEAD") -> dict[str, Any]:
    """Inspect the compiled cache without creating, rebuilding, or mutating it.

    The returned target is the source revision a read would query now.  Clients
    that need no implicit writes can use this result (or ``require_database``
    with ``require_compiled=True``) to decide whether to run ``wedl compile``.
    """
    _cache, database, _revisions = cache_paths(repository)
    resolved = repository.resolve(revision)
    tree_oid = repository.tree_oid(resolved)
    target = {"revision": resolved, "treeOid": tree_oid}
    if not database.exists():
        return {
            "state": "missing",
            "reason": "compiled database does not exist",
            "database": str(database),
            "target": target,
        }
    meta = database_meta(database)
    if not meta:
        return {
            "state": "incompatible",
            "reason": "compiled database has no readable revision metadata",
            "database": str(database),
            "target": target,
        }
    incompatible_fields = [
        field
        for field, expected, actual in (
            ("sourceSchema", "supported source schema", meta.get("source_schema")),
            ("sqliteSchema", SQLITE_SCHEMA, meta.get("sqlite_schema")),
            ("compilerVersion", __version__, meta.get("compiler_version")),
        )
        if (actual not in COMPILED_SOURCE_SCHEMAS if field == "sourceSchema" else actual != expected)
    ]
    if not str(meta.get("compiler_fingerprint") or "").startswith(COMPILER_FINGERPRINT_PREFIX):
        incompatible_fields.append("compilerFingerprint")
    compiled = {
        "revision": meta.get("head_commit"),
        "treeOid": meta.get("tree_oid"),
        "sourceSchema": meta.get("source_schema"),
        "sqliteSchema": meta.get("sqlite_schema"),
        "compilerVersion": meta.get("compiler_version"),
        "compilerFingerprint": meta.get("compiler_fingerprint"),
    }
    if incompatible_fields:
        return {
            "state": "incompatible",
            "reason": "compiled database is incompatible with this WEDL version or schema",
            "incompatibleFields": incompatible_fields,
            "database": str(database),
            "target": target,
            "compiled": compiled,
        }
    integrity_issues = _compiled_database_issues(database)
    if integrity_issues:
        return {
            "state": "incompatible",
            "reason": "compiled database failed structural or integrity checks",
            "incompatibleFields": list(integrity_issues),
            "database": str(database),
            "target": target,
            "compiled": compiled,
        }
    if meta.get("head_commit") != resolved or meta.get("tree_oid") != tree_oid:
        return {
            "state": "stale",
            "reason": "compiled database does not match the requested source revision",
            "database": str(database),
            "target": target,
            "compiled": compiled,
        }
    return {
        "state": "ready",
        "reason": None,
        "database": str(database),
        "target": target,
        "compiled": compiled,
    }


def _insert_entities(connection: sqlite3.Connection, world: World) -> None:
    records = sorted(world.records.values(), key=lambda item: item.source_path)
    connection.executemany(
        "INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)",
        [
            (
                record.id, record.kind, record.title, record.domain, record.status,
                record.source_path, record.blob_oid, record.body, canonical_json(record.frontmatter),
            )
            for record in records
        ],
    )
    tags = [(record.id, tag) for record in records for tag in dict.fromkeys(record.tags)]
    aliases = [(record.id, alias) for record in records for alias in dict.fromkeys(record.aliases)]
    refs = [
        (record.id, target, "source")
        for record in records
        for target in sorted(extract_entity_refs(record.frontmatter) | markdown_entity_links(record.body))
        if target != record.id
    ]
    if tags:
        connection.executemany("INSERT INTO entity_tag VALUES (?,?)", tags)
    if aliases:
        connection.executemany("INSERT INTO entity_alias VALUES (?,?)", aliases)
    if refs:
        connection.executemany("INSERT OR IGNORE INTO entity_ref VALUES (?,?,?)", refs)


def _insert_threads(connection: sqlite3.Connection, world: World) -> None:
    threads = sorted(world.threads, key=lambda thread: thread.id)
    if threads:
        connection.executemany(
            "INSERT INTO narrative_thread VALUES (?,?)",
            [(thread.id, thread.label) for thread in threads],
        )
    memberships = [
        (record.id, thread_id)
        for record in sorted(world.records.values(), key=lambda item: item.id)
        if record.kind not in {"world", "hypothesis"}
        for thread_id in sorted(record.thread_ids)
    ]
    if memberships:
        connection.executemany("INSERT INTO record_thread VALUES (?,?)", memberships)


def _insert_narrative(connection: sqlite3.Connection, world: World) -> None:
    for record in sorted(world.records.values(), key=lambda item: item.id):
        data = record.frontmatter
        if record.kind == "event":
            point = StoryTime.from_value(data.get("time"), world.default_timeline)
            connection.execute("INSERT INTO event VALUES (?,?,?,?,?,?)", (record.id, point.timeline, point.tick, point.order, data.get("location"), record.status))
            for index, participant in enumerate(data.get("participants") or []):
                connection.execute("INSERT INTO event_participant VALUES (?,?,?,?)", (record.id, participant.get("character"), participant.get("role"), index))
            for index, effect in enumerate(data.get("effects") or []):
                connection.execute("INSERT INTO state_effect VALUES (?,?,?,?,?,?,?)", (effect.get("id"), record.id, effect.get("target"), effect.get("key"), effect.get("operation"), canonical_json(effect.get("value")) if "value" in effect else None, index))
        elif record.kind == "knowledge":
            claim = data.get("claim") or {}
            connection.execute("INSERT INTO knowledge VALUES (?,?,?,?,?)", (record.id, data.get("knower"), claim.get("key"), claim.get("statement"), canonical_json(claim)))
            for index, transition in enumerate(data.get("transitions") or []):
                point = StoryTime.from_value(transition.get("time"), world.default_timeline)
                connection.execute("INSERT INTO knowledge_transition VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", (transition.get("id"), record.id, point.timeline, point.tick, point.order, transition.get("state"), transition.get("confidence"), transition.get("acquisition"), transition.get("causing_event"), transition.get("source_entity"), transition.get("note"), index))
        elif record.kind == "relationship":
            connection.execute("INSERT INTO relationship VALUES (?,?,?,?,?)", (record.id, data.get("from"), data.get("to"), data.get("relationship_kind"), data.get("inverse")))
            for index, transition in enumerate(data.get("transitions") or []):
                point = StoryTime.from_value(transition.get("time"), world.default_timeline)
                connection.execute("INSERT INTO relationship_transition VALUES (?,?,?,?,?,?,?,?,?,?,?)", (transition.get("id"), record.id, point.timeline, point.tick, point.order, transition.get("relationship_status"), canonical_json(transition.get("metrics") or {}), canonical_json(transition.get("facets") or []), transition.get("causing_event"), transition.get("note"), index))
        elif record.kind == "scene":
            time_range = data.get("time") or {}
            start = StoryTime.from_value(time_range.get("start"), world.default_timeline)
            current = StoryTime.from_value(time_range.get("current"), world.default_timeline)
            end = StoryTime.from_value(time_range["end"], world.default_timeline) if time_range.get("end") else None
            connection.execute("INSERT INTO scene VALUES (?,?,?,?,?,?,?,?,?,?)", (record.id, record.status, start.timeline, start.tick, start.order, current.tick, current.order, end.tick if end else None, end.order if end else None, data.get("location")))
            for participant in data.get("participants") or []:
                from_point = StoryTime.from_value(participant.get("from") or time_range.get("start"), world.default_timeline)
                to_value = participant.get("to") or time_range.get("end")
                to_point = StoryTime.from_value(to_value, world.default_timeline) if to_value else None
                connection.execute("INSERT INTO scene_participant VALUES (?,?,?,?,?,?,?,?)", (record.id, participant.get("character"), participant.get("role"), int(bool(participant.get("point_of_view"))), from_point.tick, from_point.order, to_point.tick if to_point else None, to_point.order if to_point else None))
            for observation in data.get("observations") or []:
                point = StoryTime.from_value(observation.get("at"), world.default_timeline)
                end_value = observation.get("until")
                until = StoryTime.from_value(end_value, world.default_timeline) if end_value else None
                connection.execute("INSERT INTO scene_observation VALUES (?,?,?,?,?,?,?,?,?,?)", (observation.get("id"), record.id, canonical_json(observation.get("audience") or ["participants"]), point.timeline, point.tick, point.order, until.tick if until else None, until.order if until else None, observation.get("text"), float(observation.get("salience", 1.0))))
        elif record.kind == "conversation":
            time_range = data.get("time") or {}
            start = StoryTime.from_value(time_range.get("start"), world.default_timeline)
            end = StoryTime.from_value(time_range["end"], world.default_timeline) if time_range.get("end") else None
            connection.execute("INSERT INTO conversation VALUES (?,?,?,?,?,?,?,?,?)", (record.id, record.status, data.get("scene"), data.get("location"), start.timeline, start.tick, start.order, end.tick if end else None, end.order if end else None))
            for participant in data.get("participants") or []:
                from_point = StoryTime.from_value(participant.get("from") or time_range.get("start"), world.default_timeline)
                to_value = participant.get("to") or time_range.get("end")
                to_point = StoryTime.from_value(to_value, world.default_timeline) if to_value else None
                connection.execute("INSERT INTO conversation_participant VALUES (?,?,?,?,?,?,?)", (record.id, participant.get("character"), participant.get("role"), from_point.tick, from_point.order, to_point.tick if to_point else None, to_point.order if to_point else None))
            for turn in data.get("turns") or []:
                point = turn_time(turn, record, world.default_timeline)
                metadata = {key: value for key, value in turn.items() if key not in {"id", "at", "speaker", "text", "delivery", "audience"}}
                connection.execute("INSERT INTO conversation_turn VALUES (?,?,?,?,?,?,?,?,?,?)", (turn.get("id"), record.id, point.timeline, point.tick, point.order, turn.get("speaker"), turn.get("text"), turn.get("delivery"), canonical_json(turn.get("audience") or ["participants"]), canonical_json(metadata)))
            for recollection in data.get("recollections") or []:
                point = StoryTime.from_value(recollection.get("at"), world.default_timeline)
                connection.execute("INSERT INTO conversation_recollection VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)", (recollection.get("id"), record.id, recollection.get("character"), point.timeline, point.tick, point.order, recollection.get("state", "remembered"), recollection.get("summary"), recollection.get("interpretation"), recollection.get("emotional_impression"), recollection.get("confidence"), canonical_json(recollection.get("exact_turns") or []), canonical_json(recollection.get("remembered_quotes") or [])))
        elif record.kind == "story-point":
            lifecycle = data.get("lifecycle") or {}
            connection.execute("INSERT INTO story_point VALUES (?,?,?,?,?,?,?)", (record.id, lifecycle.get("initial_state"), data.get("activation_policy"), int(data.get("priority", 0)), data.get("repeat_policy"), canonical_json(data.get("trigger") or {}), canonical_json(data.get("dependencies") or {})))
            for index, transition in enumerate(lifecycle.get("transitions") or []):
                point = StoryTime.from_value(transition.get("time"), world.default_timeline)
                connection.execute("INSERT INTO story_point_transition VALUES (?,?,?,?,?,?,?,?,?)", (transition.get("id"), record.id, point.timeline, point.tick, point.order, transition.get("state"), transition.get("causing_event"), transition.get("note"), index))


def _insert_derived(connection: sqlite3.Connection, world: World) -> StoryTime:
    at = effective_time(world)
    for record in world.records.values():
        if record.kind not in {"character", "object", "location"}:
            continue
        state, citations = resolve_state(world, record.id, at)
        for key, value in state.items():
            connection.execute("INSERT INTO current_state VALUES (?,?,?,?)", (record.id, key, canonical_json(value), canonical_json(citations.get(key))))
    for character in world.by_kind("character"):
        for item in current_knowledge(world, character.id, at):
            connection.execute("INSERT INTO current_knowledge VALUES (?,?,?,?,?,?)", (item["knowledgeId"], character.id, item["claimKey"], item["state"], item.get("confidence"), canonical_json({key: value for key, value in item.items() if key != "record"})))
        for item in relationships_from(world, character.id, at):
            connection.execute("INSERT INTO current_relationship VALUES (?,?,?,?,?,?,?)", (item["relationshipId"], item["from"], item["to"], item["status"], canonical_json(item["metrics"]), canonical_json(item["facets"]), canonical_json({key: value for key, value in item.items() if key != "record"})))
    scene = world.active_scene()
    for item in evaluate_all_story_points(world, at, scene):
        connection.execute("INSERT INTO story_point_current VALUES (?,?,?,?,?)", (item["storyPointId"], item["storedState"], item["derivedState"], int(item["eligible"]), canonical_json(item)))
    # Build interaction rows in one event pass. Calling `interactions()` for
    # every pair/event combination rescanned all events and all consequences,
    # becoming quadratic on long stories.
    knowledge_by_event: dict[str, list[str]] = {}
    relationship_by_event: dict[str, list[str]] = {}
    for record in world.by_kind("knowledge"):
        for transition in record.frontmatter.get("transitions") or []:
            if isinstance(transition, dict) and transition.get("causing_event"):
                knowledge_by_event.setdefault(str(transition["causing_event"]), []).append(record.id)
    for record in world.by_kind("relationship"):
        for transition in record.frontmatter.get("transitions") or []:
            if isinstance(transition, dict) and transition.get("causing_event"):
                relationship_by_event.setdefault(str(transition["causing_event"]), []).append(record.id)
    interaction_rows: list[tuple[Any, ...]] = []
    for event in canonical_events(world, at):
        participants = [
            item for item in event.frontmatter.get("participants") or []
            if isinstance(item, dict) and item.get("character")
        ]
        roles = {str(item["character"]): item.get("role") for item in participants}
        point = event_time(event, world.default_timeline)
        for first, second in combinations(sorted(roles), 2):
            value = {
                "eventId": event.id,
                "title": event.title,
                "time": point.to_dict(),
                "locationId": event.frontmatter.get("location"),
                "roles": {first: roles[first], second: roles[second]},
                "knowledgeConsequences": sorted(set(knowledge_by_event.get(event.id, []))),
                "relationshipConsequences": sorted(set(relationship_by_event.get(event.id, []))),
            }
            interaction_rows.append(
                (event.id, first, second, point.timeline, point.tick, point.order, canonical_json(value))
            )
    if interaction_rows:
        connection.executemany(
            "INSERT OR IGNORE INTO character_interaction VALUES (?,?,?,?,?,?,?)",
            interaction_rows,
        )
    return at


def _vector_cache_key(
    profile: CompilationProfile,
    scope: str,
    training_hashes: list[str],
    provider_config: dict[str, Any],
) -> str:
    material: dict[str, Any] = {
        "profile": profile.as_dict(),
        "providerConfig": {
            key: value
            for key, value in provider_config.items()
            if key not in {"api_key", "token", "authorization"}
        },
    }
    if profile.vector_provider == "lsa":
        material["scope"] = scope
        material["corpus"] = sorted(training_hashes)
    return hashlib.sha256(canonical_json(material).encode()).hexdigest()


def _cached_model(cache: sqlite3.Connection, cache_key: str) -> VectorModel | None:
    row = cache.execute(
        "SELECT model_id,provider,model_name,dimensions,normalized,corpus_hash,config_json,model_blob "
        "FROM vector_model_cache WHERE cache_key=?",
        (cache_key,),
    ).fetchone()
    return model_from_row(row) if row else None


def _cached_vectors(
    cache: sqlite3.Connection,
    cache_key: str,
    input_hashes: list[str],
) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for start in range(0, len(input_hashes), 400):
        batch = input_hashes[start : start + 400]
        if not batch:
            continue
        placeholders = ",".join("?" for _ in batch)
        rows = cache.execute(
            f"SELECT input_hash,dimensions,vector FROM vector_value_cache "
            f"WHERE cache_key=? AND input_hash IN ({placeholders})",
            [cache_key, *batch],
        ).fetchall()
        result.update(
            {
                str(row[0]): blob_to_vector(bytes(row[2]), int(row[1]))
                for row in rows
            }
        )
    return result


def _persist_vector_build(
    cache: sqlite3.Connection,
    cache_key: str,
    build: Any,
) -> None:
    model = build.model
    cache.execute(
        "INSERT OR REPLACE INTO vector_model_cache VALUES (?,?,?,?,?,?,?,?,?)",
        (
            cache_key,
            model.model_id,
            model.provider,
            model.model_name,
            model.dimensions,
            int(model.normalized),
            model.corpus_hash,
            canonical_json(model.config),
            model.model_blob,
        ),
    )
    rows = []
    for input_hash, vector in build.vectors.items():
        normalized = normalize_vector(vector)
        norm = float((normalized @ normalized) ** 0.5)
        rows.append(
            (
                cache_key,
                input_hash,
                int(normalized.size),
                norm,
                vector_to_blob(normalized),
            )
        )
    if rows:
        cache.executemany(
            "INSERT OR REPLACE INTO vector_value_cache VALUES (?,?,?,?,?)",
            rows,
        )
    cache.commit()


def _populate_vector_connection(connection: sqlite3.Connection, cache_key: str, build: Any) -> None:
    """Persist one completed vector build into a caller-owned connection only."""
    _persist_vector_build(connection, cache_key, build)


def _load_or_build_vectors(
    repository: Repository,
    profile: CompilationProfile,
    scope: str,
    texts_by_hash: dict[str, str],
    training_hashes: list[str],
    provider_config: dict[str, Any], cache_connection: sqlite3.Connection | None = None,
) -> tuple[VectorModel, dict[str, Any], dict[str, int]]:
    input_hashes = sorted(texts_by_hash)
    cache_key = _vector_cache_key(profile, scope, training_hashes, provider_config)
    cache = cache_connection or vector_cache(repository)
    try:
        model = _cached_model(cache, cache_key)
        cached = _cached_vectors(cache, cache_key, input_hashes) if model else {}
        if model and len(cached) == len(input_hashes):
            return model, cached, {"reused": len(input_hashes), "generated": 0}

        missing = {
            input_hash: texts_by_hash[input_hash]
            for input_hash in input_hashes
            if input_hash not in cached
        }
        if model is not None:
            generated_vectors = embed_documents(model, missing)
            build = type("CachedVectorBuild", (), {
                "model": model,
                "vectors": generated_vectors,
                "cacheable_across_corpora": True,
            })()
        else:
            build = build_vectors(
                texts_by_hash,
                profile,
                provider_config,
                training_hashes=training_hashes,
                scope=scope,
            )
        _persist_vector_build(cache, cache_key, build)
        vectors = {**cached, **build.vectors}
        return build.model, vectors, {
            "reused": len(cached),
            "generated": len(build.vectors),
        }
    finally:
        if cache_connection is None:
            cache.close()


def _insert_search(
    connection: sqlite3.Connection,
    world: World,
    repository: Repository,
    profile: CompilationProfile, vector_cache_connection: sqlite3.Connection | None = None,
) -> dict[str, Any]:
    timings: dict[str, float] = {}
    if profile.name == "state":
        return {
            "documents": 0,
            "ftsDocuments": 0,
            "vectorDocuments": 0,
            "uniqueVectors": 0,
            "vectorLinks": 0,
            "reused": 0,
            "generated": 0,
            "uniqueInputs": 0,
            "timingsMs": {},
            "vectorModels": [],
        }

    stage = time.perf_counter()
    documents = build_documents(world)
    timings["buildDocuments"] = (time.perf_counter() - stage) * 1000
    hashes: list[str] = []
    if profile.vector_enabled:
        stage = time.perf_counter()
        hashes = [sha256_bytes(document.vector_text.encode()) for document in documents]
        timings["hashInputs"] = (time.perf_counter() - stage) * 1000

    models_by_scope: dict[str, VectorModel] = {}
    vectors_by_model_hash: dict[tuple[str, str], Any] = {}
    vector_stats = {"reused": 0, "generated": 0}
    if profile.vector_enabled:
        provider_config = world.config.get("embedding_policy") or {}
        stage = time.perf_counter()
        scoped_indexes = {
            "author": [
                index
                for index, document in enumerate(documents)
                if document.audience_kind == "author"
            ],
            "character": [
                index
                for index, document in enumerate(documents)
                if document.audience_kind != "author"
            ],
        }
        for scope, indexes in scoped_indexes.items():
            if not indexes:
                continue
            texts_by_hash: dict[str, str] = {}
            for index in indexes:
                texts_by_hash.setdefault(hashes[index], documents[index].vector_text)
            if profile.vector_provider == "lsa":
                if scope == "character":
                    training_hashes = sorted(
                        {
                            hashes[index]
                            for index in indexes
                            if documents[index].audience_kind == "public"
                        }
                    )
                    # A world with no globally public prose has no safe
                    # character-trained latent space. Character vector search
                    # remains unavailable rather than allowing private text to
                    # influence another character's ranking.
                    if not training_hashes:
                        continue
                else:
                    training_hashes = sorted(texts_by_hash)
            else:
                training_hashes = sorted(texts_by_hash)
            model, scoped_vectors, stats = _load_or_build_vectors(
                repository,
                profile,
                scope,
                texts_by_hash,
                training_hashes,
                provider_config, vector_cache_connection,
            )
            models_by_scope[scope] = model
            vector_stats["reused"] += stats["reused"]
            vector_stats["generated"] += stats["generated"]
            for input_hash, vector in scoped_vectors.items():
                vectors_by_model_hash[(model.model_id, input_hash)] = vector
            connection.execute(
                "INSERT INTO vector_model VALUES (?,?,?,?,?,?,?,?,?)",
                (
                    scope,
                    model.model_id,
                    model.provider,
                    model.model_name,
                    model.dimensions,
                    int(model.normalized),
                    model.corpus_hash,
                    canonical_json(model.config),
                    model.model_blob,
                ),
            )
        timings["buildVectors"] = (time.perf_counter() - stage) * 1000

    stage = time.perf_counter()
    document_rows: list[tuple[Any, ...]] = []
    fts_rows: list[tuple[Any, ...]] = []
    for rowid, document in enumerate(documents, start=1):
        document_rows.append(
            (
                rowid,
                document.document_id,
                document.entity_id,
                document.document_kind,
                document.heading,
                document.audience_kind,
                document.audience_character_id,
                document.scene_id,
                document.valid_from.timeline if document.valid_from else None,
                document.valid_from.tick if document.valid_from else None,
                document.valid_from.order if document.valid_from else None,
                document.valid_until.tick if document.valid_until else None,
                document.valid_until.order if document.valid_until else None,
                document.text,
                canonical_json(document.metadata),
                document.chunk_hash,
            )
        )
        if profile.fts_enabled:
            fts_rows.append(
                (
                    rowid,
                    document.metadata.get("title", ""),
                    " ".join(document.metadata.get("aliases") or []),
                    document.heading or "",
                    document.text,
                    document.metadata.get("domain", ""),
                    " ".join(document.metadata.get("tags") or []),
                )
            )
    timings["prepareRows"] = (time.perf_counter() - stage) * 1000

    if document_rows:
        stage = time.perf_counter()
        connection.executemany(
            "INSERT INTO search_document(rowid,document_id,entity_id,document_kind,heading,audience_kind,"
            "audience_character_id,scene_id,timeline,from_tick,from_order,until_tick,until_order,text,metadata_json,chunk_hash) "
            "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            document_rows,
        )
        timings["insertDocuments"] = (time.perf_counter() - stage) * 1000
    if fts_rows:
        stage = time.perf_counter()
        connection.executemany(
            "INSERT INTO search_fts(rowid,title,aliases,heading,text,domain,tags) VALUES (?,?,?,?,?,?,?)",
            fts_rows,
        )
        timings["insertFts"] = (time.perf_counter() - stage) * 1000

    unique_vector_count = 0
    vector_link_count = 0
    if profile.vector_enabled and models_by_scope:
        stage = time.perf_counter()
        vector_ids: dict[tuple[str, str], int] = {}
        vector_rows: list[tuple[Any, ...]] = []
        for vector_id, key in enumerate(sorted(vectors_by_model_hash), start=1):
            model_id, input_hash = key
            vector = normalize_vector(vectors_by_model_hash[key])
            norm = float((vector @ vector) ** 0.5)
            vector_ids[key] = vector_id
            vector_rows.append(
                (
                    vector_id,
                    model_id,
                    input_hash,
                    int(vector.size),
                    norm,
                    vector_to_blob(vector),
                )
            )
        if vector_rows:
            connection.executemany(
                "INSERT INTO vector_embedding VALUES (?,?,?,?,?,?)",
                vector_rows,
            )
        link_rows: list[tuple[str, int]] = []
        for document, input_hash in zip(documents, hashes, strict=True):
            scope = "author" if document.audience_kind == "author" else "character"
            model = models_by_scope.get(scope)
            if model is None:
                continue
            vector_id = vector_ids.get((model.model_id, input_hash))
            if vector_id is not None:
                link_rows.append((document.document_id, vector_id))
        if link_rows:
            connection.executemany(
                "INSERT INTO document_vector VALUES (?,?)",
                link_rows,
            )
        unique_vector_count = len(vector_rows)
        vector_link_count = len(link_rows)
        timings["insertVectors"] = (time.perf_counter() - stage) * 1000

    return {
        "documents": len(documents),
        "ftsDocuments": len(fts_rows),
        "vectorDocuments": vector_link_count,
        "uniqueVectors": unique_vector_count,
        "vectorLinks": vector_link_count,
        "reused": vector_stats["reused"],
        "generated": vector_stats["generated"],
        "uniqueInputs": len(set(hashes)) if profile.vector_enabled else 0,
        "timingsMs": {key: round(value, 3) for key, value in timings.items()},
        "vectorModels": [
            {
                "scope": scope,
                "modelId": model.model_id,
                "provider": model.provider,
                "modelName": model.model_name,
                "dimensions": model.dimensions,
                "normalized": model.normalized,
                "corpusHash": model.corpus_hash,
            }
            for scope, model in sorted(models_by_scope.items())
        ],
    }


def _populate_full_compiled_connection(
    connection: sqlite3.Connection,
    world: World,
    repository: Repository,
    profile: CompilationProfile,
    compiler_fingerprint: str,
    build_mode: str,
    timings: dict[str, float],
    *,
    vector_cache_connection: sqlite3.Connection | None = None,
) -> tuple[StoryTime, dict[str, Any]]:
    """Build the complete direct-compiler projection on a supplied connection."""
    stats: dict[str, Any] = {}
    current = _populate_compiled_connection(connection, world, timings)
    stage = time.perf_counter()
    stats.update(insert_chronology_index(connection, build_chronology_projection(world, validated=True)))
    timings["chronology"] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter()
    stats.update(insert_spatial_index(connection, build_spatial_projection(world, validated=True)))
    timings["spatial"] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter()
    stats.update(insert_generational_index(connection, world, current))
    timings["generational"] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter()
    stats.update(_insert_search(connection, world, repository, profile, vector_cache_connection))
    timings["search"] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter()
    connection.executescript(INDEX_DDL)
    timings["indexes"] = (time.perf_counter() - stage) * 1000
    connection.execute(
        "INSERT INTO revision VALUES (?,?,?,?,?,?,?,?,?,?,CURRENT_TIMESTAMP,?)",
        (
            world.revision, world.tree_oid, world.schema, SQLITE_SCHEMA,
            __version__, compiler_fingerprint, profile.name,
            canonical_json(profile.as_dict()), canonical_json(stats.get("vectorModels") or []),
            len(world.records), build_mode,
        ),
    )
    for key, value in timings.items():
        connection.execute("INSERT INTO compile_metric VALUES (?,?,?)", (key, value, "{}"))
    return current, stats


def _compiled_report(
    *,
    world: World,
    database: Path,
    database_bytes: int,
    build_mode: str,
    changed: list[str],
    profile: CompilationProfile,
    current: StoryTime,
    stats: dict[str, Any],
    timings: dict[str, float],
    source_load: dict[str, Any],
) -> dict[str, Any]:
    """Return the public direct-compiler report for any finished connection."""
    return {
        "status": "compiled", "buildMode": build_mode,
        "revision": world.revision, "treeOid": world.tree_oid,
        "database": str(database), "databaseBytes": database_bytes,
        "recordCount": len(world.records), "searchProfile": profile.as_dict(),
        "searchDocumentCount": stats.get("documents", 0),
        "ftsDocumentCount": stats.get("ftsDocuments", 0),
        "vectorDocumentCount": stats.get("vectorDocuments", 0),
        "uniqueVectorCount": stats.get("uniqueVectors", 0),
        "vectorLinkCount": stats.get("vectorLinks", 0),
        "vectorModels": stats.get("vectorModels") or [],
        "vectorCache": {"reused": stats.get("reused", 0), "generated": stats.get("generated", 0), "uniqueInputs": stats.get("uniqueInputs", 0)},
        "searchProjection": {"timingsMs": stats.get("timingsMs", {})},
        "activeSceneId": world.active_scene().id if world.active_scene() else None,
        "activeScenes": [{"id": scene.id, "title": scene.title} for scene in world.active_scenes()],
        "currentTime": current.to_dict(), "changedPaths": changed,
        "sourceLoad": source_load,
        "chronology": {
            "calendarCount": stats.get("calendarCount", 0), "eraCount": stats.get("eraCount", 0),
            "anchorCount": stats.get("anchorCount", 0), "annotationCount": stats.get("annotationCount", 0),
            "insertBatches": stats.get("chronologyInsertBatches", 0),
            "maxInsertBatch": stats.get("chronologyMaxInsertBatch", 0),
            "timingMs": round(timings.get("chronology", 0), 3),
        },
        "timingsMs": {key: round(value, 3) for key, value in timings.items()},
    }


def compile_world(
    repository: Repository,
    revision: str = "HEAD",
    *,
    force: bool = False,
    profile_name: str | None = None,
    vector_provider: str | None = None,
    vector_model: str | None = None,
    vector_dimensions: int | None = None,
    vector_max_features: int | None = None,
    retain_revisions: int = 5,
) -> dict[str, Any]:
    started = time.perf_counter()
    timings: dict[str, float] = {}
    cache, database, revisions = cache_paths(repository)

    stage = time.perf_counter()
    resolved = repository.resolve(revision)
    tree_oid = repository.tree_oid(resolved)
    previous = database_meta(getattr(repository, "_transaction_profile_database", database))
    database_issues = _compiled_database_issues(database)
    timings["resolveRevision"] = (time.perf_counter() - stage) * 1000
    no_profile_override = all(
        value is None
        for value in (
            profile_name,
            vector_provider,
            vector_model,
            vector_dimensions,
            vector_max_features,
        )
    )
    if (
        not force
        and no_profile_override
        and previous.get("head_commit") == resolved
        and previous.get("tree_oid") == tree_oid
        and previous.get("source_schema") in COMPILED_SOURCE_SCHEMAS
        and previous.get("sqlite_schema") == SQLITE_SCHEMA
        and previous.get("compiler_version") == __version__
        and str(previous.get("compiler_fingerprint") or "").startswith(COMPILER_FINGERPRINT_PREFIX)
        and previous.get("profile_json")
        and not database_issues
    ):
        timings["total"] = (time.perf_counter() - started) * 1000
        return {
            "status": "cache-hit",
            "revision": resolved,
            "treeOid": tree_oid,
            "database": str(database),
            "recordCount": int(previous.get("record_count") or 0),
            "databaseBytes": database.stat().st_size,
            "searchProfile": json.loads(str(previous["profile_json"])),
            "vectorModels": json.loads(str(previous.get("vector_models_json") or "[]")),
            "sourceLoad": {
                "mode": "not-required",
                "parsed": 0,
                "cacheHits": 0,
                "blobReads": 0,
            },
            "timingsMs": {key: round(value, 3) for key, value in timings.items()},
        }

    # Load and parse once.  Invalid candidates never create a compiled cache.
    stage = time.perf_counter()
    world = repository.load_world(resolved, cache_write=False)
    timings["loadSource"] = (time.perf_counter() - stage) * 1000
    stage = time.perf_counter()
    diagnostics = validate_world(world)
    timings["validate"] = (time.perf_counter() - stage) * 1000
    errors = [item for item in diagnostics if item["severity"] == "error"]
    if errors:
        raise ValidationFailed(
            f"world validation failed with {len(errors)} error(s)", diagnostics
        )
    if no_profile_override and previous.get("profile_json"):
        # A compilation profile is an operational choice. Preserve it across
        # ordinary commits and external fast-forwards instead of unexpectedly
        # switching a lightweight FTS repository back to the world's default.
        previous_profile = json.loads(str(previous["profile_json"]))
        profile_name = str(previous_profile.get("name") or "hybrid")
        vector_provider = previous_profile.get("vectorProvider")
        vector_model = previous_profile.get("vectorModel")
        vector_dimensions = previous_profile.get("vectorDimensions")
        vector_max_features = previous_profile.get("vectorMaxFeatures")
    profile = resolve_profile(
        world,
        profile_name=profile_name,
        vector_provider=vector_provider,
        vector_model=vector_model,
        vector_dimensions=vector_dimensions,
        vector_max_features=vector_max_features,
    )
    compiler_fingerprint = fingerprint(world, profile)

    if (
        not force
        and previous.get("head_commit") == world.revision
        and previous.get("tree_oid") == world.tree_oid
        and previous.get("source_schema") == world.schema
        and previous.get("sqlite_schema") == SQLITE_SCHEMA
        and previous.get("compiler_version") == __version__
        and str(previous.get("compiler_fingerprint") or "").startswith(COMPILER_FINGERPRINT_PREFIX)
        and previous.get("compiler_fingerprint") == compiler_fingerprint
        and not database_issues
    ):
        timings["total"] = (time.perf_counter() - started) * 1000
        return {
            "status": "cache-hit",
            "revision": world.revision,
            "treeOid": world.tree_oid,
            "database": str(database),
            "recordCount": len(world.records),
            "databaseBytes": database.stat().st_size,
            "searchProfile": profile.as_dict(),
            "vectorModels": json.loads(str(previous.get("vector_models_json") or "[]")),
            "sourceLoad": dict(repository.last_load_stats),
            "timingsMs": {key: round(value, 3) for key, value in timings.items()},
        }

    build_mode = "full"
    changed: list[str] = []
    if previous.get("head_commit") and repository.is_ancestor(
        str(previous["head_commit"]), world.revision
    ):
        build_mode = "fast-forward-rebuild"
        changed = repository.changed_paths(str(previous["head_commit"]), world.revision)

    # Do not create a disposable cache merely by attempting to compile an
    # unsupported or invalid source component. This keeps latent v0.7
    # validation an entirely read-only boundary until its migration owns
    # generic runtime acceptance.
    cache.mkdir(parents=True, exist_ok=True)
    revisions.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix="world-", suffix=".sqlite", dir=cache
    )
    os.close(descriptor)
    temporary = Path(temporary_name)
    stats: dict[str, Any] = {}
    try:
        with closing(connect(temporary)) as connection:
            stage = time.perf_counter()
            _bootstrap_compiled_connection(connection)
            timings["schema"] = (time.perf_counter() - stage) * 1000
            connection.execute("BEGIN IMMEDIATE")
            current, stats = _populate_full_compiled_connection(
                connection, world, repository, profile, compiler_fingerprint, build_mode, timings,
            )
            connection.commit()
            _verify_compiled_connection(connection)
        os.replace(temporary, database)
        if world.revision != "WORKTREE":
            retained = _revision_cache_path(repository, revisions, world.revision)
            retained.unlink(missing_ok=True)
            try:
                os.link(database, retained)
            except OSError:
                shutil.copy2(database, retained)
            for old in sorted(
                _revision_cache_files(repository, revisions),
                key=lambda path: path.stat().st_mtime,
                reverse=True,
            )[retain_revisions:]:
                old.unlink(missing_ok=True)
        timings["total"] = (time.perf_counter() - started) * 1000
        return _compiled_report(
            world=world,
            database=database,
            database_bytes=database.stat().st_size,
            build_mode=build_mode,
            changed=changed,
            profile=profile,
            current=current,
            stats=stats,
            timings=timings,
            source_load=dict(repository.last_load_stats),
        )
    finally:
        temporary.unlink(missing_ok=True)

def world_from_database(repository: Repository, database: Path) -> World:
    with closing(connect(database, True)) as connection:
        meta_row = connection.execute("SELECT * FROM revision LIMIT 1").fetchone()
        if meta_row is None:
            raise RuntimeError("compiled database has no revision metadata")
        meta = dict(meta_row)
        records: dict[str, Record] = {}
        for row in connection.execute(
            "SELECT id,source_path,blob_oid,body_markdown,frontmatter_json FROM entity"
        ):
            frontmatter = json.loads(row[4])
            body = str(row[3])
            # Queries never need exact source bytes; retain a compact synthetic
            # envelope so code using Record.raw_bytes remains well-defined.
            raw = (canonical_json(frontmatter) + "\n" + body).encode("utf-8")
            record = Record(
                frontmatter,
                body,
                str(row[1]),
                raw,
                blob_oid=row[2],
                revision=str(meta["head_commit"]),
            )
            records[record.id] = record
    return World(
        str(meta["head_commit"]),
        str(meta["tree_oid"]),
        records,
        repository.root,
        repository.source_root,
        str(meta["head_commit"]) == "WORKTREE",
    )


def require_database(
    repository: Repository,
    revision: str = "HEAD",
    *,
    require_compiled: bool = False,
) -> tuple[World, Path]:
    readiness = cache_readiness(repository, revision)
    database = Path(readiness["database"])
    if readiness["state"] != "ready":
        if require_compiled:
            raise CompileRequired(
                "compiled cache is required; run `wedl compile` before this read",
                details={
                    "cache": readiness,
                    "hint": "Run `wedl compile` for this repository, or omit --require-compiled to allow an automatic rebuild.",
                },
            )
        compile_world(repository, revision)
        readiness = cache_readiness(repository, revision)
        if readiness["state"] != "ready":
            raise RuntimeError(f"compilation did not produce a ready database: {readiness['state']}")
    resolved = str(readiness["target"]["revision"])
    tree_oid = str(readiness["target"]["treeOid"])
    stat = database.stat()
    cache_key = (resolved, tree_oid, stat.st_mtime_ns, stat.st_size)
    world = repository._compiled_world_cache.get(cache_key)
    if world is None:
        world = world_from_database(repository, database)
        repository._compiled_world_cache.clear()
        repository._compiled_world_cache[cache_key] = world
    return world, database
