from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import uuid

import pytest

from wedl import transaction_recovery as recovery
from wedl.errors import RepositoryError

pytestmark = pytest.mark.normal_unit
TOTAL = 16 * 1024 * 1024


def _record(*, legacy: bool = False) -> dict[str, object]:
    version = recovery.FORMAT_VERSION if legacy else recovery.BUDGETED_FORMAT_VERSION
    record = {
        "version": version, "generation": 1, "id": str(uuid.uuid4()),
        "phase": "prepared", "ref": "refs/heads/main",
        "previousHead": "1" * 40, "committedHead": "2" * 40,
        "surfaces": [recovery.Surface("story/item.md", None, b"next", "source").as_json(version=version)],
        "indexBefore": {}, "indexAfter": {}, "indexLock": None,
        "indexArtifacts": None, "canonicalLock": None,
        "privateArtifacts": [], "sealedArtifacts": {}, "surfaceArtifacts": {},
    }
    if not legacy:
        budget = {"totalBytes": TOTAL, "callerReserveBytes": 128, "admittedPeakBytes": TOTAL}
        record["liveByteBudget"] = dict(budget)
        record["$liveByteBudget"] = dict(budget)
    return record


def _path(tmp_path: Path, record: dict[str, object]) -> Path:
    directory = tmp_path / ".wedl" / "transactions"
    directory.mkdir(parents=True)
    return directory / (str(record["id"]) + ".json")


