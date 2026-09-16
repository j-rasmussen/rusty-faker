"""Provider base classes, mirroring ``faker.providers``.

Custom providers subclass ``BaseProvider``; its random helpers draw from the generator's
Rust RNG, so seeding a ``Faker`` also makes custom providers repeatable.

MIT, Copyright (c) 2012 Daniele Faraglia; see NOTICE.
"""

from __future__ import annotations

import string
from collections import OrderedDict
from typing import Any, Collection, List, Optional, Sequence, TypeVar, Union

from rusty_faker.generator import Generator

T = TypeVar("T")
ElementsType = Union[Collection[T], OrderedDict[T, float]]


def _core(provider: "BaseProvider") -> Any:
    core = getattr(provider.generator, "_core", None)
    if core is None:
        raise TypeError("rusty_faker providers need a rusty_faker Generator")
    return core


class BaseProvider:
    __provider__ = "base"
    __lang__: Optional[str] = None
    __use_weighting__ = False

    def __init__(self, generator: Any) -> None:
        """
        Base class for fake data providers
        :param generator: `Generator` instance
        """
        self.generator = generator

    def locale(self) -> str:
        return _core(self).locale()

    def language_code(self) -> str:
        return _core(self).language_code()

    def random_int(self, min: int = 0, max: int = 9999, step: int = 1) -> int:
        return _core(self).random_int(min, max, step)

    def random_digit(self) -> int:
        return _core(self).random_digit()

    def random_digit_not_null(self) -> int:
        return _core(self).random_digit_not_null()

    def random_digit_above_two(self) -> int:
        return _core(self).random_digit_above_two()

    def random_digit_or_empty(self) -> Union[int, str]:
        return _core(self).random_digit_or_empty()

    def random_digit_not_null_or_empty(self) -> Union[int, str]:
        return _core(self).random_digit_not_null_or_empty()

    def random_number(self, digits: Optional[int] = None, fix_len: bool = False) -> int:
        return _core(self).random_number(digits, fix_len)

    def random_letter(self) -> str:
        return _core(self).random_letter()

    def random_letters(self, length: int = 16) -> Sequence[str]:
        return _core(self).random_letters(length)

    def random_lowercase_letter(self) -> str:
        return _core(self).random_lowercase_letter()

    def random_uppercase_letter(self) -> str:
        return _core(self).random_uppercase_letter()

    def random_elements(
        self,
        elements: ElementsType[T] = ("a", "b", "c"),  # type: ignore[assignment]
        length: Optional[int] = None,
        unique: bool = False,
        use_weighting: Optional[bool] = None,
    ) -> Sequence[T]:
        use_weighting = use_weighting if use_weighting is not None else self.__use_weighting__
        return _core(self).random_elements(elements, length, unique, use_weighting)

    def random_choices(
        self,
        elements: ElementsType[T] = ("a", "b", "c"),  # type: ignore[assignment]
        length: Optional[int] = None,
    ) -> Sequence[T]:
        return self.random_elements(elements, length, unique=False)

    def random_element(self, elements: ElementsType[T] = ("a", "b", "c")) -> T:  # type: ignore[assignment]
        return self.random_elements(elements, length=1)[0]

    def random_sample(
        self, elements: ElementsType[T] = ("a", "b", "c"), length: Optional[int] = None  # type: ignore[assignment]
    ) -> Sequence[T]:
        return self.random_elements(elements, length, unique=True)

    def randomize_nb_elements(
        self,
        number: int = 10,
        le: bool = False,
        ge: bool = False,
        min: Optional[int] = None,
        max: Optional[int] = None,
    ) -> int:
        return _core(self).randomize_nb_elements(number, le, ge, min, max)

    def numerify(self, text: str = "###") -> str:
        return _core(self).numerify(text)

    def lexify(self, text: str = "????", letters: str = string.ascii_letters) -> str:
        return _core(self).lexify(text, letters)

    def bothify(self, text: str = "## ??", letters: str = string.ascii_letters) -> str:
        return _core(self).bothify(text, letters)

    def hexify(self, text: str = "^^^^", upper: bool = False) -> str:
        return _core(self).hexify(text, upper)


class DynamicProvider(BaseProvider):
    def __init__(
        self,
        provider_name: str,
        elements: Optional[List] = None,
        generator: Optional[Any] = None,
    ):
        """
        A faker Provider capable of getting a list of elements to randomly select from,
        instead of using the predefined list of elements which exist in the default providers in faker.

        :param provider_name: Name of provider, which would translate into the function name e.g. faker.my_fun().
        :param elements: List of values to randomly select from
        :param generator: Generator object. If missing, the default Generator is used.
        """

        if not generator:
            generator = Generator()

        super().__init__(generator)
        if provider_name.startswith("__"):
            raise ValueError("Provider name cannot start with __ as it would be ignored by Faker")

        self.provider_name = provider_name

        self.elements = []
        if elements:
            self.elements = elements

        setattr(self, provider_name, self.get_random_value)  # Add a method for the provider_name value

    def add_element(self, element: str) -> None:
        """Add new element."""
        self.elements.append(element)

    def get_random_value(self, use_weighting: bool = True) -> Any:
        """Returns a random value for this provider.

        :param use_weighting: boolean option to use weighting. Defaults to True
        """
        if not self.elements or len(self.elements) == 0:
            raise ValueError("Elements should be a list of values the provider samples from")

        return self.random_elements(self.elements, length=1, use_weighting=use_weighting)[0]


__all__ = ["BaseProvider", "DynamicProvider", "ElementsType"]
