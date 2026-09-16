"""Behavioral compatibility: value shapes, arguments, errors, seeding and the proxy features."""

import copy
import ipaddress
import pickle
import re
from collections import OrderedDict
from datetime import date, datetime, time, timedelta, timezone
from zoneinfo import ZoneInfo

import pytest

from rusty_faker import Faker
from rusty_faker.exceptions import ParseError, UniquenessException
from rusty_faker.providers import BaseProvider, DynamicProvider

N = 300


def test_value_shapes(rusty):
    email_re = re.compile(r"^[a-z0-9._-]+@[a-z0-9.-]+\.[a-z]+$")
    for _ in range(N):
        assert email_re.match(rusty.email()), rusty.email()
        assert email_re.match(rusty.free_email())
        assert re.fullmatch(r"\d{5}", rusty.postcode())
        assert re.fullmatch(r"\d{5}-\d{4}", rusty.zipcode_plus4())
        assert re.fullmatch(r"([0-9a-f]{2}:){5}[0-9a-f]{2}", rusty.mac_address())
        ipaddress.IPv4Address(rusty.ipv4())
        assert ipaddress.IPv4Address(rusty.ipv4_private()).is_private
        ipaddress.IPv4Network(rusty.ipv4(network=True))
        ipaddress.IPv6Network(rusty.ipv6(network=True))
        assert "{{" not in rusty.address() and "\n" in rusty.address()
        assert rusty.sentence().endswith(".")
        assert len(rusty.text(max_nb_chars=160)) <= 160
        assert rusty.url().startswith(("http://", "https://"))
        assert 10 == sum(c.isdigit() for c in rusty.basic_phone_number())
        assert rusty.state_abbr(include_territories=False, include_freely_associated_states=False).isalpha()
        assert 0 <= rusty.port_number(is_dynamic=True) - 49152 <= 16383


def test_date_time_values(rusty):
    now = datetime.now()
    for _ in range(N):
        dt = rusty.date_time()
        assert isinstance(dt, datetime) and dt.tzinfo is None and datetime(1970, 1, 1) <= dt <= now + timedelta(days=1)
        assert isinstance(rusty.date_object(), date)
        assert isinstance(rusty.time_object(), time)
        dob = rusty.date_of_birth(minimum_age=18, maximum_age=30)
        age = (date.today() - dob).days / 365.25
        assert 17.9 <= age <= 31.1
        assert rusty.date_this_year().year == date.today().year
        start, end = date(2000, 1, 1), date(2000, 12, 31)
        assert start <= rusty.date_between(start_date=start, end_date=end) <= end
        assert rusty.past_date() < date.today() < rusty.future_date()
        between = rusty.date_time_between(start_date="-1y", end_date=timedelta(days=-1))
        assert now - timedelta(days=367) <= between <= now
        assert rusty.date_time_between_dates(datetime_start=0, datetime_end=10) <= datetime(1970, 1, 1, 0, 0, 10)
    assert re.fullmatch(r"\d{4}-\d{2}-\d{2}", rusty.date())
    assert re.fullmatch(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}", rusty.iso8601(sep=" ", timespec="minutes"))
    assert rusty.month_name() in {"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"}
    assert isinstance(rusty.time_delta(end_datetime="+30d"), timedelta)
    assert isinstance(rusty.unix_time(), float)


def test_timezone_aware_fallbacks(rusty):
    paris = ZoneInfo("Europe/Paris")
    assert rusty.date_time(tzinfo=timezone.utc).tzinfo is timezone.utc
    assert rusty.date_time_between(start_date="-3d", tzinfo=paris).tzinfo is paris
    assert rusty.date_time_this_month(tzinfo=paris).tzinfo is paris
    assert rusty.future_datetime(tzinfo=paris) > datetime.now(paris)
    assert rusty.iso8601(tzinfo=timezone.utc).endswith("+00:00")
    assert isinstance(rusty.date_of_birth(tzinfo=paris), date)
    assert isinstance(rusty.pytimezone(), ZoneInfo)
    series = list(rusty.time_series(start_date="-1d", end_date="now", precision=3600))
    assert len(series) in (24, 25) and isinstance(series[0][0], datetime)


def test_errors_match_faker(rusty):
    with pytest.raises(ValueError):
        rusty.domain_name(0)
    with pytest.raises(ValueError):
        rusty.country_code(representation="alpha-9")
    with pytest.raises(Exception, match="State Abbreviation not found"):
        rusty.postcode_in_state("XX")
    with pytest.raises(ParseError):
        rusty.date_time_between(start_date="whenever")
    assert issubclass(ParseError, ValueError)
    with pytest.raises(ValueError):
        rusty.text(max_nb_chars=4)
    with pytest.raises(ValueError):
        rusty.random_int(5, 1)
    with pytest.raises(ValueError):
        rusty.nic_handle(suffix="X")
    with pytest.raises(ValueError, match="OrderedDict"):
        rusty.random_element({"a": 1})
    with pytest.raises(AttributeError):
        rusty.not_a_formatter()
    with pytest.raises(ValueError):
        rusty.date_of_birth(minimum_age=10, maximum_age=5)


