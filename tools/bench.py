#!/usr/bin/env python3
"""Compare call throughput of rusty_faker against the pinned Python Faker.

Usage: .venv/bin/python tools/bench.py [--calls 100000]
"""

from __future__ import annotations

import argparse
import time
from collections import OrderedDict
from datetime import timezone
from zoneinfo import ZoneInfo

import faker

import rusty_faker

CASES = [
    ("first_name", (), {}),
    ("name", (), {}),
    ("address", (), {}),
    ("email", (), {}),
    ("phone_number", (), {}),
    ("company", (), {}),
    ("sentence", (), {}),
    ("text", (), {}),
    ("ipv4", (), {}),
    ("date_time", (), {}),
    ("date_of_birth", (), {}),
    ("date_time_between", ("-1y", "now"), {}),
    ("date_time (tzinfo)", (), {"tzinfo": timezone.utc}),
    ("date_time (zoneinfo)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("date_time_between (tz)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("date_time_this_year (tz)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("date_of_birth (tz)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("iso8601 (tz)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("future_datetime (tz)", (), {"tzinfo": ZoneInfo("Europe/Paris")}),
    ("random_element", (OrderedDict([("a", 0.5), ("b", 0.3), ("c", 0.2)]),), {}),
    ("bothify", ("??-####",), {}),
]


def rate(fake, method: str, args, kwargs, calls: int) -> float:
    fn = getattr(fake, method.split(" ")[0])
    fn(*args, **kwargs)  # warm caches
    start = time.perf_counter()
    for _ in range(calls):
        fn(*args, **kwargs)
    return calls / (time.perf_counter() - start)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--calls", type=int, default=100_000)
    args = parser.parse_args()

    real, rusty = faker.Faker("en_US"), rusty_faker.Faker("en_US")
    real.seed_instance(0)
    rusty.seed_instance(0)

    print(f"Faker {faker.VERSION} vs rusty_faker {rusty_faker.VERSION}, {args.calls:,} calls each\n")
    print(f"{'formatter':<22}{'Faker calls/s':>16}{'rusty calls/s':>16}{'speedup':>10}")
    for method, call_args, call_kwargs in CASES:
        real_rate = rate(real, method, call_args, call_kwargs, args.calls)
        rusty_rate = rate(rusty, method, call_args, call_kwargs, args.calls)
        print(f"{method:<22}{real_rate:>16,.0f}{rusty_rate:>16,.0f}{rusty_rate / real_rate:>9.1f}x")


if __name__ == "__main__":
    main()
