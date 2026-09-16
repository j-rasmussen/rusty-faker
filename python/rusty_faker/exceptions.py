"""Exceptions, mirroring ``faker.exceptions``."""

from rusty_faker._core import ParseError


class BaseFakerException(Exception):
    """The base exception for all Faker exceptions."""


class UniquenessException(BaseFakerException):
    """Raised by ``.unique`` after too many attempts to find a new value."""


class UnsupportedFeature(BaseFakerException):
    """The requested feature is not available on this system."""

    def __init__(self, msg: str, name: str) -> None:
        self.name = name
        super().__init__(msg)


__all__ = ["BaseFakerException", "ParseError", "UniquenessException", "UnsupportedFeature"]
