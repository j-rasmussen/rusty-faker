"""Factory, mirroring ``faker.factory.Factory``.

MIT, Copyright (c) 2012 Daniele Faraglia; see NOTICE.
"""

from __future__ import annotations

import importlib
import logging
import re
from typing import Any, List, Optional

from rusty_faker.config import AVAILABLE_LOCALES, DEFAULT_LOCALE
from rusty_faker.generator import Generator

logger = logging.getLogger(__name__)

_LOCALE_RE = re.compile(r"^[A-Za-z]{2,3}(_[A-Za-z0-9]+)*$")


def resolve_locale(locale: Optional[str]) -> str:
    """Normalizes a locale code and maps it to compiled-in data (falling back like Faker)."""
    locale = (locale or DEFAULT_LOCALE).replace("-", "_")
    if not _LOCALE_RE.match(locale):
        msg = f"Invalid configuration for faker locale `{locale}`"
        raise AttributeError(msg)
    if locale not in AVAILABLE_LOCALES:
        logger.debug("Specified locale `%s` is not available in rusty-faker. Locale reset to `%s`", locale, DEFAULT_LOCALE)
        return DEFAULT_LOCALE
    return locale


def _import_providers(names: Optional[List[str]]) -> List[Any]:
    providers = []
    for name in names or []:
        if name == "faker.providers" or name.startswith("faker.providers."):
            continue  # built into the Rust core
        module = importlib.import_module(name)
        provider = getattr(module, "Provider", None)
        if provider is None:
            raise AttributeError(f"Module {name!r} has no Provider class")
        providers.append(provider)
    return providers


class Factory:
    @classmethod
    def create(
        cls,
        locale: Optional[str] = None,
        providers: Optional[List[str]] = None,
        generator: Optional[Generator] = None,
        includes: Optional[List[str]] = None,
        # Should we use weightings (more realistic) or weight every element equally (faster)?
        # By default, use weightings for backwards compatibility & realism
        use_weighting: bool = True,
        **config: Any,
    ) -> Generator:
        requested = (locale or DEFAULT_LOCALE).replace("-", "_")
        data_locale = resolve_locale(requested)
        config["locale"] = requested
        faker = generator or Generator(use_weighting=use_weighting, _data_locale=data_locale, **config)
        for provider in _import_providers(list(providers or []) + list(includes or [])):
            faker.add_provider(provider)
        return faker
