# Releasing onceover

Pushing a `vX.Y.Z` tag runs tests, builds and tests wheels, verifies the source
distribution, and publishes to PyPI and crates.io. Running the Release workflow
manually builds and checks artifacts without publishing. Release tooling uses
Python 3.11+; the package itself supports Python 3.10+.

## Registry setup

### PyPI

Create a [pending trusted publisher](https://pypi.org/manage/account/publishing/)
with these values:

| Field | Value |
|---|---|
| PyPI project | `onceover` |
| GitHub owner | `fcela` |
| Repository | `onceover` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

PyPI creates the project on its first successful publication. No PyPI API token
is needed. See the [PyPI instructions](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/).

### crates.io

The Rust crates are published in this order:

1. `onceover-report`
2. `onceover-core`
3. `onceover-deps`
4. `onceover-mines`
5. `onceover`

`onceover-py` is included only in Python distributions.

For the first release, create a crates.io API token authorized to publish the
five crates and add it as the repository Actions secret `CARGO_REGISTRY_TOKEN`:
https://github.com/fcela/onceover/settings/secrets/actions/new. The workflow
uses that token to create the crates. Do not put the token in a file or commit.

After the first release, configure a trusted publisher on each crate's
crates.io settings page: owner `fcela`, repository `onceover`, workflow
`release.yml`, environment `crates-io`. Remove the GitHub token secret and
revoke the bootstrap token; subsequent releases use OIDC automatically.
See [crates.io trusted publishing](https://crates.io/docs/trusted-publishing).

GitHub environments can be configured at
https://github.com/fcela/onceover/settings/environments. Their names must match
the registry configuration exactly.

## Publish a version

1. Set the version in `[workspace.package]` and all five local dependency
   constraints in `Cargo.toml`, `[project]` in `pyproject.toml`, and
   `__version__` in `python/onceover/__init__.py`.
2. Run `cargo check --workspace` to update `Cargo.lock`, then
   `python scripts/release.py check` and the test suites.
3. Commit, push, and tag the same version:

   ```bash
   git push origin main
   git tag v0.1.0
   git push origin v0.1.0
   ```

   Replace `0.1.0` for subsequent releases. Tags must match the package version.

Both Linux architectures are required: manylinux 2.28 x86_64 and aarch64.
macOS x86_64/aarch64 and Windows x86_64 wheels are also built and tested.
The stable-ABI wheels target CPython 3.10+ and are tested on Python 3.10,
3.13, and 3.15. Free-threaded Python wheels are not included.

The PyPI and crates.io jobs wait for successful checks and all build artifacts.
If a registry upload fails, fix its configuration and rerun failed jobs.
Already published files and crate versions are skipped. Published versions
are immutable; use a new version to change their contents.
