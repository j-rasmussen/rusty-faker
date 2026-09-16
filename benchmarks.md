# Benchmarks

Call throughput of `rusty_faker` against the pinned Python Faker, measured with
`tools/bench.py`.

## How to reproduce

```
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop --uv --release
.venv/bin/python tools/bench.py --calls 20000
```

The `--release` build matters: a debug extension is several times slower and the
numbers below are not comparable to it.

## Results

Run on 2026-09-15 — Apple M2 Pro, macOS 27.0, rustc 1.97.1, CPython 3.14.0,
Faker 40.39.0 vs rusty_faker 0.1.0, locale `en_US`, both generators seeded with
`seed_instance(0)`, 20,000 calls per formatter.

| formatter                | Faker calls/s | rusty calls/s | speedup |
| ------------------------ | ------------: | ------------: | ------: |
| first_name               |        39,603 |    10,383,316 |    262x |
| name                     |        18,006 |     5,591,212 |    311x |
| address                  |        14,078 |     2,845,743 |    202x |
| email                    |        18,798 |     2,647,108 |    141x |
| phone_number             |       206,122 |    10,656,011 |     52x |
| company                  |        12,720 |     5,392,349 |    424x |
| sentence                 |       199,396 |     4,431,314 |     22x |
| text                     |        26,659 |       558,386 |     21x |
| ipv4                     |        62,719 |     6,306,910 |    101x |
| date_time                |       516,635 |     6,883,497 |     13x |
| date_of_birth            |       322,428 |     6,276,396 |     20x |
| date_time_between        |       206,623 |     3,078,423 |     15x |
| date_time (tzinfo)       |       496,998 |     4,859,972 |    9.8x |
| date_time (zoneinfo)     |       495,433 |     4,797,409 |    9.7x |
| date_time_between (tz)   |       187,532 |     2,399,664 |     13x |
| date_time_this_year (tz) |       327,435 |       782,353 |    2.4x |
| date_of_birth (tz)       |       338,187 |     2,689,678 |    8.0x |
| iso8601 (tz)             |       353,963 |       961,502 |    2.7x |
| future_datetime (tz)     |       153,671 |     2,281,846 |     15x |
| random_element           |     1,216,989 |     6,453,347 |    5.3x |
| bothify                  |       301,963 |    10,424,355 |     35x |

These are single runs on one machine; treat single-digit percentage differences
as noise. The ordering is stable.

## Why the wheels are not abi3

An abi3 build would ship one wheel per platform covering every CPython from 3.10 on,
instead of one per version. It was measured and rejected: under `Py_LIMITED_API` PyO3
cannot use the datetime C-API macros and constructs values through the Python-level
`datetime` type instead, which costs 40-50% on the datetime formatters.

| formatter                | per-version | abi3      | change |
| ------------------------ | ----------: | --------: | -----: |
| date_time                |   6,883,497 | 3,314,642 |   -52% |
| date_time (tzinfo)       |   4,859,972 | 2,790,454 |   -43% |
| date_of_birth (tz)       |   2,689,678 | 1,553,393 |   -42% |
| date_time_this_year (tz) |     782,353 |   566,997 |   -28% |
| first_name               |  10,383,316 |10,638,536 |     0% |
| company                  |   5,392,349 | 5,185,153 |     0% |

String formatters were unaffected, but `date_time` fell from 13x to 6.6x Faker and
`date_time_this_year (tz)` from 2.4x to 1.8x. The cost of the decision is that a new
CPython release needs a new rusty-faker release; adding `pyo3/abi3-py310` back to
`[tool.maturin] features` would reverse the trade.

## Reading the results

* **Template-heavy string formatters win biggest** (`company` 424x, `name` 311x,
  `first_name` 262x). The build script precomputes cumulative weights and
  pre-tokenizes `{{token}}` templates, and the hot paths append to a `&mut
  String`, so a call is a draw plus a few appends. Faker re-runs template
  substitution in Python on every call.
* **The floor is the timezone-aware date methods.** `date_time_this_year (tz)`
  (2.4x) and `iso8601 (tz)` (2.7x) are the outliers: they still call into
  CPython per call — the `*_this_*` methods ask Python for the period boundaries
  because those depend on the zone's offset at that wall-clock time. There the
  round-trip dominates and the Rust core barely shows.
* **`text` is the slowest rusty formatter in absolute terms** (558k calls/s)
  simply because each call builds far more output than the others.
* **`random_element` (5.3x) is the narrowest non-datetime case**, since Faker's
  version is a thin wrapper over `random` and rusty pays PyO3 argument
  conversion for the passed-in `OrderedDict`.
