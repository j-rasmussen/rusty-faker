"""rusty-faker: a Rust-backed drop-in replacement for Faker.

    from rusty_faker import Faker
    fake = Faker()
    fake.name()
"""

from rusty_faker.factory import Factory
from rusty_faker.generator import Generator
from rusty_faker.proxy import Faker

VERSION = "0.1.0"
__version__ = VERSION

__all__ = ("Factory", "Generator", "Faker")
