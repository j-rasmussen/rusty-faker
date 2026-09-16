"""API surface parity with Faker for the v1 providers."""

import inspect
import types

import faker
import pytest

import rusty_faker
from tests.compat.conftest import V1_FORMATTERS, required_params

# Formatters whose return type legitimately varies between calls.
MIXED_RETURN = {"random_digit_or_empty", "random_digit_not_null_or_empty", "random_element", "random_elements",
                "random_choices", "random_sample", "time_series"}


def test_v1_formatter_list_is_populated():
    assert len(V1_FORMATTERS) > 150
    assert {"name", "address", "email", "date_time", "sentence"} <= set(V1_FORMATTERS)


@pytest.mark.parametrize("name", V1_FORMATTERS)
def test_formatter_exists(rusty, name):
    assert callable(getattr(rusty, name))


@pytest.mark.parametrize("name", V1_FORMATTERS)
def test_parameter_names_match(real, rusty, name):
    expected = [p for p in inspect.signature(getattr(real, name)).parameters]
    actual = [p for p in inspect.signature(getattr(rusty, name)).parameters]
    assert actual == expected


@pytest.mark.parametrize("name", V1_FORMATTERS)
def test_return_types_match(real, rusty, name):
    real_fn, rusty_fn = getattr(real, name), getattr(rusty, name)
    if required_params(real_fn):
        pytest.skip("needs arguments")
    real_value, rusty_value = real_fn(), rusty_fn()
    if name in MIXED_RETURN:
        return
    if isinstance(real_value, types.GeneratorType):
        assert inspect.isgenerator(rusty_value) or hasattr(rusty_value, "__next__")
        return
    assert type(rusty_value) is type(real_value), f"{name}: {rusty_value!r} vs {real_value!r}"
    if isinstance(real_value, list) and real_value:
        assert type(rusty_value[0]) is type(real_value[0])


def test_public_names_on_package():
    for name in ("Faker", "Generator", "Factory"):
        assert hasattr(rusty_faker, name) and hasattr(faker, name)
    from rusty_faker.exceptions import UniquenessException  # noqa: F401
    from rusty_faker.providers import BaseProvider, DynamicProvider  # noqa: F401


def test_dir_lists_formatters(rusty):
    listing = dir(rusty)
    assert "name" in listing and "date_of_birth" in listing
