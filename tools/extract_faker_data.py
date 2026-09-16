#!/usr/bin/env python3
"""Extract rusty-faker's compiled-in data tables from a pinned Python Faker release.

Every public, non-callable data attribute of each supported provider is dumped
(with locale inheritance already resolved) to
``crates/rusty-faker-core/data/<locale>/<provider>.json``. ``build.rs`` in the
core crate turns these files into static Rust tables.

The output is derived from Faker's data tables and inherits Faker's MIT license;
see NOTICE.

Usage:
    .venv/bin/python tools/extract_faker_data.py [--locale en_US ...] [--out DIR]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import OrderedDict
from pathlib import Path
from typing import Any

import faker
from faker import Faker
from faker.providers import BaseProvider
from faker.typing import Country

PINNED_FAKER_VERSION = "40.39.0"
PROVIDERS = ("person", "address", "company", "internet", "lorem", "phone_number", "date_time")
DEFAULT_OUT = Path(__file__).resolve().parent.parent / "crates" / "rusty-faker-core" / "data"
TOKEN_RE = re.compile(r"\{\{\s*(\w+)(:\s*\w+?)?\s*\}\}")


class UnsupportedValue(Exception):
    pass


def _is_str_seq(value: Any) -> bool:
    return isinstance(value, (list, tuple)) and all(isinstance(v, str) for v in value)


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def encode(value: Any) -> dict[str, Any]:
    if isinstance(value, str):
        return {"type": "str", "value": value}
    if isinstance(value, OrderedDict) and all(_is_number(v) for v in value.values()):
        return {"type": "weighted", "items": list(value.keys()), "weights": [float(v) for v in value.values()]}
    if isinstance(value, dict):
        values = list(value.values())
        if all(_is_str_seq(v) for v in values):
            return {"type": "str_list_map", "entries": [[k, list(v)] for k, v in value.items()]}
        if all(isinstance(v, tuple) and len(v) == 2 and all(_is_number(x) for x in v) for v in values):
            return {"type": "int_range_map", "entries": [[k, int(v[0]), int(v[1])] for k, v in value.items()]}
        raise UnsupportedValue(f"dict with values like {values[:1]!r}")
    if isinstance(value, (list, tuple)):
        if _is_str_seq(value):
            return {"type": "str_list", "items": list(value)}
        if all(_is_number(v) and isinstance(v, int) for v in value):
            return {"type": "int_list", "items": list(value)}
        if all(_is_str_seq(v) for v in value):
            return {"type": "str_list_list", "items": [list(v) for v in value]}
        if all(isinstance(v, Country) for v in value):
            return {
                "type": "countries",
                "items": [
                    {
                        "name": c.name,
                        "alpha_2_code": c.alpha_2_code,
                        "alpha_3_code": c.alpha_3_code,
                        "continent": c.continent,
                        "capital": c.capital,
                        "timezones": list(c.timezones),
                    }
                    for c in value
                ],
            }
        raise UnsupportedValue(f"sequence with items like {list(value)[:1]!r}")
    raise UnsupportedValue(type(value).__name__)


def iter_strings(encoded: dict[str, Any]):
    kind = encoded["type"]
    if kind == "str":
        yield encoded["value"]
    elif kind in ("weighted", "str_list"):
        yield from encoded["items"]
    elif kind == "str_list_list":
        for group in encoded["items"]:
            yield from group
    elif kind == "str_list_map":
        for key, items in encoded["entries"]:
            yield key
            yield from items


def dump_attributes(obj: Any, skip_base: bool) -> tuple[dict[str, Any], list[str]]:
    attributes: dict[str, Any] = {}
    skipped: list[str] = []
    for name in sorted(dir(obj)):
        if name.startswith("_") or name == "generator":
            continue
        value = getattr(obj, name)
        if callable(value):
            continue
        if skip_base and getattr(BaseProvider, name, None) is value:
            continue
        try:
            attributes[name] = encode(value)
        except UnsupportedValue as exc:
            skipped.append(f"{name} ({exc})")
    return attributes, skipped


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=1, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")


def extract_locale(locale: str, out: Path) -> None:
    fake = Faker(locale)
    by_provider = {type(p).__module__.split(".")[2]: p for p in fake.get_providers() if type(p).__module__.count(".") >= 2}
    tokens: set[str] = set()
    non_ascii: dict[str, int] = {}

    base_attrs, _ = dump_attributes(BaseProvider, skip_base=False)
    write_json(out / locale / "base.json", {"faker_version": faker.VERSION, "locale": locale, "provider": "base", "attributes": base_attrs})

    for provider in PROVIDERS:
        instance = by_provider.get(provider)
        if instance is None:
            sys.exit(f"provider {provider!r} not found for locale {locale!r}")
        attributes, skipped = dump_attributes(instance, skip_base=True)
        for name, encoded in attributes.items():
            for s in iter_strings(encoded):
                tokens.update(m.group(1) for m in TOKEN_RE.finditer(s))
                if not s.isascii():
                    non_ascii[f"{provider}.{name}"] = non_ascii.get(f"{provider}.{name}", 0) + 1
        write_json(
            out / locale / f"{provider}.json",
            {
                "faker_version": faker.VERSION,
                "locale": locale,
                "provider": provider,
                "provider_module": type(instance).__module__,
                "attributes": attributes,
            },
        )
        print(f"{locale}/{provider}: {len(attributes)} attributes" + (f"; skipped: {', '.join(skipped)}" if skipped else ""))

    print(f"{locale} template tokens: {', '.join(sorted(tokens))}")
    print(f"{locale} non-ASCII strings: {non_ascii or 'none'}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--locale", action="append", dest="locales", help="locale to extract (repeatable, default en_US)")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help="output data directory")
    parser.add_argument("--allow-version-mismatch", action="store_true", help=f"allow a Faker other than {PINNED_FAKER_VERSION}")
    args = parser.parse_args()

    if faker.VERSION != PINNED_FAKER_VERSION and not args.allow_version_mismatch:
        sys.exit(f"expected Faker {PINNED_FAKER_VERSION}, found {faker.VERSION}")
    for locale in args.locales or ["en_US"]:
        extract_locale(locale, args.out)


if __name__ == "__main__":
    main()
