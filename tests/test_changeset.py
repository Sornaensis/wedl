from __future__ import annotations

import subprocess

from wedl.changeset import apply, preview


def test_conversation_changeset_is_atomic_and_idempotent(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    nessa = world.find("Nessa Quill", "character")
    scene = world.find("Flood Gallery N", "scene")
    payload = {
        "protocol": "wedl-changeset/v1",
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "conversation-test-v1",
        "summary": "record a new exchange",
        "operations": [
            {
                "type": "conversation.create",
                "temporaryId": "$tmp.conversation",
                "value": {
                    "title": "A Test Exchange",
                    "status": "closed",
                    "scene": scene.id,
                    "location": scene.frontmatter["location"],
                    "time": {"start": {"timeline": "main", "tick": 140, "order": 0}, "end": {"timeline": "main", "tick": 140, "order": 20}},
                    "participants": [
                        {"character": mara.id, "from": {"timeline": "main", "tick": 140, "order": 0}, "to": {"timeline": "main", "tick": 140, "order": 20}},
                        {"character": nessa.id, "from": {"timeline": "main", "tick": 140, "order": 0}, "to": {"timeline": "main", "tick": 140, "order": 20}},
                    ],
                    "turns": [
                        {"id": "turn_00000000000000000000000000", "at": {"timeline": "main", "tick": 140, "order": 10}, "speaker": mara.id, "text": "Keep the ribbon dry.", "audience": ["participants"]},
                        {"id": "turn_00000000000000000000000001", "at": {"timeline": "main", "tick": 140, "order": 20}, "speaker": nessa.id, "text": "I heard you.", "audience": ["participants"]},
                    ],
                    "recollections": [
                        {"id": "recol_00000000000000000000000000", "character": mara.id, "at": {"timeline": "main", "tick": 140, "order": 30}, "state": "remembered", "summary": "Nessa was defensive but attentive.", "interpretation": "She is frightened.", "confidence": 0.7, "exact_turns": ["turn_00000000000000000000000001"], "remembered_quotes": []}
                    ],
                },
            }
        ],
    }
    plan = preview(ash_repo, payload)
    assert plan["valid"] is True
    assert plan["generatedIds"]["$tmp.conversation"].startswith("conv_")
    receipt = apply(ash_repo, payload, allow_unconfirmed=True)
    assert receipt["newHead"] == ash_repo.head()
    replay = apply(ash_repo, payload, allow_unconfirmed=True)
    assert replay["idempotentReplay"] is True


def test_unrelated_staged_file_is_preserved(ash_repo) -> None:
    note = ash_repo.root / "notes.txt"
    note.write_text("staged but unrelated\n")
    subprocess.run(["git", "-C", str(ash_repo.root), "add", "notes.txt"], check=True)
    world = ash_repo.load_world()
    object_record = world.find("Black Salt Vial", "object")
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "update-object-v1",
        "summary": "retag one object",
        "operations": [{"type": "entity.update", "entity": object_record.id, "frontmatterPatch": {"tags": [*object_record.tags, "test-tag"]}}],
    }
    apply(ash_repo, payload, allow_unconfirmed=True)
    staged = subprocess.run(["git", "-C", str(ash_repo.root), "diff", "--cached", "--name-only"], check=True, text=True, stdout=subprocess.PIPE).stdout.splitlines()
    assert "notes.txt" in staged