def test_random_elements(rusty):
    weighted = OrderedDict([("common", 0.99), ("rare", 0.01)])
    picks = [rusty.random_element(weighted) for _ in range(2000)]
    assert picks.count("common") > 1900
    assert sorted(rusty.random_sample("abcde", length=5)) == list("abcde")
    assert len(rusty.random_choices(["x", "y"], length=10)) == 10
    assert set(rusty.random_elements({1, 2, 3}, length=5)) <= {1, 2, 3}
    assert rusty.random_element(range(5)) in range(5)
    assert set(rusty.words(nb=3, ext_word_list=["a", "b", "c"], unique=True)) == {"a", "b", "c"}
    assert rusty.lexify("??", letters="Z") == "ZZ"


def test_global_and_instance_seeding():
    Faker.seed(4321)
    first = [Faker().name() for _ in range(5)]
    Faker.seed(4321)
    second = [Faker().name() for _ in range(5)]
    assert first == second

    a, b = Faker(), Faker()
    a.seed_instance("same")
    b.seed_instance("same")
    assert [a.address() for _ in range(5)] == [b.address() for _ in range(5)]
    b.seed_instance(b"other")
    assert [a.address() for _ in range(5)] != [b.address() for _ in range(5)]

    with pytest.raises(TypeError):
        a.seed(1)


def test_seed_instance_invalidates_cached_methods():
    warmed = Faker()
    warmed.name()  # caches the bound method of the shared generator
    warmed.seed_instance(99)
    fresh = Faker()
    fresh.seed_instance(99)
    assert [warmed.name() for _ in range(5)] == [fresh.name() for _ in range(5)]


def test_custom_providers_override_and_share_seed():
    class Provider(BaseProvider):
        def name(self):
            return "overridden"

        def code(self):
            return self.bothify("??-##") + self.random_element(["x", "y"])

    fake = Faker()
    fake.name()
    fake.add_provider(Provider)
    assert fake.name() == "overridden"
    fake.seed_instance(7)
    first = [fake.code() for _ in range(5)]
    fake.seed_instance(7)
    assert [fake.code() for _ in range(5)] == first

    fake.add_provider(DynamicProvider(provider_name="profession", elements=["dr", "nurse"]))
    assert fake.profession() in {"dr", "nurse"}


def test_parse_format_and_argument_groups(rusty):
    rusty.set_arguments("small", {"min": 1, "max": 3})
    assert rusty.parse("{{ random_int:small }}") in {"1", "2", "3"}
    assert "{{" not in rusty.parse("{{first_name}} {{ last_name }}")
    assert rusty.format("random_int", 5, 5) == 5
    with pytest.raises(AttributeError):
        rusty.format("nope")


def test_unique_and_optional(rusty):
    values = {rusty.unique.random_element("ab") for _ in range(2)}
    assert values == {"a", "b"}
    with pytest.raises(UniquenessException):
        rusty.unique.random_element("ab")
    rusty.unique.clear()
    rusty.unique.random_element("ab")
    assert rusty.optional.name(prob=1.0) is not None
    with pytest.raises(ValueError):
        rusty.optional.name(prob=0)
    results = [rusty.optional.first_name(prob=0.5) for _ in range(400)]
    assert 100 < sum(r is None for r in results) < 300


def test_multiple_locales():
    fake = Faker(["en_US", "de-DE"])
    assert fake.locales == ["en_US", "de_DE"]
    assert fake.name()
    assert fake["de_DE"].city()
    with pytest.raises(NotImplementedError):
        fake.parse("{{name}}")
    weighted = Faker(OrderedDict([("en_US", 1), ("fr_FR", 2)]))
    assert weighted.weights == [1, 2] and weighted.email()
    with pytest.raises(AttributeError):
        Faker("not a locale!")


def test_copy_and_pickle(rusty):
    clone = copy.deepcopy(rusty)
    assert clone.name() and clone is not rusty
    restored = pickle.loads(pickle.dumps(rusty))
    assert restored.address()


def test_pytest_plugin(pytester):
    pytester.makepyfile(
        """
        import pytest

        @pytest.fixture
        def faker_seed():
            return 12

        def test_fixture(faker):
            assert type(faker).__module__ == "rusty_faker.proxy"
            faker.seed_instance(12)
            first = faker.name()
            faker.seed_instance(12)
            assert faker.name() == first
        """
    )
    result = pytester.runpytest_subprocess("-p", "no:faker")
    result.assert_outcomes(passed=1)
