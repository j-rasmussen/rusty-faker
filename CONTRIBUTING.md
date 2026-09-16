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

## Known issue: the extension will not load on macOS 26/27

On macOS 26 and 27 with Xcode's `ld-27037.1`, a locally built extension may build fine
and then fail to import:

```
ImportError: dlopen(.../rusty_faker/_core.cpython-314-darwin.so): (mis-aligned LINKEDIT string pool, fileOffset=0x...)
```

The linker places the symbol string table directly after the indirect symbol table
without padding, so when the indirect symbol count is odd the string pool lands
4-byte-aligned and dyld requires 8:

```
stroff = indirectsymoff + nindirectsyms * 4      # 1680024 + 313 * 4 = 1681276, mod 8 == 4
```

Check a build with `otool -l <file>.so | grep -A4 LC_SYMTAB`. It is an Apple linker bug,
not a project one: the same source built by GitHub's runners pads correctly, with
identical symbol counts, and those wheels import on an affected machine. Build flags do
not help — the indirect symbol count comes from the binary's import set, so LTO, `strip`,
`opt-level`, `-ld_classic` and the deployment target all leave it unchanged. The Command
Line Tools ship the same linker version, so `DEVELOPER_DIR` does not help either.

Until Apple fixes it, get a working local install from CI instead of building:

```bash
gh run download --name wheels-macos-aarch64 --dir /tmp/rf-wheels   # from a Release run
uv pip install --python .venv --no-index --find-links /tmp/rf-wheels --reinstall rusty-faker
```

That is enough to run the test suite and the benchmarks against a released build; it just
will not include local Rust changes, which have to go through CI to be tested.

**Watch for stale artifacts.** If more than one `_core.*.so` is present in
`python/rusty_faker/` — say one left over from a differently configured build — Python
may load the wrong one, so a "passing" local run can be testing a binary from before your
change. Delete them all before rebuilding.

## Things worth knowing

- **The data is generated.** Never hand-edit `crates/rusty-faker-core/data/**/*.json`;
  re-run `tools/extract_faker_data.py` against the pinned Faker. See
  `crates/rusty-faker-core/data/README.md`.
- **Wheels are built per CPython version, not abi3.** abi3 was measured and rejected: the
  limited API has no datetime C-API macros, so it cost 40-50% on the datetime formatters
  (see `benchmarks.md`). The consequence is that every new CPython release needs a new
  rusty-faker release, and the matrices in `.github/workflows/` are the list of supported
  versions.
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
