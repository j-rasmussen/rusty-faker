"""Timezone-aware date/time formatters, adapted from ``faker.providers.date_time``
(MIT, Copyright (c) 2012 Daniele Faraglia).

The Rust core handles naive datetimes. When a ``tzinfo`` is passed, the binding calls the
function of the same name here; random draws still come from the Rust generator.
"""

from __future__ import annotations

import re
import zoneinfo
from calendar import timegm
from datetime import MAXYEAR
from datetime import date as dtdate
from datetime import datetime, timedelta
from datetime import timezone as dttimezone
from typing import Any, Callable, Dict, Iterator, Optional, Tuple, Union

from rusty_faker.exceptions import ParseError

_timedelta_pattern = ""
for _name, _sym in [
    ("years", "y"),
    ("months", "M"),
    ("weeks", "w"),
    ("days", "d"),
    ("hours", "h"),
    ("minutes", "m"),
    ("seconds", "s"),
]:
    _timedelta_pattern += rf"((?P<{_name}>(?:\+|-)\d+?){_sym})?"
_regex = re.compile(_timedelta_pattern)


def _get_local_timezone():
    return datetime.now().astimezone().tzinfo


def _get_next_month_start(dt):
    if dt.month == 12:
        return dt.replace(year=dt.year + 1, month=1)
    return dt.replace(month=dt.month + 1)


def datetime_to_timestamp(dt: Union[dtdate, datetime]) -> int:
    if isinstance(dt, datetime) and getattr(dt, "tzinfo", None) is not None:
        dt = dt.astimezone(dttimezone.utc)
    return timegm(dt.timetuple())


def timestamp_to_datetime(timestamp: Union[int, float], tzinfo) -> datetime:
    if tzinfo is None:
        pick = convert_timestamp_to_datetime(timestamp, _get_local_timezone())
        return pick.astimezone(dttimezone.utc).replace(tzinfo=None)
    return convert_timestamp_to_datetime(timestamp, tzinfo)


def convert_timestamp_to_datetime(timestamp: Union[int, float], tzinfo) -> datetime:
    if timestamp >= 0:
        return datetime.fromtimestamp(timestamp, tzinfo)
    return datetime(1970, 1, 1, tzinfo=tzinfo) + timedelta(seconds=int(timestamp))


def change_year(current_date: dtdate, year_diff: int) -> dtdate:
    year = current_date.year + year_diff
    try:
        return current_date.replace(year=year)
    except ValueError as e:
        if year != 0 and current_date.month == 2 and current_date.day == 29:
            return current_date.replace(month=3, day=1, year=year)
        raise e


def _rand_seconds(gen: Any, start: float, end: float) -> float:
    if start > end:
        raise ValueError("empty range for _rand_seconds: start datetime must be before than end datetime")
    return gen._uniform(start, end)


def _parse_date_string(value: str) -> Dict[str, float]:
    parts = _regex.match(value)
    if not parts:
        raise ParseError(f"Can't parse date string `{value}`")
    time_params: Dict[str, float] = {}
    for name_, param_ in parts.groupdict().items():
        if param_:
            time_params[name_] = int(param_)

    if "years" in time_params:
        time_params["days"] = time_params.get("days", 0) + 365.24 * time_params.pop("years")
    if "months" in time_params:
        time_params["days"] = time_params.get("days", 0) + 30.42 * time_params.pop("months")

    if not time_params:
        raise ParseError(f"Can't parse date string `{value}`")
    return time_params


def _parse_timedelta(value: Union[timedelta, str, float]) -> Union[float, int]:
    if isinstance(value, timedelta):
        return value.total_seconds()
    if isinstance(value, str):
        return timedelta(**_parse_date_string(value)).total_seconds()  # type: ignore
    if isinstance(value, (int, float)):
        return value
    raise ParseError(f"Invalid format for timedelta {value!r}")


def _parse_date_time(value: Any, tzinfo=None) -> int:
    if isinstance(value, (datetime, dtdate)):
        return datetime_to_timestamp(value)
    now = datetime.now(tzinfo)
    if isinstance(value, timedelta):
        return datetime_to_timestamp(now + value)
    if isinstance(value, str):
        if value == "now":
            return datetime_to_timestamp(datetime.now(tzinfo))
        return datetime_to_timestamp(now + timedelta(**_parse_date_string(value)))  # type: ignore
    if isinstance(value, int):
        return value
    raise ParseError(f"Invalid format for date {value!r}")


