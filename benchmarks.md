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
`seed_instance(0)`, 20,000 calls per formatter, against the released abi3 wheel.

| formatter                | Faker calls/s | rusty calls/s | speedup |
| ------------------------ | ------------: | ------------: | ------: |
| first_name               |        39,044 |    10,638,536 |    273x |
| name                     |        17,774 |     5,588,868 |    314x |
| address                  |        13,910 |     2,854,900 |    205x |
| email                    |        18,402 |     2,712,891 |    147x |
| phone_number             |       203,638 |     9,501,188 |     47x |
| company                  |        12,645 |     5,185,153 |    410x |
| sentence                 |       191,611 |     4,328,614 |     23x |
| text                     |        27,022 |       553,888 |     21x |
| ipv4                     |        61,915 |     5,868,185 |     95x |
| date_time                |       499,164 |     3,314,642 |    6.6x |
| date_of_birth            |       314,151 |     3,912,300 |     13x |
| date_time_between        |       200,121 |     2,032,211 |     10x |
| date_time (tzinfo)       |       492,377 |     2,790,454 |    5.7x |
| date_time (zoneinfo)     |       483,051 |     2,751,725 |    5.7x |
| date_time_between (tz)   |       185,402 |     1,909,293 |     10x |
| date_time_this_year (tz) |       323,855 |       566,997 |    1.8x |
| date_of_birth (tz)       |       331,964 |     1,553,393 |    4.7x |
| iso8601 (tz)             |       354,945 |       821,646 |    2.3x |
| future_datetime (tz)     |       143,045 |     1,783,637 |     13x |
| random_element           |     1,178,924 |     5,149,331 |    4.4x |
| bothify                  |       292,672 |    10,049,832 |     34x |

These are single runs on one machine; treat single-digit percentage differences
as noise. The ordering is stable.

## What abi3 costs

The published wheels target CPython's limited API, which has a real and uneven price.
Under `Py_LIMITED_API`, PyO3 cannot use the datetime C-API macros and constructs values
by calling the Python-level `datetime` type instead. Measured against an otherwise
identical non-abi3 build:

| formatter                | non-abi3  | abi3      | change |
| ------------------------ | --------: | --------: | -----: |
| date_time                | 6,883,497 | 3,314,642 |   -52% |
| date_time (tzinfo)       | 4,859,972 | 2,790,454 |   -43% |
| date_of_birth (tz)       | 2,689,678 | 1,553,393 |   -42% |
| date_time_this_year (tz) |   782,353 |   566,997 |   -28% |
| first_name               |10,383,316 |10,638,536 |     0% |
| company                  | 5,392,349 | 5,185,153 |     0% |

String formatters are unaffected; datetime construction pays for it, and
`date_time_this_year (tz)` lands at 1.8x Faker. That is the trade for one wheel per
platform covering every CPython from 3.10 on, including versions released after this
one. Dropping `pyo3/abi3-py310` from `[tool.maturin] features` restores the non-abi3
numbers at the cost of a wheel per Python version per platform.

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
