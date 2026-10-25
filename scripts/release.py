"""Validate release metadata and publish Rust crates in dependency order."""

import argparse
import ast
import json
import os
from pathlib import Path
import subprocess
import tomllib
import urllib.error
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
CRATES = (
    "onceover-report",
    "onceover-core",
    "onceover-deps",
    "onceover-mines",
    "onceover",
)


def read_toml(path):
    return tomllib.loads(path.read_text())


def check(tag=""):
    workspace = read_toml(ROOT / "Cargo.toml")["workspace"]
    version = workspace["package"]["version"]
    project = read_toml(ROOT / "pyproject.toml")["project"]
    if project["version"] != version:
        raise ValueError("Python and Rust package versions must match")
    module = ast.parse((ROOT / "python/onceover/__init__.py").read_text())
    python_version = next(
        node.value.value
        for node in module.body
        if isinstance(node, ast.Assign)
        and any(isinstance(target, ast.Name) and target.id == "__version__" for target in node.targets)
    )
    if python_version != version:
        raise ValueError("onceover.__version__ must match the package version")
    if tag and tag != f"v{version}":
        raise ValueError(f"release tag must be v{version}, got {tag!r}")
    for name in CRATES:
        manifest = read_toml(ROOT / "crates" / name / "Cargo.toml")
        if manifest["package"]["version"] != {"workspace": True}:
            raise ValueError(f"{name} must inherit the workspace version")
        if workspace["dependencies"][name]["version"] != f"={version}":
            raise ValueError(f"update the workspace dependency version for {name}")
    if read_toml(ROOT / "crates/onceover-py/Cargo.toml")["package"]["publish"] is not False:
        raise ValueError("the Python binding crate must not publish to crates.io")
    print(f"Release metadata matches {version}", flush=True)
    return version, workspace["package"]["repository"]


def published(name, version, repository):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}",
        headers={"User-Agent": f"onceover-release ({repository})"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise
    if (data["crate"].get("repository") or "").rstrip("/") != repository.rstrip("/"):
        raise ValueError(f"{name} is registered to a different repository")
    for entry in data["versions"]:
        if entry["num"] == version:
            if entry["yanked"]:
                raise ValueError(f"{name} {version} is yanked; choose a new version")
            return True
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "publish-crates"])
    parser.add_argument("--tag", default="")
    args = parser.parse_args()
    version, repository = check(args.tag)
    if args.command == "publish-crates":
        if not os.environ.get("CARGO_REGISTRY_TOKEN"):
            parser.error("CARGO_REGISTRY_TOKEN or crates.io trusted publishing is required")
        # Preflight every crate before publishing any new version. Already
        # published versions are skipped so a partial release can be resumed.
        existing = {name: published(name, version, repository) for name in CRATES}
        for name in CRATES:
            if existing[name]:
                print(f"Skipping {name} {version}: already published", flush=True)
                continue
            subprocess.run(
                ["cargo", "publish", "--locked", "--package", name], cwd=ROOT, check=True
            )


if __name__ == "__main__":
    main()