def test_transaction_updates_real_index_for_new_files(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    nessa = world.find("Nessa Quill", "character")
    scene = world.find("The Choice of Records", "scene")
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "new-file-index-sync-v1",
        "summary": "record a cleanly materialized conversation",
        "operations": [
            {
                "type": "conversation.create",
                "temporaryId": "$tmp.clean-conversation",
                "value": {
                    "title": "Index Synchronization Exchange",
                    "status": "closed",
                    "scene": scene.id,
                    "location": scene.frontmatter["location"],
                    "time": {
                        "start": {"timeline": "main", "tick": 179, "order": 0},
                        "end": {"timeline": "main", "tick": 179, "order": 20},
                    },
                    "participants": [
                        {"character": mara.id, "from": {"timeline": "main", "tick": 179, "order": 0}, "to": {"timeline": "main", "tick": 179, "order": 20}},
                        {"character": nessa.id, "from": {"timeline": "main", "tick": 179, "order": 0}, "to": {"timeline": "main", "tick": 179, "order": 20}},
                    ],
                    "turns": [
                        {"id": "turn_00000000000000000000000010", "at": {"timeline": "main", "tick": 179, "order": 10}, "speaker": mara.id, "text": "The index should remain clean.", "audience": ["participants"]},
                        {"id": "turn_00000000000000000000000011", "at": {"timeline": "main", "tick": 179, "order": 20}, "speaker": nessa.id, "text": "Then check it.", "audience": ["participants"]},
                    ],
                    "recollections": [],
                },
            }
        ],
    }
    apply(ash_repo, payload, allow_unconfirmed=True)
    managed_status = subprocess.run(
        ["git", "-C", str(ash_repo.root), "status", "--porcelain=v1", "--", "story"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    assert managed_status == ""


def test_append_operations_allocate_typed_temporary_ids(ash_repo) -> None:
    world = ash_repo.load_world()
    scene = world.find("The Choice of Records", "scene")
    mara = world.find("Mara Vale", "character")
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "typed-append-temp-ids-v2",
        "summary": "create a conversation and append one typed turn and memory",
        "operations": [
            {
                "type": "conversation.create",
                "temporaryId": "$tmp.conversation.append-test",
                "value": {
                    "title": "Typed Append Test",
                    "status": "active",
                    "scene": scene.id,
                    "location": scene.frontmatter["location"],
                    "time": {"start": {"timeline": "main", "tick": 180, "order": 0}},
                    "participants": [
                        {"character": mara.id, "from": {"timeline": "main", "tick": 180, "order": 0}}
                    ],
                    "turns": [],
                    "recollections": [],
                },
            },
            {
                "type": "conversation.turn.append",
                "conversationId": "$tmp.conversation.append-test",
                "temporaryId": "$tmp.turn.appended",
                "turn": {
                    "at": {"timeline": "main", "tick": 180, "order": 10},
                    "speaker": mara.id,
                    "text": "This line should receive a typed turn ID.",
                    "audience": ["participants"],
                },
            },
            {
                "type": "conversation.recollection.record",
                "conversationId": "$tmp.conversation.append-test",
                "temporaryId": "$tmp.recollection.appended",
                "recollection": {
                    "character": mara.id,
                    "at": {"timeline": "main", "tick": 180, "order": 20},
                    "state": "remembered",
                    "summary": "Mara remembers the appended line.",
                    "interpretation": "It was a type-safety test.",
                    "confidence": 1.0,
                    "exact_turns": ["$tmp.turn.appended"],
                    "remembered_quotes": [],
                },
            },
        ],
    }
    plan = preview(ash_repo, payload)
    assert plan["valid"] is True
    assert plan["generatedIds"]["$tmp.conversation.append-test"].startswith("conv_")
    assert plan["generatedIds"]["$tmp.turn.appended"].startswith("turn_")
    assert plan["generatedIds"]["$tmp.recollection.appended"].startswith("recol_")


def test_changeset_recompile_preserves_selected_profile(ash_repo) -> None:
    from wedl.compiler import compile_world, database_meta

    compile_world(ash_repo, force=True, profile_name="fts")
    world = ash_repo.load_world()
    object_record = world.find("Black Salt Vial", "object")
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "preserve-fts-profile-v1",
        "summary": "update an object without changing compile profile",
        "operations": [
            {
                "type": "entity.update",
                "entity": object_record.id,
                "frontmatterPatch": {"tags": [*object_record.tags, "profile-test"]},
            }
        ],
    }
    receipt = apply(ash_repo, payload, allow_unconfirmed=True)
    assert receipt["compile"]["searchProfile"]["name"] == "fts"
    meta = database_meta(ash_repo.root / ".wedl" / "world.sqlite")
    assert meta["search_profile"] == "fts"
