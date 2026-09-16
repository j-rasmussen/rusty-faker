"""Faker proxy, adapted from ``faker.proxy`` (MIT, Copyright (c) 2012 Daniele Faraglia).

Single-locale instances cache resolved formatter methods in their own ``__dict__``, so
``fake.name()`` becomes a dict lookup plus a direct call into Rust.
"""

from __future__ import annotations

import copy
import functools
import re
from collections import OrderedDict
from random import Random
from typing import Any, Callable, Pattern, Sequence, TypeVar

from rusty_faker.config import DEFAULT_LOCALE
from rusty_faker.exceptions import UniquenessException
from rusty_faker.factory import Factory
from rusty_faker.generator import Generator, SeedType

_UNIQUE_ATTEMPTS = 1000

RetType = TypeVar("RetType")


class _ClassOnlySeed:
    """``Faker.seed`` works on the class; accessing it on an instance raises like Faker."""

    def __get__(self, instance: Any, owner: type) -> Callable[..., None]:
        if instance is not None:
            msg = "Calling `.seed()` on instances is deprecated. Use the class method `Faker.seed()` instead."
            raise TypeError(msg)

        def seed(seed: SeedType = None) -> None:
            """
            Hashables the shared `random.Random` object across all factories

            :param seed: seed value
            """
            Generator.seed(seed)

        return seed


