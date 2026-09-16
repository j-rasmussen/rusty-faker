"""Generator: resolves formatters to custom Python providers first, then the Rust core.

Mirrors ``faker.generator.Generator``. Resolved bound methods are cached on the instance
(and on single-locale ``Faker`` proxies) so repeat calls skip Python-level dispatch.

MIT, Copyright (c) 2012 Daniele Faraglia; see NOTICE.
"""

from __future__ import annotations

import hashlib
import random as random_module
import re
import weakref
from typing import TYPE_CHECKING, Any, Callable, Dict, Hashable, List, Optional, Type, Union

from rusty_faker import _core
from rusty_faker.config import DEFAULT_LOCALE

if TYPE_CHECKING:
    from rusty_faker.providers import BaseProvider

SeedType = Union[int, float, str, bytes, bytearray, None]

_re_token = re.compile(r"\{\{\s*(\w+)(:\s*\w+?)?\s*\}\}")
random = random_module.Random()
mod_random = random  # compat with the name Faker released in 0.8
Sentinel = object()
_U64 = (1 << 64) - 1


def seed_to_u64(seed: SeedType) -> Optional[int]:
    """Maps any seed ``random.seed`` accepts to a stable 64-bit seed (``None`` = entropy)."""
    if seed is None:
        return None
    if isinstance(seed, int):
        seed = abs(seed)
        if seed <= _U64:
            return seed
        data = seed.to_bytes((seed.bit_length() + 7) // 8, "little")
    elif isinstance(seed, float):
        data = repr(seed).encode()
    elif isinstance(seed, str):
        data = seed.encode("utf-8")
    elif isinstance(seed, (bytes, bytearray)):
        data = bytes(seed)
    else:
        raise TypeError("The only supported seed types are: None, int, float, str, bytes, and bytearray.")
    return int.from_bytes(hashlib.blake2b(data, digest_size=8).digest(), "little")


class Generator:
    __config: Dict[str, Dict[Hashable, Any]] = {
        "arguments": {},
    }

    _is_seeded = False
    _global_seed: Any = Sentinel
    # Rust generators shared by every Generator that has not called seed_instance(),
    # keyed by (locale, use_weighting); the Rust counterpart of Faker's module-level Random.
    _shared_cores: Dict[tuple, Any] = {}

    def __init__(self, use_weighting: bool = True, **config: Any) -> None:
        # `_data_locale` is the compiled-in locale chosen by Factory; `locale` in config is
        # what the user asked for (kept for error messages, like Faker).
        data_locale = config.pop("_data_locale", None) or DEFAULT_LOCALE
        self.providers: List["BaseProvider"] = []
        self.__config = dict(list(self.__config.items()) + list(config.items()))
        self.__random = random
        self._data_locale = data_locale
        self._use_weighting = use_weighting
        self._core = self._shared_core(data_locale, use_weighting)
        self._cached: set = set()
        self._provider_methods: set = set()
        self._proxies: "weakref.WeakSet[Any]" = weakref.WeakSet()

    @classmethod
    def _shared_core(cls, locale: str, use_weighting: bool) -> Any:
        key = (locale, use_weighting)
        core = cls._shared_cores.get(key)
        if core is None:
            seed = None if cls._global_seed is Sentinel else seed_to_u64(cls._global_seed)
            core = _core.Generator(locale, seed)
            core._use_weighting = use_weighting
            cls._shared_cores[key] = core
        return core

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(f"'Generator' object has no attribute {name!r}")
        core = self.__dict__.get("_core")
        try:
            value = getattr(core, name)
        except AttributeError:
            raise AttributeError(f"'Generator' object has no attribute {name!r}") from None
        self.__dict__[name] = value
        self._cached.add(name)
        return value

    def __dir__(self) -> List[str]:
        return sorted(set(super().__dir__()) | set(_core.Generator._formatter_names()))

    def __deepcopy__(self, memo: dict) -> "Generator":
        import copy

        result = self.__class__.__new__(self.__class__)
        memo[id(self)] = result
        for key, value in self.__dict__.items():
            if key in self._cached:
                continue
            if key == "_proxies":
                result.__dict__[key] = weakref.WeakSet()
            elif key == "_cached":
                result.__dict__[key] = set()
            else:
                result.__dict__[key] = copy.deepcopy(value, memo)
        for name in self._provider_methods:
            provider_method = getattr(self, name)
            owner = getattr(provider_method, "__self__", None)
            if owner is not None and id(owner) in memo:
                result.__dict__[name] = getattr(memo[id(owner)], name)
        return result

    def __getstate__(self) -> dict:
        state = {k: v for k, v in self.__dict__.items() if k not in self._cached and k not in ("_proxies", "_core")}
        state["_cached"] = set()
        state["_core_is_shared"] = self._core is Generator._shared_cores.get((self._data_locale, self._use_weighting))
        return state

    def __setstate__(self, state: dict) -> None:
        shared = state.pop("_core_is_shared", True)
        self.__dict__.update(state)
        self._proxies = weakref.WeakSet()
        if shared:
            self._core = self._shared_core(self._data_locale, self._use_weighting)
        else:
            self._core = _core.Generator(self._data_locale)
            self._core._use_weighting = self._use_weighting

    def _invalidate(self) -> None:
        """Drops cached bound methods here and on every proxy that cached them."""
        for name in self._cached:
            if name not in self._provider_methods:
                self.__dict__.pop(name, None)
        self._cached.clear()
        for proxy in list(self._proxies):
            proxy._clear_cache()

    def add_provider(self, provider: Union["BaseProvider", Type["BaseProvider"]]) -> None:
        if isinstance(provider, type):
            provider = provider(self)

        self.providers.insert(0, provider)

        for method_name in dir(provider):
            # skip 'private' method
            if method_name.startswith("_"):
                continue

            faker_function = getattr(provider, method_name)

            if callable(faker_function):
                # add all faker method to generator
                self.set_formatter(method_name, faker_function)
        self._invalidate()

    def provider(self, name: str) -> Optional["BaseProvider"]:
        try:
            lst = [p for p in self.get_providers() if hasattr(p, "__provider__") and p.__provider__ == name.lower()]
            return lst[0]
        except IndexError:
            return None

    def get_providers(self) -> List["BaseProvider"]:
        """Returns added (Python) providers. Built-in providers live in the Rust core."""
        return self.providers

    @property
    def random(self) -> random_module.Random:
        return self.__random

    @random.setter
    def random(self, value: random_module.Random) -> None:
        self.__random = value

    def seed_instance(self, seed: SeedType = None) -> "Generator":
        """Gives this generator its own seeded RNGs (Rust core and ``random.Random``)."""
        if self.__random == random:
            # create per-instance random obj when first time seed_instance() is called
            self.__random = random_module.Random()
        self.__random.seed(seed)
        if self._core is Generator._shared_cores.get((self._data_locale, self._use_weighting)):
            self._core = _core.Generator(self._data_locale, seed_to_u64(seed))
            self._core._use_weighting = self._use_weighting
            self._invalidate()
        else:
            self._core._seed(seed_to_u64(seed))
        self._is_seeded = True
        return self

    @classmethod
    def seed(cls, seed: SeedType = None) -> None:
        random.seed(seed)
        cls._global_seed = seed
        cls._is_seeded = True
        u64 = seed_to_u64(seed)
        for core in cls._shared_cores.values():
            core._seed(u64)

    def format(self, formatter: str, *args: Any, **kwargs: Any) -> str:
        """
        This is a secure way to make a fake from another Provider.
        """
        return self.get_formatter(formatter)(*args, **kwargs)

    def get_formatter(self, formatter: str) -> Callable:
        try:
            return getattr(self, formatter)
        except AttributeError:
            if "locale" in self.__config:
                msg = f'Unknown formatter {formatter!r} with locale {self.__config["locale"]!r}'
            else:
                raise AttributeError(f"Unknown formatter {formatter!r}")
            raise AttributeError(msg)

    def set_formatter(self, name: str, formatter: Callable) -> None:
        """
        This method adds a provider method to generator.
        Override this method to add some decoration or logging stuff.
        """
        setattr(self, name, formatter)
        self._provider_methods.add(name)
        self._cached.discard(name)

    def set_arguments(self, group: str, argument: Union[str, Dict[str, Any]], value: Optional[Any] = None) -> None:
        """
        Creates an argument group, with an individual argument or a dictionary
        of arguments. The argument groups is used to apply arguments to tokens,
        when using the generator.parse() method. To further manage argument
        groups, use get_arguments() and del_arguments() methods.

        generator.set_arguments('small', 'max_value', 10)
        generator.set_arguments('small', {'min_value': 5, 'max_value': 10})
        """
        if group not in self.__config["arguments"]:
            self.__config["arguments"][group] = {}

        if isinstance(argument, dict):
            self.__config["arguments"][group] = argument
        elif not isinstance(argument, str):
            raise ValueError("Arguments must be either a string or dictionary")
        else:
            self.__config["arguments"][group][argument] = value

    def get_arguments(self, group: str, argument: Optional[str] = None) -> Any:
        """
        Get the value of an argument configured within a argument group, or
        the entire group as a dictionary. Used in conjunction with the
        set_arguments() method.

        generator.get_arguments('small', 'max_value')
        generator.get_arguments('small')
        """
        if group in self.__config["arguments"] and argument:
            result = self.__config["arguments"][group].get(argument)
        else:
            result = self.__config["arguments"].get(group)

        return result

    def del_arguments(self, group: str, argument: Optional[str] = None) -> Any:
        """
        Delete an argument from an argument group or the entire argument group.
        Used in conjunction with the set_arguments() method.

        generator.del_arguments('small')
        generator.del_arguments('small', 'max_value')
        """
        if group in self.__config["arguments"]:
            if argument:
                result = self.__config["arguments"][group].pop(argument)
            else:
                result = self.__config["arguments"].pop(group)
        else:
            result = None

        return result

    def parse(self, text: str) -> str:
        """
        Replaces tokens like '{{ tokenName }}' or '{{tokenName}}' in a string with
        the result from the token method call. Arguments can be parsed by using an
        argument group. For more information on the use of argument groups, please
        refer to the set_arguments() method.

        Example:

        generator.set_arguments('red_rgb', {'hue': 'red', 'color_format': 'rgb'})
        generator.set_arguments('small', 'max_value', 10)

        generator.parse('{{ color:red_rgb }} - {{ pyint:small }}')
        """
        return _re_token.sub(self.__format_token, text)

    def __format_token(self, matches):
        formatter, argument_group = list(matches.groups())
        argument_group = argument_group.lstrip(":").strip() if argument_group else ""

        if argument_group:
            try:
                arguments = self.__config["arguments"][argument_group]
            except KeyError:
                raise AttributeError(f"Unknown argument group {argument_group!r}")

            formatted = str(self.format(formatter, **arguments))
        else:
            formatted = str(self.format(formatter))

        return "".join(formatted)

    # Date/time formatters that need Python objects end to end.

    def pytimezone(self, *args: Any, **kwargs: Any) -> Any:
        """Random timezone as a ``zoneinfo.ZoneInfo`` usable as ``tzinfo``."""
        from rusty_faker import _tz

        return _tz.pytimezone(self, *args, **kwargs)

    def time_series(
        self,
        start_date: Any = "-30d",
        end_date: Any = "now",
        precision: Optional[float] = None,
        distrib: Optional[Callable] = None,
        tzinfo: Any = None,
    ) -> Any:
        """Yields ``(datetime, value)`` tuples from ``start_date`` to ``end_date``."""
        from rusty_faker import _tz

        return _tz.time_series(self._core, start_date, end_date, precision, distrib, tzinfo)
