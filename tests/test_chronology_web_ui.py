from __future__ import annotations

from importlib import resources
from pathlib import Path
import shutil

from fastapi.testclient import TestClient
import pytest

from wedl.repository import Repository
from wedl.server import create_app


@pytest.fixture()
def chronology_ui_repository(tmp_path: Path) -> Repository:
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, tmp_path / "world" / "story")
    return Repository(tmp_path / "world")


def test_chronology_ui_assets_are_served_and_the_legacy_catalog_has_an_honest_panel(chronology_ui_repository: Repository) -> None:
    with TestClient(create_app(chronology_ui_repository.root)) as client:
        index = client.get("/")
        module = client.get("/assets/chronology.mjs")
        app = client.get("/assets/app.js")
        catalog = client.get("/api/chronology")

    assert index.status_code == module.status_code == app.status_code == catalog.status_code == 200
    assert 'data-view="chronology"' in index.text
    assert "chronology.mjs" in app.text
    assert "CHRONOLOGY_PROTOCOL" in module.text
    assert catalog.json()["capability"]["mode"] == "ordinal-only"
    assert catalog.json()["calendars"] == catalog.json()["eras"] == catalog.json()["anchors"] == []
