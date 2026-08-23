from __future__ import annotations

import pytest

from wedl.errors import NotFound
from wedl.query import list_entities, show_entity


def test_entity_references_resolve_by_id_title_alias_and_slug(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")

    assert world.find(mara.id, "character") is mara
    assert world.find("mArA vAlE", "character") is mara
    assert world.find("MARA", "character") is mara
    assert world.find("mara-vale", "character") is mara
    assert world.find("chief-sorn", "character").title == "Ilyra Sorn"
    assert show_entity(ash_repo, "mara-vale")["id"] == mara.id


def test_exact_id_precedes_a_matching_alias(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    ilyra = world.find("Ilyra Sorn", "character")
    ilyra.frontmatter["aliases"].append(mara.id)

    assert world.find(mara.id, "character") is mara


def test_title_or_alias_precedes_a_matching_slug(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    ilyra = world.find("Ilyra Sorn", "character")
    ilyra.frontmatter["aliases"].append("Mara!")

    assert world.find("Mara", "character") is mara


def test_ambiguous_entity_reference_includes_candidate_details(ash_repo) -> None:
    world = ash_repo.load_world()
    ilyra = world.find("Ilyra Sorn", "character")
    ilyra.frontmatter["aliases"].append("Mara")

    with pytest.raises(NotFound) as raised:
        world.find("Mara", "character")

    error = raised.value
    assert error.message == "ambiguous entity name 'Mara'"
    assert {candidate["title"] for candidate in error.details["candidates"]} == {"Mara Vale", "Ilyra Sorn"}
    assert error.details["suggestions"] == error.details["candidates"]


def test_unknown_entity_reference_returns_suggestions_without_resolving_fuzzily(ash_repo) -> None:
    world = ash_repo.load_world()

    with pytest.raises(NotFound) as raised:
        world.find("Mara Vail", "character")

    error = raised.value
    assert "could not resolve 'Mara Vail'; did you mean 'Mara Vale'?" == error.message
    assert error.details["suggestions"] == [
        {
            "id": "char_00HBQM4T4CMF11RKMNDBRP92QC",
            "kind": "character",
            "title": "Mara Vale",
            "reference": "Mara Vale",
        }
    ]


def test_near_typo_of_an_ambiguous_reference_does_not_select_a_record(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    ilyra = world.find("Ilyra Sorn", "character")
    mara.frontmatter["aliases"].append("Shared Call")
    ilyra.frontmatter["aliases"].append("Shared Call")

    with pytest.raises(NotFound) as raised:
        world.find("Shared Cal", "character")

    assert raised.value.message == "could not resolve 'Shared Cal'"
    assert raised.value.details == {}


def test_entity_list_text_matches_aliases_and_canonical_slugs(ash_repo) -> None:
    assert [item["title"] for item in list_entities(ash_repo, kind="character", text="chief-sorn")] == ["Ilyra Sorn"]
    assert [item["title"] for item in list_entities(ash_repo, kind="character", text="mara-vale")] == ["Mara Vale"]