class Faker:
    """Proxy class capable of supporting multiple locales"""

    cache_pattern: Pattern = re.compile(r"^_cached_\w*_mapping$")
    generator_attrs = [
        attr for attr in dir(Generator) if not attr.startswith("__") and attr not in ["seed", "seed_instance", "random"]
    ]

    seed = _ClassOnlySeed()

    def __init__(
        self,
        locale: str | Sequence[str] | dict[str, int | float] | None = None,
        providers: list[str] | None = None,
        generator: Generator | None = None,
        includes: list[str] | None = None,
        use_weighting: bool = True,
        **config: Any,
    ) -> None:
        self._factory_map: OrderedDict[str, Generator | Faker] = OrderedDict()
        self._weights = None
        self._cached_names: set = set()
        self._unique_proxy = UniqueProxy(self)
        self._optional_proxy = OptionalProxy(self)

        if isinstance(locale, str):
            locales = [locale.replace("-", "_")]

        # This guarantees a FIFO ordering of elements in `locales` based on the final
        # locale string while discarding duplicates after processing
        elif isinstance(locale, (list, tuple, set)):
            locales = []
            for code in locale:
                if not isinstance(code, str):
                    raise TypeError(f'The locale "{str(code)}" must be a string.')
                final_locale = code.replace("-", "_")
                if final_locale not in locales:
                    locales.append(final_locale)

        elif isinstance(locale, (OrderedDict, dict)):
            assert all(isinstance(v, (int, float)) for v in locale.values())
            odict = OrderedDict()
            for k, v in locale.items():
                key = k.replace("-", "_")
                odict[key] = v
            locales = list(odict.keys())
            self._weights = list(odict.values())

        else:
            locales = [DEFAULT_LOCALE]

        if len(locales) == 1:
            self._factory_map[locales[0]] = Factory.create(
                locales[0],
                providers,
                generator,
                includes,
                use_weighting=use_weighting,
                **config,
            )
        else:
            for locale in locales:
                self._factory_map[locale] = Faker(
                    locale,
                    providers,
                    generator,
                    includes,
                    use_weighting=use_weighting,
                    **config,
                )

        self._locales = locales
        self._factories = list(self._factory_map.values())
        if len(self._factories) == 1:
            self._factories[0]._proxies.add(self)

    def __dir__(self):
        attributes = set(super().__dir__())
        for factory in self.factories:
            attributes |= {attr for attr in dir(factory) if not attr.startswith("_")}
        return sorted(attributes)

    def __getitem__(self, locale: str) -> Faker:
        if locale.replace("-", "_") in self.locales and len(self.locales) == 1:
            return self
        instance = self._factory_map[locale.replace("-", "_")]
        assert isinstance(instance, Faker)  # for mypy
        return instance

    def __getattr__(self, attr: str) -> Any:
        """
        Handles cache access and proxying behavior

        :param attr: attribute name
        :return: the appropriate attribute
        """
        factories = self.__dict__.get("_factories")
        if factories is None:
            raise AttributeError(attr)
        if len(factories) == 1:
            value = getattr(factories[0], attr)
            if not attr.startswith("_") and attr not in Faker.generator_attrs and callable(value):
                self.__dict__[attr] = value
                self._cached_names.add(attr)
            return value
        elif attr in self.generator_attrs:
            msg = "Proxying calls to `%s` is not implemented in multiple locale mode." % attr
            raise NotImplementedError(msg)
        elif self.cache_pattern.match(attr):
            msg = "Cached attribute `%s` does not exist" % attr
            raise AttributeError(msg)
        else:
            factory = self._select_factory(attr)
            return getattr(factory, attr)

    def _clear_cache(self) -> None:
        for name in self._cached_names:
            self.__dict__.pop(name, None)
        self._cached_names.clear()

    def __deepcopy__(self, memodict):
        cls = self.__class__
        result = cls.__new__(cls)
        memodict[id(self)] = result
        result._locales = copy.deepcopy(self._locales, memodict)
        result._factory_map = copy.deepcopy(self._factory_map, memodict)
        result._factories = list(result._factory_map.values())
        result._weights = copy.deepcopy(self._weights, memodict)
        result._cached_names = set()
        result._unique_proxy = UniqueProxy(result)
        result._unique_proxy._seen = {k: {result._unique_proxy._sentinel} for k in self._unique_proxy._seen.keys()}
        result._optional_proxy = OptionalProxy(result)
        if len(result._factories) == 1:
            result._factories[0]._proxies.add(result)
        return result

    def __getstate__(self) -> dict:
        state = {k: v for k, v in self.__dict__.items() if k not in self._cached_names}
        state["_cached_names"] = set()
        return state

    def __setstate__(self, state: Any) -> None:
        self.__dict__.update(state)
        if len(self._factories) == 1:
            self._factories[0]._proxies.add(self)

    @property
    def unique(self) -> UniqueProxy:
        return self._unique_proxy

    @property
    def optional(self) -> OptionalProxy:
        return self._optional_proxy

    def _chance(self, percent: int) -> bool:
        factory = self._factories[0]
        while isinstance(factory, Faker):
            factory = factory._factories[0]
        return factory._core._chance(percent)

    def _select_factory(self, method_name: str) -> Generator:
        """
        Returns a random factory that supports the provider method

        :param method_name: Name of provider method
        :return: A factory that supports the provider method
        """

        factories, weights = self._map_provider_method(method_name)

        if len(factories) == 0:
            msg = f"No generator object has attribute {method_name!r}"
            raise AttributeError(msg)
        elif len(factories) == 1:
            return factories[0]

        if weights:
            factory = self._select_factory_distribution(factories, weights)
        else:
            factory = self._select_factory_choice(factories)
        return factory

    def _select_factory_distribution(self, factories, weights):
        return self.factories[0].random.choices(factories, weights=weights, k=1)[0]

    def _select_factory_choice(self, factories):
        return self._factories[0].random.choice(factories)

    def _map_provider_method(self, method_name: str) -> tuple[list[Generator], list[float] | None]:
        """
        Creates a 2-tuple of factories and weights for the given provider method name

        The first element of the tuple contains a list of compatible factories.
        The second element of the tuple contains a list of distribution weights.

        :param method_name: Name of provider method
        :return: 2-tuple (factories, weights)
        """

        # Return cached mapping if it exists for given method
        attr = f"_cached_{method_name}_mapping"
        if hasattr(self, attr):
            return getattr(self, attr)

        # Create mapping if it does not exist
        if self._weights:
            value = [
                (factory, weight)
                for factory, weight in zip(self.factories, self._weights)
                if hasattr(factory, method_name)
            ]
            factories, weights = zip(*value) if value else ((), ())
            mapping = list(factories), list(weights)
        else:
            value = [factory for factory in self.factories if hasattr(factory, method_name)]  # type: ignore
            mapping = value, None  # type: ignore

        # Then cache and return results
        setattr(self, attr, mapping)
        return mapping

    def seed_instance(self, seed: SeedType | None = None) -> None:
        """
        Creates and seeds a new `random.Random` object for each factory

        :param seed: seed value
        """
        for factory in self._factories:
            factory.seed_instance(seed)

    def seed_locale(self, locale: str, seed: SeedType | None = None) -> None:
        """
        Creates and seeds a new `random.Random` object for the factory of the specified locale

        :param locale: locale string
        :param seed: seed value
        """
        self._factory_map[locale.replace("-", "_")].seed_instance(seed)

    @property
    def random(self) -> Random:
        """
        Proxies `random` getter calls

        In single locale mode, this will be proxied to the `random` getter
        of the only internal `Generator` object. Subclasses will have to
        implement desired behavior in multiple locale mode.
        """

        if len(self._factories) == 1:
            return self._factories[0].random
        else:
            msg = "Proxying `random` getter calls is not implemented in multiple locale mode."
            raise NotImplementedError(msg)

    @random.setter
    def random(self, value: Random) -> None:
        """
        Proxies `random` setter calls

        In single locale mode, this will be proxied to the `random` setter
        of the only internal `Generator` object. Subclasses will have to
        implement desired behavior in multiple locale mode.
        """

        if len(self._factories) == 1:
            self._factories[0].random = value
        else:
            msg = "Proxying `random` setter calls is not implemented in multiple locale mode."
            raise NotImplementedError(msg)

    @property
    def locales(self) -> list[str]:
        return list(self._locales)

    @property
    def weights(self) -> list[int | float] | None:
        return self._weights

    @property
    def factories(self) -> list[Generator | Faker]:
        return self._factories

    def items(self) -> list[tuple[str, Generator | Faker]]:
        return list(self._factory_map.items())


