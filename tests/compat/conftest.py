"""Shared fixtures: the real Faker (pinned in the dev extras) and rusty_faker side by side."""

import inspect

import faker
import pytest

import rusty_faker

V1_PROVIDER_MODULES = ("person", "address", "internet", "company", "lorem", "phone_number", "date_time")


def v1_formatter_names() -> list[str]:
    """Public formatter names Faker exposes for the v1 providers (plus BaseProvider)."""
    fake = faker.Faker("en_US")
    names: set[str] = set()
    for provider in fake.get_providers():
        module = type(provider).__module__
        in_scope = module == "faker.providers" or any(f"faker.providers.{m}" in module for m in V1_PROVIDER_MODULES)
        if not in_scope:
            continue
        for cls in type(provider).__mro__:
            if not cls.__module__.startswith("faker.providers"):
                continue
            if cls.__module__ != "faker.providers" and not any(f"faker.providers.{m}" in cls.__module__ for m in V1_PROVIDER_MODULES):
                continue
            for name, member in vars(cls).items():
                if not name.startswith("_") and callable(member):
                    names.add(name)
    return sorted(names)


V1_FORMATTERS = v1_formatter_names()


@pytest.fixture
def real():
    fake = faker.Faker("en_US")
    fake.seed_instance(1234)
    return fake


@pytest.fixture
def rusty():
    fake = rusty_faker.Faker("en_US")
    fake.seed_instance(1234)
    return fake


def required_params(func) -> list[str]:
    return [
        p.name
        for p in inspect.signature(func).parameters.values()
        if p.default is inspect.Parameter.empty and p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD, p.KEYWORD_ONLY)
    ]