def _parse_start_datetime(value: Any) -> int:
    return 0 if value is None else _parse_date_time(value)


def _parse_end_datetime(value: Any) -> int:
    return datetime_to_timestamp(datetime.now()) if value is None else _parse_date_time(value)


def unix_time(gen: Any, end_datetime: Any = None, start_datetime: Any = None) -> float:
    start = _parse_start_datetime(start_datetime)
    end = _parse_end_datetime(end_datetime)
    return float(_rand_seconds(gen, start, end))


def date_time(gen: Any, tzinfo=None, end_datetime: Any = None) -> datetime:
    return datetime(1970, 1, 1, tzinfo=tzinfo) + timedelta(seconds=unix_time(gen, end_datetime=end_datetime))


def date_time_ad(gen: Any, tzinfo=None, end_datetime: Any = None, start_datetime: Any = None) -> datetime:
    start_time = -62135596800 if start_datetime is None else _parse_start_datetime(start_datetime)
    end = _parse_end_datetime(end_datetime)
    ts = _rand_seconds(gen, start_time, end)
    return datetime(1970, 1, 1, tzinfo=tzinfo) + timedelta(seconds=ts)


def iso8601(gen: Any, tzinfo=None, end_datetime: Any = None, sep: str = "T", timespec: str = "auto") -> str:
    return date_time(gen, tzinfo, end_datetime=end_datetime).isoformat(sep, timespec)


def date_time_between(gen: Any, start_date: Any = None, end_date: Any = None, tzinfo=None) -> datetime:
    start = _parse_date_time("-30y" if start_date is None else start_date, tzinfo=tzinfo)
    end = _parse_date_time("now" if end_date is None else end_date, tzinfo=tzinfo)
    if end - start <= 1:
        ts = start + gen._random()
    else:
        ts = _rand_seconds(gen, start, end)
    if tzinfo is None:
        return datetime(1970, 1, 1, tzinfo=tzinfo) + timedelta(seconds=ts)
    return (datetime(1970, 1, 1, tzinfo=dttimezone.utc) + timedelta(seconds=ts)).astimezone(tzinfo)


def future_datetime(gen: Any, end_date: Any = None, tzinfo=None) -> datetime:
    return date_time_between(gen, "+1s", "+30d" if end_date is None else end_date, tzinfo)


def past_datetime(gen: Any, start_date: Any = None, tzinfo=None) -> datetime:
    return date_time_between(gen, "-30d" if start_date is None else start_date, "-1s", tzinfo)


def date_time_between_dates(gen: Any, datetime_start: Any = None, datetime_end: Any = None, tzinfo=None) -> datetime:
    start = datetime_to_timestamp(datetime.now(tzinfo)) if datetime_start is None else _parse_date_time(datetime_start)
    end = datetime_to_timestamp(datetime.now(tzinfo)) if datetime_end is None else _parse_date_time(datetime_end)
    timestamp = _rand_seconds(gen, start, end)
    try:
        if tzinfo is None:
            pick = convert_timestamp_to_datetime(timestamp, _get_local_timezone())
            try:
                pick = pick.astimezone(dttimezone.utc).replace(tzinfo=None)
            except OSError:
                pass
        else:
            pick = datetime.fromtimestamp(timestamp, tzinfo)
    except OverflowError:
        raise OverflowError(
            "You specified an end date with a timestamp bigger than the maximum allowed on this"
            " system. Please specify an earlier date.",
        )
    return pick


def _this_period(gen: Any, start: datetime, end: datetime, now: datetime, before_now: bool, after_now: bool, tzinfo) -> datetime:
    if before_now and after_now:
        return date_time_between_dates(gen, start, end, tzinfo)
    if not before_now and after_now:
        return date_time_between_dates(gen, now, end, tzinfo)
    if not after_now and before_now:
        return date_time_between_dates(gen, start, now, tzinfo)
    return now