class UniqueProxy:
    def __init__(self, proxy: Faker, excluded_types: tuple[type, ...] = ()):
        self._proxy = proxy
        self._seen: dict = {}
        self._sentinel = object()
        self._excluded_types = excluded_types

    def clear(self) -> None:
        self._seen = {}

    def exclude_types(self, types: list[type]) -> UniqueProxy:
        """Return new UniqueProxy excluding specified types from uniqueness checks."""
        new_proxy = UniqueProxy(self._proxy, tuple(types))
        new_proxy._seen = self._seen
        new_proxy._sentinel = self._sentinel
        return new_proxy

    def __getitem__(self, locale: str) -> UniqueProxy:
        locale_proxy = self._proxy[locale]
        unique_proxy = UniqueProxy(locale_proxy, self._excluded_types)
        unique_proxy._seen = self._seen
        unique_proxy._sentinel = self._sentinel
        return unique_proxy

    def __getattr__(self, name: str) -> Any:
        obj = getattr(self._proxy, name)
        if callable(obj):
            return self._wrap(name, obj)
        else:
            raise TypeError("Accessing non-functions through .unique is not supported.")

    def __getstate__(self):
        return self.__dict__.copy()

    def __setstate__(self, state):
        self.__dict__.update(state)

    def _make_hashable(self, value: Any) -> Any:
        """Convert unhashable types (e.g., dict) to a hashable representation."""
        if isinstance(value, dict):
            return tuple(sorted((k, self._make_hashable(v)) for k, v in value.items()))
        elif isinstance(value, list):
            return tuple(self._make_hashable(v) for v in value)
        elif isinstance(value, set):
            return frozenset(self._make_hashable(v) for v in value)
        return value

    def _wrap(self, name: str, function: Callable) -> Callable:
        @functools.wraps(function)
        def wrapper(*args, **kwargs):
            if self._excluded_types:
                retval = function(*args, **kwargs)
                if isinstance(retval, self._excluded_types):
                    return retval
                hashable_retval = self._make_hashable(retval)
                key = (name, args, tuple(sorted(kwargs.items())))
                generated = self._seen.setdefault(key, {self._sentinel})
                if hashable_retval not in generated:
                    generated.add(hashable_retval)
                    return retval
            else:
                key = (name, args, tuple(sorted(kwargs.items())))
                generated = self._seen.setdefault(key, {self._sentinel})
                retval = self._sentinel
                hashable_retval = self._make_hashable(retval)

            for i in range(_UNIQUE_ATTEMPTS):
                if hashable_retval not in generated:
                    break
                retval = function(*args, **kwargs)
                hashable_retval = self._make_hashable(retval)
            else:
                raise UniquenessException(f"Got duplicated values after {_UNIQUE_ATTEMPTS:,} iterations.")

            generated.add(hashable_retval)

            return retval

        return wrapper


class OptionalProxy:
    """
    Return either a fake value or None, with a customizable probability.
    """

    def __init__(self, proxy: Faker):
        self._proxy = proxy

    def __getattr__(self, name: str) -> Any:
        obj = getattr(self._proxy, name)
        if callable(obj):
            return self._wrap(name, obj)
        else:
            raise TypeError("Accessing non-functions through .optional is not supported.")

    def __getstate__(self):
        return self.__dict__.copy()

    def __setstate__(self, state):
        self.__dict__.update(state)

    def _wrap(self, name: str, function: Callable[..., RetType]) -> Callable[..., RetType | None]:
        @functools.wraps(function)
        def wrapper(*args: Any, prob: float = 0.5, **kwargs: Any) -> RetType | None:
            if not 0 < prob <= 1.0:
                raise ValueError("prob must be between 0 and 1")
            return function(*args, **kwargs) if self._proxy._chance(int(prob * 100)) else None

        return wrapper
