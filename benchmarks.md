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
`seed_instance(0)`, 20,000 calls per formatter, measured against the published
`cp314-cp314-macosx_11_0_arm64` wheel — the artifact users install, not a local build.

| formatter                | Faker calls/s | rusty calls/s | speedup |
| ------------------------ | ------------: | ------------: | ------: |
| first_name               |        39,441 |    10,787,731 |    274x |
| name                     |        18,280 |     5,478,826 |    300x |
| address                  |        14,259 |     2,750,653 |    193x |
| email                    |        18,968 |     2,749,645 |    145x |
| phone_number             |       208,753 |     9,543,692 |     46x |
| company                  |        12,958 |     5,624,626 |    434x |
| sentence                 |       202,292 |     4,501,759 |     22x |
| text                     |        28,032 |       558,638 |     20x |
| ipv4                     |        63,247 |     5,887,260 |     93x |
| date_time                |       523,732 |     5,588,609 |     11x |
| date_of_birth            |       321,230 |     5,273,855 |     16x |
| date_time_between        |       203,588 |     3,122,743 |     15x |
| date_time (tzinfo)       |       502,904 |     4,684,434 |    9.3x |
| date_time (zoneinfo)     |       504,627 |     4,499,649 |    8.9x |
| date_time_between (tz)   |       191,821 |     2,480,992 |     13x |
| date_time_this_year (tz) |       331,611 |       788,184 |    2.4x |
| date_of_birth (tz)       |       331,177 |     2,762,192 |    8.3x |
| iso8601 (tz)             |       359,609 |       976,704 |    2.7x |
| future_datetime (tz)     |       150,647 |     2,176,673 |     14x |
| random_element           |     1,222,762 |     6,542,897 |    5.4x |
| bothify                  |       301,934 |     9,932,129 |     33x |

These are single runs on one machine; treat single-digit percentage differences
as noise. The ordering is stable.

## Why the wheels are not abi3

An abi3 build would ship one wheel per platform covering every CPython from 3.10 on,
instead of one per version. It was measured and rejected: under `Py_LIMITED_API` PyO3
cannot use the datetime C-API macros and constructs values through the Python-level
`datetime` type instead. Both columns below are CI-built wheels benchmarked on the same
machine in the same way:

| formatter                | per-version |       abi3 | change |
| ------------------------ | ----------: | ---------: | -----: |
| date_time                |   5,588,609 |  3,314,642 |   -41% |
| date_time (tzinfo)       |   4,684,434 |  2,790,454 |   -40% |
| date_of_birth (tz)       |   2,762,192 |  1,553,393 |   -44% |
| date_time_this_year (tz) |     788,184 |    566,997 |   -28% |
| first_name               |  10,787,731 | 10,638,536 |     -1% |
| company                  |   5,624,626 |  5,185,153 |     -8% |

String formatters were unaffected; the datetime formatters lost about 40%, taking
`date_time` from 11x to 6.6x Faker and `date_time_this_year (tz)` from 2.4x to 1.8x.
The cost of choosing per-version wheels instead is that a new CPython release needs a new
rusty-faker release; adding `pyo3/abi3-py310` back to `[tool.maturin] features` would
reverse the trade.

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
