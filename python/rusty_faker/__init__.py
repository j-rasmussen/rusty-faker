"""rusty-faker: a Rust-backed drop-in replacement for Faker.

    from rusty_faker import Faker
    fake = Faker()
    fake.name()
"""

# The version lives in Cargo.toml (`[workspace.package] version`) and reaches Python
# through the compiled module, so a release bumps exactly one file.
from rusty_faker._core import __version__
from rusty_faker.factory import Factory
from rusty_faker.generator import Generator
from rusty_faker.proxy import Faker

VERSION = __version__

__all__ = ("Factory", "Generator", "Faker", "VERSION", "__version__")
