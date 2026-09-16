# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — unreleased

First release.

### Added

- Rust implementations of the `base`, `person`, `address`, `company`, `internet`,
  `lorem`, `phone_number` and `date_time` providers for `en_US` — 158 formatters,
  checked against Faker 40.39.0's signatures and return types by the compat suite.
- `Faker` proxy adapted from Faker's own: multi-locale and weighted construction,
  `fake["locale"]`, `unique`, `optional`, `seed`/`seed_instance`, `add_provider`,
  custom `BaseProvider` subclasses, `DynamicProvider`, copy and pickle.
- A pytest plugin exposing the `faker` fixture, mirroring Faker's.
- Wheels for CPython 3.10 to 3.14 on Linux x86_64/aarch64, macOS arm64/x86_64 and
  Windows x64, plus an sdist. Not abi3: it was measured and cost 40-50% on the datetime
  formatters, which the limited API forces through the Python-level `datetime`
  constructor. See benchmarks.md.
- `py.typed`, a generated `proxy.pyi` and a hand-written `_core.pyi`.

### Known limitations

- Seeded output is reproducible within rusty-faker but is **not** byte-identical to
  Faker's for the same seed.
- Only `en_US` data ships. A well-formed but unimplemented locale silently falls back
  to `en_US` rather than raising.
- `fake.parse()` raises `NotImplementedError` on a multi-locale proxy.
- Installing alongside the real Faker makes the two `faker` pytest fixtures collide;
  disable one with `-p no:faker`.
- `rusty-faker-core` is not published to crates.io.