def _document(record: dict[str, object]) -> bytes:
    return (json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n").encode()


def _wire(document: bytes, record: dict[str, object], **overrides) -> bytes:
    budget = record["liveByteBudget"]
    fields = {
        "total": budget["totalBytes"], "reserve": budget["callerReserveBytes"],
        "admitted": budget["admittedPeakBytes"], "surfaces": len(record["surfaces"]),
        "bytes": len(document), "sha256": hashlib.sha256(document).hexdigest(),
    }
    fields.update(overrides)
    header = " ".join(f"{name}={value}" for name, value in fields.items()) + "\n"
    return recovery.BUDGETED_ENVELOPE + header.encode("ascii") + document


def _forbid_document_decode(monkeypatch: pytest.MonkeyPatch):
    def forbidden(*_args, **_kwargs):
        pytest.fail("Unauthenticated journal reached full JSON/image allocation")
    monkeypatch.setattr(recovery.json, "loads", forbidden)
    monkeypatch.setattr(recovery, "_decode", forbidden)


def test_budgeted_load_reuses_one_authenticated_document_and_exact_images(tmp_path, monkeypatch):
    record = _record()
    path = _path(tmp_path, record)
    payload = recovery._canonical_journal_payload(record)
    path.write_bytes(payload)
    calls = []
    phases = []
    original = recovery._reject_noncanonical_budgeted_json

    def authenticate(document):
        calls.append(document)
        return original(document)

    monkeypatch.setattr(recovery, "_reject_noncanonical_budgeted_json", authenticate)
    monkeypatch.setattr(recovery, "_live_byte_observer", lambda phase, terms: phases.append((phase, terms)))
    journal = recovery.TransactionJournal.load(tmp_path, path)
    assert journal.record == record
    assert journal.surfaces == (recovery.Surface("story/item.md", None, b"next", "source"),)
    assert journal.surfaces is journal.surfaces
    assert journal._journal_bytes == payload
    assert journal._journal_identity == recovery._identity(os.stat(path))
    assert calls == [_document(record)]
    names = [phase for phase, _terms in phases]
    assert names.index("restart-header-precheck") < names.index("budgeted-json-stream-pre-admit")
    assert names.index("budgeted-json-stream-pre-admit") < names.index("budgeted-json-preparse")
    assert names.count("budgeted-json-preparse") == 1
    assert all(terms["projected_peak"] <= TOTAL for phase, terms in phases if "projected_peak" in terms)


def test_resigned_hostile_canonical_documents_refuse_before_full_decode(tmp_path, monkeypatch):
    record = _record()
    path = _path(tmp_path, record)
    document = _document(record)
    variants = [
        document.replace(b'"generation":1', b'"generation":1,"generation":1'),
        document.replace(b'"generation":1', b'"generation":1,"\\u0067eneration":1'),
        document.replace(b'"generation":', b'"\\u0067eneration":'),
        (json.dumps(dict(reversed(list(record.items()))), separators=(",", ":")) + "\n").encode(),
        document.replace(b'":', b'": ', 1),
        document.replace(b'"ref":', b'"r\xffef":'),
        document[:-2] + b"\n",
    ]
    assert all(variant != document for variant in variants)
    _forbid_document_decode(monkeypatch)
    for variant in variants:
        payload = _wire(variant, record)
        path.write_bytes(payload)
        with pytest.raises(RepositoryError):
            recovery.TransactionJournal.load(tmp_path, path)
        assert path.read_bytes() == payload


def test_envelope_length_digest_surface_and_early_budget_refuse_before_decode(tmp_path, monkeypatch):
    record = _record()
    path = _path(tmp_path, record)
    document = _document(record)
    valid = _wire(document, record)
    variants = [
        _wire(document, record, sha256="0" * 64),
        _wire(document, record, bytes=len(document) + 1),
        _wire(document, record, surfaces=2),
        _wire(document, record, total=1, reserve=0, admitted=1),
        _wire(document, record, total=10, reserve=11, admitted=1),
        _wire(document, record, total=recovery.sys.maxsize + 1, reserve=recovery.sys.maxsize + 1, admitted=1),
        recovery.BUDGETED_ENVELOPE + b"damaged\n" + document,
        valid[len(recovery.BUDGETED_ENVELOPE):],
    ]
    _forbid_document_decode(monkeypatch)
    for payload in variants:
        path.write_bytes(payload)
        with pytest.raises(RepositoryError):
            recovery.TransactionJournal.load(tmp_path, path)
        assert path.read_bytes() == payload


def test_parsed_header_binding_and_persisted_budget_remain_authoritative(tmp_path, monkeypatch):
    record = _record()
    path = _path(tmp_path, record)
    cases = []
    for change in ("version", "budget", "alias"):
        altered = json.loads(_document(record))
        if change == "version":
            altered["version"] = recovery.FORMAT_VERSION
        elif change == "budget":
            altered["liveByteBudget"]["callerReserveBytes"] += 1
            altered["$liveByteBudget"]["callerReserveBytes"] += 1
        else:
            altered["$liveByteBudget"]["callerReserveBytes"] += 1
        cases.append(_wire(_document(altered), record))
    original_decode = recovery._decode
    monkeypatch.setattr(recovery, "_decode", lambda _value: pytest.fail("Unbound header reached image decoding"))
    for payload in cases:
        path.write_bytes(payload)
        with pytest.raises(RepositoryError, match="live-byte budget"):
            recovery.TransactionJournal.load(tmp_path, path)
    monkeypatch.setattr(recovery, "_decode", original_decode)
    record["liveByteBudget"]["admittedPeakBytes"] = 0
    record["$liveByteBudget"]["admittedPeakBytes"] = 0
    payload = recovery._canonical_journal_payload(record)
    path.write_bytes(payload)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        recovery.TransactionJournal.load(tmp_path, path)
    assert path.read_bytes() == payload


def test_same_size_descriptor_mutation_re_admits_joined_graph_before_key_sets(tmp_path, monkeypatch):
    record = _record()
    record["padding"] = "x" * 4095
    changed = json.loads(_document(record))
    changed["padding"] = [0] * 2048
    # These equal-size canonical values have very different container graphs.
    assert len(_document(record)) == len(_document(changed))
    for _ in range(3):
        before_document = _document(record)
        payload = _wire(before_document, record)
        graph = recovery._budgeted_json_graph_upper_bound(before_document)
        total = max(
            5 * len(payload) + graph + 128,
            recovery._projected_live_sizes([], caller_reserve=128, journal_bytes=len(payload), surface_count=1),
        ) + 1024
        for value in (record, changed):
            value["liveByteBudget"]["totalBytes"] = total
            value["liveByteBudget"]["admittedPeakBytes"] = total
            value["$liveByteBudget"] = dict(value["liveByteBudget"])
    before_payload = _wire(_document(record), record)
    after_document = _document(changed)
    after_payload = _wire(after_document, changed)
    assert len(before_payload) == len(after_payload)
    assert 5 * len(after_payload) + recovery._budgeted_json_graph_upper_bound(after_document) + 128 > total
    path = _path(tmp_path, record)
    path.write_bytes(before_payload)
    identity = recovery._identity(os.stat(path))
    original_scan = recovery._budgeted_json_graph_upper_bound_from_descriptor
    events = []

    def mutate_after_scan(descriptor, **kwargs):
        result = original_scan(descriptor, **kwargs)
        path.write_bytes(after_payload)
        assert recovery._identity(os.stat(path)) == identity
        return result

    monkeypatch.setattr(recovery, "_budgeted_json_graph_upper_bound_from_descriptor", mutate_after_scan)
    monkeypatch.setattr(recovery, "_live_byte_observer", lambda phase, terms: events.append((phase, terms)))
    monkeypatch.setattr(recovery, "_reject_noncanonical_budgeted_json",
                        lambda _document: pytest.fail("Joined graph exceeded budget before key-set admission"))
    _forbid_document_decode(monkeypatch)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        recovery._load_journal_payload(path, include_document=True)
    stream = next(terms for phase, terms in events if phase == "budgeted-json-stream-pre-admit")
    joined = next(terms for phase, terms in events if phase == "budgeted-json-preparse")
    assert stream["projected_peak"] <= total < joined["projected_peak"]
    assert joined["parsed_graph"] > stream["parsed_graph"]
    assert path.read_bytes() == after_payload


def test_default_legacy_and_authoritative_cleanup_limits_preserve_loader_contract(tmp_path):
    record = _record()
    path = _path(tmp_path, record)
    payload = recovery._canonical_journal_payload(record)
    path.write_bytes(payload)
    identity = recovery._identity(os.stat(path))
    assert recovery._load_journal_payload(path) == (payload, identity)
    assert recovery._load_journal_payload(path, include_document=True) == (payload, identity, _document(record))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        recovery._load_journal_payload(path, budgeted_temp_limit=len(payload) - 1)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        recovery._load_journal_payload(path, authoritative_budget=(1, 0))
    legacy = _record(legacy=True)
    legacy["id"] = record["id"]
    legacy_payload = recovery._canonical_journal_payload(legacy)
    path.write_bytes(legacy_payload)
    legacy_identity = recovery._identity(os.stat(path))
    assert recovery._load_journal_payload(path) == (legacy_payload, legacy_identity)
    assert recovery._load_journal_payload(path, include_document=True) == (legacy_payload, legacy_identity, legacy_payload)
    loaded = recovery.TransactionJournal.load(tmp_path, path)
    assert loaded.record == legacy and loaded._journal_bytes == legacy_payload
    assert loaded.surfaces == (recovery.Surface("story/item.md", None, b"next", "source"),)
    with pytest.raises(RepositoryError, match="live-byte budget"):
        recovery._load_journal_payload(path, budgeted_temp_limit=len(legacy_payload) + 1)


def test_loaded_identity_and_same_byte_journal_cas_substitution_still_refuse(tmp_path, monkeypatch):
    record = _record()
    path = _path(tmp_path, record)
    payload = recovery._canonical_journal_payload(record)
    path.write_bytes(payload)
    journal = recovery.TransactionJournal.load(tmp_path, path)
    generation = journal.record["generation"]
    replacement = path.with_suffix(".replacement")
    replacement.write_bytes(payload)
    replacement_identity = recovery._identity(os.stat(replacement))
    assert replacement_identity != journal._journal_identity
    os.replace(replacement, path)
    with pytest.raises(RepositoryError):
        journal._persist()
    assert journal.record["generation"] == generation
    assert path.read_bytes() == payload
    assert recovery._identity(os.stat(path)) == replacement_identity
    monkeypatch.setattr(recovery, "_path_has_identity", lambda _path, _identity: False)
    _forbid_document_decode(monkeypatch)
    with pytest.raises(RepositoryError, match="invalid WEDL transaction journal"):
        recovery.TransactionJournal.load(tmp_path, path)
    assert path.read_bytes() == payload


def _committed_lexical_bound(document: bytes) -> int:
    """The pre-span committed scanner, retained as a finite equivalence oracle."""
    quotes = openings = separators = 0
    in_string = escaped = False
    for byte in document:
        if in_string:
            if escaped:
                escaped = False
            elif byte == 0x5C:
                escaped = True
            elif byte == 0x22:
                in_string = False
                quotes += 1
        elif byte == 0x22:
            in_string = True
            quotes += 1
        elif byte in (0x5B, 0x7B):
            openings += 1
        elif byte in (0x2C, 0x3A):
            separators += 1
    return (4 * len(document)
            + max(recovery.sys.getsizeof({}), recovery.sys.getsizeof([])) * openings
            + recovery.sys.getsizeof("") * ((quotes + 1) // 2)
            + 32 * separators)


def test_graph_bound_exact_lexical_corpus_and_escape_parity():
    from itertools import product

    container = max(recovery.sys.getsizeof({}), recovery.sys.getsizeof([]))
    string = recovery.sys.getsizeof("")
    # Expected counts are stated independently of either scanner.
    cases = [
        (b"", 0, 0, 0), (b"[{,:]", 0, 2, 2),
        (b'"[{,:]"', 2, 0, 0), (b'"unterminated[{,:', 1, 0, 0),
        (b'\\[{,:', 0, 2, 2), (b'\xff\x00', 0, 0, 0),
    ]
    for document, quotes, openings, separators in cases:
        expected = 4 * len(document) + container * openings + string * ((quotes + 1) // 2) + 32 * separators
        assert recovery._budgeted_json_graph_upper_bound(document) == expected
    for run in (*range(66), 1023, 1024):
        document = b'"' + b"\\" * run + b'"[,:'
        quotes, openings, separators = (1, 0, 0) if run % 2 else (2, 1, 2)
        expected = 4 * len(document) + string * ((quotes + 1) // 2) + container * openings + 32 * separators
        assert recovery._budgeted_json_graph_upper_bound(document) == expected
        for tail in (b"", b"\\", b'"', b'\xff[{,:'):
            malformed = document + tail
            assert recovery._budgeted_json_graph_upper_bound(malformed) == _committed_lexical_bound(malformed)
    # All 37,449 byte strings of length0..5 over every lexical class, including
    # invalid UTF-8. The oracle also covers malformed tails without parsing JSON.
    alphabet = (0x22, 0x5C, 0x5B, 0x7B, 0x2C, 0x3A, 0x78, 0xFF)
    compared = 0
    for length in range(6):
        for values in product(alphabet, repeat=length):
            document = bytes(values)
            assert recovery._budgeted_json_graph_upper_bound(document) == _committed_lexical_bound(document)
            compared += 1
    assert compared == 37449


def test_graph_bound_descriptor_chunk_carry_and_malformed_tail(tmp_path):
    chunk = recovery._BOUNDED_CAPTURE_CHUNK
    path = tmp_path / "lexical-document"
    offset = 13
    for boundary in (chunk - 1, chunk, chunk + 1, 2 * chunk - 1):
        for run in (0, 1, 2, 3):
            # Place an escape run/quote around real descriptor chunk boundaries.
            prefix = b'["' + b"x" * (boundary - 2)
            ending = b"\\" * run + b'"'
            if run % 2:
                ending += b'[{,:"'
            document = prefix + ending + b',{"k":[0]}]\n'
            expected = _committed_lexical_bound(document)
            assert recovery._budgeted_json_graph_upper_bound(document) == expected
            path.write_bytes(b"x" * offset + document)
            descriptor = os.open(path, os.O_RDONLY)
            try:
                assert recovery._budgeted_json_graph_upper_bound_from_descriptor(
                    descriptor, offset=offset, document_bytes=len(document)) == expected
            finally:
                os.close(descriptor)
    for tail in (b'"unterminated', b'"trailing\\'):
        document = b"x" * (chunk - 1) + tail
        assert recovery._budgeted_json_graph_upper_bound(document) == _committed_lexical_bound(document)
        path.write_bytes(document)
        descriptor = os.open(path, os.O_RDONLY)
        try:
            with pytest.raises(RepositoryError, match="live-byte budget"):
                recovery._budgeted_json_graph_upper_bound_from_descriptor(
                    descriptor, offset=0, document_bytes=len(document))
        finally:
            os.close(descriptor)


def test_graph_bound_exact_budget_admission_and_checked_overflow(monkeypatch):
    record = _record()
    record["padding"] = "[{,:" * 1024 + '\\"' * 17
    document = _document(record)
    payload = _wire(document, record)
    expected_graph = _committed_lexical_bound(document)
    assert recovery._budgeted_json_graph_upper_bound(document) == expected_graph
    # Whole-document raw punctuation counts would reject this admitted string.
    raw_graph = (4 * len(document)
                 + max(recovery.sys.getsizeof({}), recovery.sys.getsizeof([]))
                 * (document.count(b"[") + document.count(b"{"))
                 + recovery.sys.getsizeof("") * ((document.count(b'"') + 1) // 2)
                 + 32 * (document.count(b",") + document.count(b":")))
    assert raw_graph > expected_graph
    reserve = 128
    peak = reserve + 5 * len(payload) + expected_graph
    phases = []
    original_canonical = recovery._reject_noncanonical_budgeted_json
    canonical_calls = []
    def canonical(admitted):
        canonical_calls.append(admitted)
        return original_canonical(admitted)
    monkeypatch.setattr(recovery, "_reject_noncanonical_budgeted_json", canonical)
    monkeypatch.setattr(recovery, "_live_byte_observer", lambda phase, terms: phases.append((phase, terms)))
    _forbid_document_decode(monkeypatch)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        recovery._budgeted_document(payload, authoritative_budget=(peak - 1, reserve))
    assert canonical_calls == []
    for total in (peak, peak + 1):
        assert recovery._budgeted_document(payload, authoritative_budget=(total, reserve)) == document
    assert canonical_calls == [document, document]
    observed = [terms for phase, terms in phases if phase == "budgeted-json-preparse"]
    assert len(observed) == 3
    assert all(terms["parsed_graph"] == expected_graph and terms["projected_peak"] == peak for terms in observed)
    # Force an otherwise tiny graph term over the same installed integer ceiling.
    monkeypatch.setattr(recovery.sys, "getsizeof", lambda _value: recovery.sys.maxsize)
    with pytest.raises(RepositoryError, match="live-byte budget"):
        recovery._budgeted_json_graph_upper_bound(b"[")
