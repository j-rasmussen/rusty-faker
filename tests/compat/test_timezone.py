"""Timezone-aware date/time parity with Faker, and with the pure-Python reference path.

The Rust fast path builds aware datetimes directly; ``rusty_faker._tz`` keeps Faker's
original Python implementation, so deterministic cases must agree with both.
"""

from datetime import date, datetime, timedelta, timezone, tzinfo
from zoneinfo import ZoneInfo

import faker
import pytest

from rusty_faker import Faker, _tz

PARIS = ZoneInfo("Europe/Paris")
KATHMANDU = ZoneInfo("Asia/Kathmandu")  # UTC+05:45
CHATHAM = ZoneInfo("Pacific/Chatham")  # UTC+12:45 / +13:45 with DST


class FixedOffset(tzinfo):
    """A hand-written tzinfo, to check we accept more than zoneinfo and datetime.timezone."""

    def __init__(self, hours: float, name: str = "FIXED"):
        self._offset = timedelta(hours=hours)
        self._name = name

    def utcoffset(self, dt):
        return self._offset

    def dst(self, dt):
        return timedelta(0)

    def tzname(self, dt):
        return self._name


ALL_TZ = [timezone.utc, PARIS, KATHMANDU, CHATHAM, timezone(timedelta(hours=-3, minutes=-30)), FixedOffset(2)]


@pytest.fixture
def real():
    fake = faker.Faker("en_US")
    fake.seed_instance(99)
    return fake


@pytest.fixture
def rusty():
    fake = Faker("en_US")
    fake.seed_instance(99)
    return fake


@pytest.mark.parametrize("tz", ALL_TZ)
def test_date_time_matches_faker_exactly_when_range_is_a_point(real, rusty, tz):
    # end_datetime=0 pins the range to [epoch, epoch], so both libraries are deterministic.
    assert rusty.date_time(tzinfo=tz, end_datetime=0) == real.date_time(tzinfo=tz, end_datetime=0)
    assert rusty.date_time(tzinfo=tz, end_datetime=0).tzinfo is tz


@pytest.mark.parametrize("tz", ALL_TZ)
def test_between_dates_matches_faker_exactly_when_range_is_a_point(real, rusty, tz):
    for moment in (datetime(1970, 1, 1), datetime(2001, 6, 30, 12), datetime(2024, 3, 31, 1, 30)):
        expected = real.date_time_between_dates(datetime_start=moment, datetime_end=moment, tzinfo=tz)
        actual = rusty.date_time_between_dates(datetime_start=moment, datetime_end=moment, tzinfo=tz)
        assert actual == expected, f"{tz} at {moment}"
        assert actual.utcoffset() == expected.utcoffset()


@pytest.mark.parametrize("tz", ALL_TZ)
def test_fast_path_matches_python_reference(rusty, tz):
    core = rusty._factories[0]._core
    moment = datetime(2015, 7, 4, 9, 30, 15)
    assert rusty.date_time(tzinfo=tz, end_datetime=0) == _tz.date_time(core, tz, 0)
    assert rusty.date_time_between_dates(datetime_start=moment, datetime_end=moment, tzinfo=tz) == _tz.date_time_between_dates(
        core, moment, moment, tz
    )
    assert rusty.date_time_ad(tzinfo=tz, start_datetime=0, end_datetime=0) == _tz.date_time_ad(core, tz, 0, 0)
    assert rusty.iso8601(tzinfo=tz, end_datetime=0) == _tz.iso8601(core, tz, 0)
    assert rusty.iso8601(tzinfo=tz, end_datetime=0, sep=" ", timespec="seconds") == _tz.iso8601(core, tz, 0, " ", "seconds")


@pytest.mark.parametrize("tz", ALL_TZ)
def test_aware_results_are_random_and_in_range(rusty, tz):
    now = datetime.now(tz)
    values = set()
    for _ in range(200):
        dt = rusty.date_time(tzinfo=tz)
        values.add(dt)
        assert dt.tzinfo is tz
        assert datetime(1970, 1, 1, tzinfo=tz) <= dt <= now + timedelta(days=1)
        assert rusty.date_time_between(start_date="-5d", end_date="now", tzinfo=tz) <= datetime.now(tz)
        assert rusty.past_datetime(tzinfo=tz) < datetime.now(tz) < rusty.future_datetime(tzinfo=tz)
    assert len(values) > 190, "timestamps should still be random"
    assert {dt.microsecond for dt in values} != {0}, "sub-second precision should be kept"


@pytest.mark.parametrize("tz", ALL_TZ)
def test_this_period_and_date_of_birth(rusty, tz):
    now = datetime.now(tz)
    for _ in range(50):
        assert rusty.date_time_this_year(tzinfo=tz).year == now.year
        assert rusty.date_time_this_month(tzinfo=tz).month == now.month
        assert rusty.date_time_this_decade(tzinfo=tz).year // 10 == now.year // 10
        assert rusty.date_time_this_century(tzinfo=tz).year // 100 == now.year // 100
        after = rusty.date_time_this_year(before_now=False, after_now=True, tzinfo=tz)
        assert after >= now - timedelta(seconds=1)
        assert rusty.date_time_this_year(before_now=False, after_now=False, tzinfo=tz).tzinfo is tz
        dob = rusty.date_of_birth(tzinfo=tz, minimum_age=18, maximum_age=30)
        assert isinstance(dob, date) and not isinstance(dob, datetime)
        assert 17.9 <= (datetime.now(tz).date() - dob).days / 365.25 <= 31.1


def test_seeded_aware_calls_are_repeatable():
    a, b = Faker(), Faker()
    a.seed_instance(2024)
    b.seed_instance(2024)
    assert [a.date_time(tzinfo=PARIS) for _ in range(5)] == [b.date_time(tzinfo=PARIS) for _ in range(5)]
    assert [a.date_time_between(tzinfo=PARIS) for _ in range(5)] == [b.date_time_between(tzinfo=PARIS) for _ in range(5)]


def test_naive_calls_are_unaffected(rusty):
    assert rusty.date_time(tzinfo=None).tzinfo is None
    assert rusty.date_time().tzinfo is None
    assert rusty.date_time_between(start_date="-1d", end_date="now").tzinfo is None


def test_invalid_tzinfo_raises(rusty):
    with pytest.raises(TypeError):
        rusty.date_time(tzinfo="Europe/Paris")
