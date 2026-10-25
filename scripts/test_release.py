"""Release validation must fail before any registry publication."""

import importlib.util
from pathlib import Path
import sys

import pytest


spec = importlib.util.spec_from_file_location("release", Path(__file__).with_name("release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def test_wrong_tag_is_rejected():
    with pytest.raises(ValueError, match="release tag must"):
        release.check("v999.0.0")


def test_mismatched_python_version_is_rejected(monkeypatch):
    original = release.read_toml

    def read(path):
        data = original(path)
        if path.name == "pyproject.toml":
            data["project"]["version"] = "999.0.0"
        return data

    monkeypatch.setattr(release, "read_toml", read)
    with pytest.raises(ValueError, match="versions must match"):
        release.check()


def test_missing_token_fails_before_registry_access(monkeypatch):
    monkeypatch.delenv("CARGO_REGISTRY_TOKEN", raising=False)
    monkeypatch.setattr(sys, "argv", ["release.py", "publish-crates"])
    monkeypatch.setattr(release, "published", lambda *args: pytest.fail("registry accessed"))
    with pytest.raises(SystemExit):
        release.main()


def test_partial_publication_resumes_in_dependency_order(monkeypatch):
    calls = []
    monkeypatch.setenv("CARGO_REGISTRY_TOKEN", "test-only-placeholder")
    monkeypatch.setattr(sys, "argv", ["release.py", "publish-crates"])
    monkeypatch.setattr(release, "published", lambda name, *args: name in release.CRATES[:2])
    monkeypatch.setattr(release.subprocess, "run", lambda command, **kwargs: calls.append(command))
    release.main()
    assert [command[-1] for command in calls] == list(release.CRATES[2:])
    assert all(command[:3] == ["cargo", "publish", "--locked"] for command in calls)


def test_conflicting_registry_package_prevents_all_uploads(monkeypatch):
    monkeypatch.setenv("CARGO_REGISTRY_TOKEN", "test-only-placeholder")
    monkeypatch.setattr(sys, "argv", ["release.py", "publish-crates"])
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: pytest.fail("uploaded"))

    def published(name, *args):
        if name == "onceover-mines":
            raise ValueError("registered to a different repository")
        return False

    monkeypatch.setattr(release, "published", published)
    with pytest.raises(ValueError, match="different repository"):
        release.main()