def date_time_this_century(gen: Any, before_now: bool = True, after_now: bool = False, tzinfo=None) -> datetime:
    now = datetime.now(tzinfo)
    start = datetime(now.year - (now.year % 100), 1, 1, tzinfo=tzinfo)
    end = datetime(min(start.year + 100, MAXYEAR), 1, 1, tzinfo=tzinfo)
    return _this_period(gen, start, end, now, before_now, after_now, tzinfo)


def date_time_this_decade(gen: Any, before_now: bool = True, after_now: bool = False, tzinfo=None) -> datetime:
    now = datetime.now(tzinfo)
    start = datetime(now.year - (now.year % 10), 1, 1, tzinfo=tzinfo)
    end = datetime(min(start.year + 10, MAXYEAR), 1, 1, tzinfo=tzinfo)
    return _this_period(gen, start, end, now, before_now, after_now, tzinfo)


def date_time_this_year(gen: Any, before_now: bool = True, after_now: bool = False, tzinfo=None) -> datetime:
    now = datetime.now(tzinfo)
    start = now.replace(month=1, day=1, hour=0, minute=0, second=0, microsecond=0)
    end = datetime(now.year + 1, 1, 1, tzinfo=tzinfo)
    return _this_period(gen, start, end, now, before_now, after_now, tzinfo)


def date_time_this_month(gen: Any, before_now: bool = True, after_now: bool = False, tzinfo=None) -> datetime:
    now = datetime.now(tzinfo)
    start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)
    end = _get_next_month_start(start)
    return _this_period(gen, start, end, now, before_now, after_now, tzinfo)


def date_of_birth(gen: Any, tzinfo=None, minimum_age: int = 0, maximum_age: int = 115) -> dtdate:
    if not isinstance(minimum_age, int):
        raise TypeError("minimum_age must be an integer.")
    if not isinstance(maximum_age, int):
        raise TypeError("maximum_age must be an integer.")
    if maximum_age < 0:
        raise ValueError("maximum_age must be greater than or equal to zero.")
    if minimum_age < 0:
        raise ValueError("minimum_age must be greater than or equal to zero.")
    if minimum_age > maximum_age:
        raise ValueError("minimum_age must be less than or equal to maximum_age.")
    now = datetime.now(tzinfo).date()
    start_date = change_year(now, -(maximum_age + 1))
    end_date = change_year(now, -minimum_age)
    dob = date_time_ad(gen, tzinfo=tzinfo, start_datetime=start_date, end_datetime=end_date).date()
    return dob if dob != start_date else dob + timedelta(days=1)


def time_series(
    gen: Any,
    start_date: Any = "-30d",
    end_date: Any = "now",
    precision: Optional[float] = None,
    distrib: Optional[Callable[[datetime], float]] = None,
    tzinfo=None,
) -> Iterator[Tuple[datetime, Any]]:
    start_date_ = _parse_date_time(start_date, tzinfo=tzinfo)
    end_date_ = _parse_date_time(end_date, tzinfo=tzinfo)

    if end_date_ < start_date_:
        raise ValueError("`end_date` must be greater than `start_date`.")

    precision_ = _parse_timedelta((end_date_ - start_date_) / 30 if precision is None else precision)
    if distrib is None:

        def distrib(dt):
            return gen._uniform(0, precision_)  # noqa

    if not callable(distrib):
        raise ValueError(f"`distrib` must be a callable. Got {distrib} instead.")

    def points():
        datapoint: Union[float, int] = start_date_
        while datapoint < end_date_:
            dt = timestamp_to_datetime(datapoint, tzinfo)
            datapoint += precision_
            yield (dt, distrib(dt))

    return points()


def pytimezone(generator: Any, *args: Any, **kwargs: Any):
    try:
        return zoneinfo.ZoneInfo(generator.timezone(*args, **kwargs))  # type: ignore
    except zoneinfo.ZoneInfoNotFoundError as exc:
        msg = (
            f"Timezone data not found: {exc}. "
            "The 'tzdata' package provides timezone database files needed by Python's zoneinfo module. "
            "While most systems have these files built-in, some minimal environments may not. "
            "Install it with: pip install tzdata"
        )
        raise ImportError(msg) from exc
