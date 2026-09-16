# Contributing

## Setup

```bash
uv venv .venv
uv pip install --python .venv/bin/python -e ".[dev]"
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop --uv --release
```

Re-run `maturin develop` after any Rust change — the Python package imports the
compiled module, so an unrebuilt change simply isn't there.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings   # PYO3_PYTHON may need setting
cargo test
cargo audit
.venv/bin/python -m pytest tests
```

`pyproject.toml` sets `addopts = "-p no:faker"`, which disables the real Faker's pytest
plugin; without it, its `faker` fixture shadows ours and the compat tests fail
confusingly.

## Things worth knowing

- **The data is generated.** Never hand-edit `crates/rusty-faker-core/data/**/*.json`;
  re-run `tools/extract_faker_data.py` against the pinned Faker. See
  `crates/rusty-faker-core/data/README.md`.
- **The extension builds against CPython's limited API** (`pyo3/abi3-py310`, set in
  `[tool.maturin] features`). `PyDateAccess`, `PyTimeAccess` and `PyDeltaAccess` are
  unavailable there — read date and time components with `getattr`, as `ymd_of` does.
- **`pyo3/extension-module` lives in `[tool.maturin]`, not `Cargo.toml`**, so that
  `cargo test` and `cargo clippy` still link against libpython and run.
- **The version lives in `Cargo.toml`** (`[workspace.package] version`) and reaches
  Python through the compiled module. Bump it there and nowhere else.
- **Type stubs are generated**: `tools/generate_stubs.py` writes `proxy.pyi` from the
  pinned Faker's signatures; `--check` fails if the checked-in stub is stale.
- `.unwrap()` and `.expect()` are denied workspace-wide outside tests.

## Adding a formatter

Implement it in the core provider file, add a `Formatter` variant only if data templates
reference it, add the `#[pymethods]` wrapper and a `FORMATTER_NAMES` entry, rebuild, then
regenerate the stubs. The compat tests check existence, signature and return type against
Faker automatically.
