# rusty-faker

A Rust-backed drop-in replacement for Python's [Faker](https://github.com/joke2k/faker).
Same API, 10-400x the throughput.

```python
from rusty_faker import Faker

fake = Faker()
fake.name()       # 'Danielle Hoffman'
fake.address()    # '778 Brown Cape\nNorth Michaelfurt, MT 20164'
fake.email()      # 'jonathanadams@example.org'
```

The generator, its data tables and every formatter live in Rust; the Python layer is a
thin proxy that caches bound methods, so `fake.name()` is a dict lookup plus one call
across the FFI boundary.

## Read this before swapping the import

**Seeded output is not byte-identical to Faker's.** `seed_instance(1234)` is reproducible
across runs and across patch releases of rusty-faker, but it will *not* reproduce the
values Faker gives for that seed. If you have snapshot tests or golden files keyed to
Faker's seeded output, they will fail.

**v1 ships `en_US` data only, and a well-formed unknown locale silently falls back to
it.** `Faker("de_DE").city()` returns an American city rather than raising. Only a
malformed locale string raises `AttributeError`.

**The pytest plugin collides with Faker's.** Both packages register a `pytest11` plugin
exposing a `faker` fixture. With both installed, one shadows the other. Pick a side:

```toml
[tool.pytest.ini_options]
addopts = "-p no:faker"   # disables the real Faker's plugin, keeping rusty-faker's
```

**This is 0.1.0.** The API tracks Faker 40.39.0's surface, but the seed-to-value mapping
may change in any minor release before 1.0.

## Install

```bash
pip install rusty-faker
uv add rusty-faker
```

Wheels are published for Linux x86_64/aarch64 (manylinux), macOS arm64/x86_64 and
Windows x86_64. They are abi3 wheels — one per platform, covering CPython 3.10 and every
later version. Other platforms, and free-threaded builds (3.13t/3.14t, which abi3 does
not cover), compile from the sdist and need a Rust toolchain (1.85+).

## What v1 covers

The `base`, `person`, `address`, `company`, `internet`, `lorem`, `phone_number` and
`date_time` providers for `en_US` — 158 formatters, checked against Faker's signatures
and return types by the compat test suite.

These proxy features work as they do in Faker: multi-locale `Faker([...])` and weighted
`Faker(OrderedDict(...))`, `fake["de_DE"]`, `fake.unique`, `fake.optional`,
`seed_instance`/`seed`, `add_provider`, `BaseProvider` subclasses, `DynamicProvider`, and
copy/pickle. Custom Python providers resolve ahead of the Rust core, so overriding a
built-in formatter works.

Not implemented: every other Faker provider, every other locale, and `fake.parse()` on a
multi-locale proxy (it raises `NotImplementedError`).

## Performance

Calls per second, Faker 40.39.0 vs rusty-faker 0.1.0, 20,000 calls each on an M2 Pro:

| formatter                | Faker   | rusty-faker | speedup |
| ------------------------ | ------: | ----------: | ------: |
| company                  |  12,645 |   5,185,153 |    410x |
| name                     |  17,774 |   5,588,868 |    314x |
| email                    |  18,402 |   2,712,891 |    147x |
| sentence                 | 191,611 |   4,328,614 |     23x |
| date_time                | 499,164 |   3,314,642 |    6.6x |
| date_time_this_year (tz) | 323,855 |     566,997 |    1.8x |

Template-heavy string formatters gain the most. The floor is the date and time methods:
they call into CPython to build each value, and the abi3 wheels pay extra for that
because the limited API has no datetime C-API macros. One machine, one run — see
[benchmarks.md](benchmarks.md) for the full table, the abi3 cost broken out, and how to
reproduce it.

## Building from source

On macOS 26/27 a locally built extension may fail to import with `mis-aligned LINKEDIT
string pool` — an Apple linker bug that does not affect the published wheels. See
[CONTRIBUTING.md](CONTRIBUTING.md#known-issue-the-extension-will-not-load-on-macos-2627).

```bash
git clone https://github.com/j-rasmussen/rusty-faker && cd rusty-faker
uv venv .venv
uv pip install --python .venv/bin/python -e ".[dev]"
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop --uv --release
.venv/bin/python -m pytest tests
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full loop.

## How it works

Faker's data tables are extracted once from a pinned release into JSON. `build.rs` turns
that JSON into static Rust tables at compile time, with cumulative weights precomputed and
`{{token}}` templates pre-tokenized — nothing is parsed at runtime. Formatters append into
a reused `String` instead of allocating per fragment. PyO3 exposes one `Generator` class
whose method and parameter names mirror Faker's. See [CLAUDE.md](CLAUDE.md) for the
architecture in detail.

## License and attribution

MIT — see [LICENSE](LICENSE).

The compiled-in data and several Python modules are derived from Faker, MIT
© 2012 Daniele Faraglia and individual contributors; its license is kept at
[LICENSE-faker](LICENSE-faker) and the derivation is itemized in [NOTICE](NOTICE).
rusty-faker is not affiliated with or endorsed by the Faker project.
