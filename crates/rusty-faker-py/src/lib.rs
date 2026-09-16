//! PyO3 bindings: exposes `rusty_faker_core::Generator` as `rusty_faker._core.Generator`.
//!
//! Timezone-aware date/time calls are built here from a Rust-drawn timestamp; only
//! `time_series` and `pytimezone` live in the pure-Python `rusty_faker._tz` module.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use pyo3::exceptions::{
    PyAttributeError, PyException, PyIndexError, PyOverflowError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{
    PyDate, PyDateTime, PyDelta, PyDict, PyList, PySequence, PyString, PyTuple, PyTzInfo,
};
use rusty_faker_core::{Clock, DateTimeValue, Error, Generator, timestamp_to_naive};

/// Python's `datetime.MAXYEAR`.
const MAX_YEAR: i32 = 9999;

pyo3::create_exception!(
    rusty_faker._core,
    ParseError,
    PyValueError,
    "A date string could not be parsed."
);

const ASCII_LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn to_py_err(err: Error) -> PyErr {
    let message = err.to_string();
    match err {
        Error::UnknownLocale(_) | Error::InvalidArgument(_) => PyValueError::new_err(message),
        Error::UnknownFormatter(_) | Error::Unsupported(_) => PyAttributeError::new_err(message),
        Error::EmptySequence => PyIndexError::new_err(message),
        Error::Overflow(_) => PyOverflowError::new_err(message),
        Error::Parse(_) => ParseError::new_err(message),
        Error::StateNotFound => PyException::new_err(message),
    }
}

trait IntoPyResult<T> {
    fn py(self) -> PyResult<T>;
}

impl<T> IntoPyResult<T> for Result<T, Error> {
    fn py(self) -> PyResult<T> {
        self.map_err(to_py_err)
    }
}

/// Owned form of Faker's `DateParseType`.
enum ParsedDate {
    Timestamp(i64),
    DateTime(NaiveDateTime),
    Date(NaiveDate),
    Delta(f64),
    Str(String),
}

impl ParsedDate {
    fn value(&self) -> DateTimeValue<'_> {
        match self {
            ParsedDate::Timestamp(ts) => DateTimeValue::Timestamp(*ts),
            ParsedDate::DateTime(dt) => DateTimeValue::DateTime(*dt),
            ParsedDate::Date(date) => DateTimeValue::Date(*date),
            ParsedDate::Delta(seconds) => DateTimeValue::Delta(*seconds),
            ParsedDate::Str(s) => DateTimeValue::Str(s),
        }
    }

    fn from_py(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_instance_of::<PyDateTime>() {
            if !value.getattr("tzinfo")?.is_none() {
                let ts: f64 = value.call_method0("timestamp")?.extract()?;
                return Ok(ParsedDate::Timestamp(ts.floor() as i64));
            }
            return Ok(ParsedDate::DateTime(value.extract()?));
        }
        if value.is_instance_of::<PyDate>() {
            return Ok(ParsedDate::Date(value.extract()?));
        }
        if value.is_instance_of::<PyDelta>() {
            return Ok(ParsedDate::Delta(
                value.call_method0("total_seconds")?.extract()?,
            ));
        }
        if let Ok(s) = value.cast::<PyString>() {
            return Ok(ParsedDate::Str(s.to_str()?.to_owned()));
        }
        if let Ok(ts) = value.extract::<i64>()
            && !value.is_instance_of::<pyo3::types::PyFloat>()
        {
            return Ok(ParsedDate::Timestamp(ts));
        }
        Err(ParseError::new_err(format!(
            "Invalid format for date {}",
            value.repr()?
        )))
    }
}

fn parse_opt(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<ParsedDate>> {
    value
        .filter(|v| !v.is_none())
        .map(ParsedDate::from_py)
        .transpose()
}

fn parse_or(value: Option<&Bound<'_, PyAny>>, default: &str) -> PyResult<ParsedDate> {
    Ok(parse_opt(value)?.unwrap_or_else(|| ParsedDate::Str(default.to_owned())))
}

/// The `tzinfo` argument as a `datetime.tzinfo`, or `None` when absent.
fn tzinfo_of<'py>(tzinfo: Option<&Bound<'py, PyAny>>) -> PyResult<Option<Bound<'py, PyTzInfo>>> {
    let Some(tz) = tzinfo.filter(|tz| !tz.is_none()) else {
        return Ok(None);
    };
    let tz = tz
        .cast::<PyTzInfo>()
        .map_err(|_| PyTypeError::new_err("tzinfo must be a datetime.tzinfo instance"))?;
    Ok(Some(tz.clone()))
}

/// `datetime(...wall clock..., tzinfo=tz)`: attaches the zone without shifting the value,
/// matching Faker's `datetime(1970, 1, 1, tzinfo=tz) + timedelta(seconds=ts)`.
fn attach_tz<'py>(
    py: Python<'py>,
    dt: NaiveDateTime,
    tz: &Bound<'py, PyTzInfo>,
) -> PyResult<Bound<'py, PyDateTime>> {
    PyDateTime::new(
        py,
        dt.year(),
        dt.month() as u8,
        dt.day() as u8,
        dt.hour() as u8,
        dt.minute() as u8,
        dt.second() as u8,
        dt.nanosecond() / 1000,
        Some(tz),
    )
}

/// `datetime.fromtimestamp(ts, tz)`: the instant `ts`, expressed in `tz`.
fn at_timestamp<'py>(
    py: Python<'py>,
    ts: f64,
    tz: &Bound<'py, PyTzInfo>,
) -> PyResult<Bound<'py, PyDateTime>> {
    PyDateTime::from_timestamp(py, ts, Some(tz))
}

fn datetime_class(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    static DATETIME: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    DATETIME
        .get_or_try_init(py, || {
            py.import("datetime")?
                .getattr("datetime")
                .map(Bound::unbind)
        })
        .map(|t| t.bind(py))
}

/// `datetime.now(tz)`.
fn now_in_tz<'py>(py: Python<'py>, tz: &Bound<'py, PyTzInfo>) -> PyResult<Bound<'py, PyDateTime>> {
    Ok(datetime_class(py)?
        .call_method1("now", (tz,))?
        .cast_into::<PyDateTime>()?)
}

/// The calendar date of an aware datetime, read through attribute access: the `PyDateAccess`
/// accessors read the struct fields directly and are unavailable under the limited API.
fn ymd_of(dt: &Bound<'_, PyDateTime>) -> PyResult<(i32, u8, u8)> {
    Ok((
        dt.getattr("year")?.extract()?,
        dt.getattr("month")?.extract()?,
        dt.getattr("day")?.extract()?,
    ))
}

/// `timegm(dt.astimezone(utc).timetuple())` for an aware datetime.
fn floor_timestamp_of(dt: &Bound<'_, PyDateTime>) -> PyResult<i64> {
    let ts: f64 = dt.call_method0("timestamp")?.extract()?;
    Ok(ts.floor() as i64)
}

fn digit_or_empty(py: Python<'_>, digit: Option<u8>) -> PyResult<Bound<'_, PyAny>> {
    match digit {
        Some(d) => Ok(d.into_pyobject(py)?.into_any()),
        None => Ok(PyString::intern(py, "").into_any()),
    }
}

fn ordered_dict_type(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    static ORDERED_DICT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    ORDERED_DICT
        .get_or_try_init(py, || {
            py.import("collections")?
                .getattr("OrderedDict")
                .map(Bound::unbind)
        })
        .map(|t| t.bind(py))
}

fn to_str_vec(words: &Option<Vec<String>>) -> Option<Vec<&str>> {
    words
        .as_ref()
        .map(|w| w.iter().map(String::as_str).collect())
}

#[pyclass(module = "rusty_faker._core", name = "Generator")]
struct PyGenerator {
    inner: Generator,
}

#[pymethods]
impl PyGenerator {
    #[new]
    #[pyo3(signature = (locale = "en_US", seed = None))]
    fn new(locale: &str, seed: Option<u64>) -> PyResult<Self> {
        let mut inner = Generator::new(locale).py()?;
        if let Some(seed) = seed {
            inner.seed(seed);
        }
        Ok(Self { inner })
    }

    /// Reseeds the generator; `None` reseeds from OS entropy.
    #[pyo3(signature = (seed = None))]
    fn _seed(&mut self, seed: Option<u64>) {
        match seed {
            Some(seed) => self.inner.seed(seed),
            None => self.inner.reseed_from_entropy(),
        }
    }

    #[getter]
    fn _locale(&self) -> &'static str {
        self.inner.locale_data().code
    }

    #[getter]
    fn _use_weighting(&self) -> bool {
        self.inner.use_weighting()
    }

    #[setter(_use_weighting)]
    fn set_use_weighting(&mut self, value: bool) {
        self.inner.set_use_weighting(value);
    }

    fn _uniform(&mut self, a: f64, b: f64) -> f64 {
        self.inner.uniform(a, b)
    }

    fn _random(&mut self) -> f64 {
        self.inner.unit()
    }

    fn __copy__(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    fn __deepcopy__(&self, _memo: Bound<'_, PyAny>) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Pickles as a fresh generator for the same locale (RNG state is not preserved).
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (&'static str,))> {
        let locale = slf.borrow().inner.locale_data().code;
        Ok((slf.get_type().into_any(), (locale,)))
    }

    /// `True` with probability `percent`/100.
    fn _chance(&mut self, percent: i64) -> bool {
        self.inner.random_int(1, 100, 1).is_ok_and(|n| n <= percent)
    }

    /// The Rust-side formatter names, for `dir()` and attribute resolution.
    #[staticmethod]
    fn _formatter_names() -> Vec<&'static str> {
        FORMATTER_NAMES.to_vec()
    }

    // ---- base ----

    #[pyo3(signature = (min = 0, max = 9999, step = 1))]
    fn random_int(&mut self, min: i64, max: i64, step: i64) -> PyResult<i64> {
        self.inner.random_int(min, max, step).py()
    }

    fn random_digit(&mut self) -> u8 {
        self.inner.random_digit()
    }

    fn random_digit_not_null(&mut self) -> u8 {
        self.inner.random_digit_not_null()
    }

    fn random_digit_above_two(&mut self) -> u8 {
        self.inner.random_digit_above_two()
    }

    fn random_digit_or_empty<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        digit_or_empty(py, self.inner.random_digit_or_empty())
    }

    fn random_digit_not_null_or_empty<'py>(
        &mut self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        digit_or_empty(py, self.inner.random_digit_not_null_or_empty())
    }

    #[pyo3(signature = (digits = None, fix_len = false))]
    fn random_number(&mut self, digits: Option<i64>, fix_len: bool) -> PyResult<u128> {
        self.inner.random_number(digits, fix_len).py()
    }

    fn random_letter(&mut self) -> char {
        self.inner.random_letter()
    }

    #[pyo3(signature = (length = 16))]
    fn random_letters(&mut self, length: usize) -> Vec<char> {
        self.inner.random_letters(length)
    }

    fn random_lowercase_letter(&mut self) -> char {
        self.inner.random_lowercase_letter()
    }

    fn random_uppercase_letter(&mut self) -> char {
        self.inner.random_uppercase_letter()
    }

    #[pyo3(signature = (elements = None, length = None, unique = false, use_weighting = None))]
    fn random_elements<'py>(
        &mut self,
        py: Python<'py>,
        elements: Option<Bound<'py, PyAny>>,
        length: Option<usize>,
        unique: bool,
        use_weighting: Option<bool>,
    ) -> PyResult<Bound<'py, PyList>> {
        let elements = match elements {
            Some(elements) => elements,
            None => PyTuple::new(py, ["a", "b", "c"])?.into_any(),
        };
        let use_weighting = use_weighting.unwrap_or_else(|| self.inner.use_weighting());
        if let Ok(dict) = elements.cast::<PyDict>() {
            if !dict.is_instance(ordered_dict_type(py)?)? {
                return Err(PyValueError::new_err(
                    "Use OrderedDict only to avoid dependency on PYTHONHASHSEED (See #363).",
                ));
            }
            let mut keys = Vec::with_capacity(dict.len());
            let mut weights = Vec::with_capacity(dict.len());
            for (key, value) in dict.iter() {
                keys.push(key);
                if use_weighting {
                    weights.push(value.extract::<f64>()?);
                }
            }
            let weights = use_weighting.then_some(weights.as_slice());
            let indices = self
                .inner
                .random_element_indices(keys.len(), weights, length, unique)
                .py()?;
            return PyList::new(py, indices.into_iter().map(|i| &keys[i]));
        }
        if length == Some(1)
            && !unique
            && let Ok(seq) = elements.cast::<PySequence>()
        {
            let len = seq.len()?;
            let index = self
                .inner
                .random_element_indices(len, None, Some(1), false)
                .py()?;
            return PyList::new(
                py,
                index
                    .into_iter()
                    .map(|i| seq.get_item(i))
                    .collect::<PyResult<Vec<_>>>()?,
            );
        }
        let items: Vec<Bound<'py, PyAny>> = elements.try_iter()?.collect::<PyResult<_>>()?;
        let indices = self
            .inner
            .random_element_indices(items.len(), None, length, unique)
            .py()?;
        PyList::new(py, indices.into_iter().map(|i| &items[i]))
    }

    #[pyo3(signature = (elements = None, length = None))]
    fn random_choices<'py>(
        &mut self,
        py: Python<'py>,
        elements: Option<Bound<'py, PyAny>>,
        length: Option<usize>,
    ) -> PyResult<Bound<'py, PyList>> {
        self.random_elements(py, elements, length, false, None)
    }

    #[pyo3(signature = (elements = None))]
    fn random_element<'py>(
        &mut self,
        py: Python<'py>,
        elements: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.random_elements(py, elements, Some(1), false, None)?
            .get_item(0)
    }

    #[pyo3(signature = (elements = None, length = None))]
    fn random_sample<'py>(
        &mut self,
        py: Python<'py>,
        elements: Option<Bound<'py, PyAny>>,
        length: Option<usize>,
    ) -> PyResult<Bound<'py, PyList>> {
        self.random_elements(py, elements, length, true, None)
    }

    #[pyo3(signature = (number = 10, le = false, ge = false, min = None, max = None))]
    fn randomize_nb_elements(
        &mut self,
        number: i64,
        le: bool,
        ge: bool,
        min: Option<i64>,
        max: Option<i64>,
    ) -> i64 {
        self.inner.randomize_nb_elements(number, le, ge, min, max)
    }

    #[pyo3(signature = (text = "###"))]
    fn numerify(&mut self, text: &str) -> String {
        self.inner.numerify(text)
    }

    #[pyo3(signature = (text = "????", letters = ASCII_LETTERS))]
    fn lexify(&mut self, text: &str, letters: &str) -> PyResult<String> {
        self.inner
            .lexify(text, (letters != ASCII_LETTERS).then_some(letters))
            .py()
    }

    #[pyo3(signature = (text = "## ??", letters = ASCII_LETTERS))]
    fn bothify(&mut self, text: &str, letters: &str) -> PyResult<String> {
        self.inner
            .bothify(text, (letters != ASCII_LETTERS).then_some(letters))
            .py()
    }

    #[pyo3(signature = (text = "^^^^", upper = false))]
    fn hexify(&mut self, text: &str, upper: bool) -> String {
        self.inner.hexify(text, upper)
    }

    fn locale(&mut self) -> String {
        self.inner.locale()
    }

    fn language_code(&mut self) -> &'static str {
        self.inner.language_code()
    }

    // ---- person ----

    fn name(&mut self) -> String {
        self.inner.name()
    }

    fn name_female(&mut self) -> String {
        self.inner.name_female()
    }

    fn name_male(&mut self) -> String {
        self.inner.name_male()
    }

    fn name_nonbinary(&mut self) -> String {
        self.inner.name_nonbinary()
    }

    fn first_name(&mut self) -> &'static str {
        self.inner.first_name()
    }

    fn first_name_female(&mut self) -> &'static str {
        self.inner.first_name_female()
    }

    fn first_name_male(&mut self) -> &'static str {
        self.inner.first_name_male()
    }

    fn first_name_nonbinary(&mut self) -> &'static str {
        self.inner.first_name_nonbinary()
    }

    fn last_name(&mut self) -> &'static str {
        self.inner.last_name()
    }

    fn last_name_female(&mut self) -> &'static str {
        self.inner.last_name_female()
    }

    fn last_name_male(&mut self) -> &'static str {
        self.inner.last_name_male()
    }

    fn last_name_nonbinary(&mut self) -> &'static str {
        self.inner.last_name_nonbinary()
    }

    fn prefix(&mut self) -> &'static str {
        self.inner.prefix()
    }

    fn prefix_female(&mut self) -> &'static str {
        self.inner.prefix_female()
    }

    fn prefix_male(&mut self) -> &'static str {
        self.inner.prefix_male()
    }

    fn prefix_nonbinary(&mut self) -> &'static str {
        self.inner.prefix_nonbinary()
    }

    fn suffix(&mut self) -> &'static str {
        self.inner.suffix()
    }

    fn suffix_female(&mut self) -> &'static str {
        self.inner.suffix_female()
    }

    fn suffix_male(&mut self) -> &'static str {
        self.inner.suffix_male()
    }

    fn suffix_nonbinary(&mut self) -> &'static str {
        self.inner.suffix_nonbinary()
    }

    fn language_name(&mut self) -> &'static str {
        self.inner.language_name()
    }

    // ---- address ----

    fn address(&mut self) -> String {
        self.inner.address()
    }

    fn administrative_unit(&mut self) -> PyResult<&'static str> {
        self.inner.administrative_unit().py()
    }

    fn state(&mut self) -> PyResult<&'static str> {
        self.inner.state().py()
    }

    fn building_number(&mut self) -> String {
        self.inner.building_number()
    }

    fn city(&mut self) -> String {
        self.inner.city()
    }

    fn city_prefix(&mut self) -> &'static str {
        self.inner.city_prefix()
    }

    fn city_suffix(&mut self) -> &'static str {
        self.inner.city_suffix()
    }

    fn country(&mut self) -> &'static str {
        self.inner.country()
    }

    #[pyo3(signature = (representation = "alpha-2"))]
    fn country_code(&mut self, representation: &str) -> PyResult<&'static str> {
        self.inner.country_code(representation).py()
    }

    fn current_country(&self) -> PyResult<&'static str> {
        self.inner.current_country().py()
    }

    fn current_country_code(&self) -> PyResult<&'static str> {
        self.inner.current_country_code().py()
    }

    fn military_apo(&mut self) -> String {
        self.inner.military_apo()
    }

    fn military_dpo(&mut self) -> String {
        self.inner.military_dpo()
    }

    fn military_ship(&mut self) -> &'static str {
        self.inner.military_ship()
    }

    fn military_state(&mut self) -> &'static str {
        self.inner.military_state()
    }

    fn postcode(&mut self) -> String {
        self.inner.postcode()
    }

    fn postalcode(&mut self) -> String {
        self.inner.postcode()
    }

    fn zipcode(&mut self) -> String {
        self.inner.postcode()
    }

    #[pyo3(signature = (state_abbr = None))]
    fn postcode_in_state(&mut self, state_abbr: Option<&str>) -> PyResult<String> {
        self.inner.postcode_in_state(state_abbr).py()
    }

    #[pyo3(signature = (state_abbr = None))]
    fn postalcode_in_state(&mut self, state_abbr: Option<&str>) -> PyResult<String> {
        self.inner.postcode_in_state(state_abbr).py()
    }

    #[pyo3(signature = (state_abbr = None))]
    fn zipcode_in_state(&mut self, state_abbr: Option<&str>) -> PyResult<String> {
        self.inner.postcode_in_state(state_abbr).py()
    }

    fn zipcode_plus4(&mut self) -> String {
        self.inner.zipcode_plus4()
    }

    fn postalcode_plus4(&mut self) -> String {
        self.inner.zipcode_plus4()
    }

    fn secondary_address(&mut self) -> String {
        self.inner.secondary_address()
    }

    #[pyo3(signature = (include_territories = true, include_freely_associated_states = true))]
    fn state_abbr(
        &mut self,
        include_territories: bool,
        include_freely_associated_states: bool,
    ) -> &'static str {
        self.inner
            .state_abbr(include_territories, include_freely_associated_states)
    }

    fn street_address(&mut self) -> String {
        self.inner.street_address()
    }

    fn street_name(&mut self) -> String {
        self.inner.street_name()
    }

    fn street_suffix(&mut self) -> &'static str {
        self.inner.street_suffix()
    }

    // ---- company ----

    fn company(&mut self) -> String {
        self.inner.company()
    }

    fn company_suffix(&mut self) -> &'static str {
        self.inner.company_suffix()
    }

    fn catch_phrase(&mut self) -> String {
        self.inner.catch_phrase()
    }

    fn bs(&mut self) -> String {
        self.inner.bs()
    }

    // ---- phone_number ----

    fn phone_number(&mut self) -> String {
        self.inner.phone_number()
    }

    fn basic_phone_number(&mut self) -> String {
        self.inner.basic_phone_number()
    }

    fn country_calling_code(&mut self) -> &'static str {
        self.inner.country_calling_code()
    }

    fn msisdn(&mut self) -> String {
        self.inner.msisdn()
    }

    // ---- internet ----

    #[pyo3(signature = (safe = true, domain = None))]
    fn email(&mut self, safe: bool, domain: Option<&str>) -> String {
        self.inner.email(safe, domain)
    }

    fn safe_email(&mut self) -> String {
        self.inner.safe_email()
    }

    fn free_email(&mut self) -> String {
        self.inner.free_email()
    }

    fn company_email(&mut self) -> String {
        self.inner.company_email()
    }

    fn ascii_email(&mut self) -> String {
        self.inner.ascii_email()
    }

    fn ascii_safe_email(&mut self) -> String {
        self.inner.ascii_safe_email()
    }

    fn ascii_free_email(&mut self) -> String {
        self.inner.ascii_free_email()
    }

    fn ascii_company_email(&mut self) -> String {
        self.inner.ascii_company_email()
    }

    fn safe_domain_name(&mut self) -> &'static str {
        self.inner.safe_domain_name()
    }

    fn free_email_domain(&mut self) -> &'static str {
        self.inner.free_email_domain()
    }

    fn user_name(&mut self) -> String {
        self.inner.user_name()
    }

    #[pyo3(signature = (levels = 1))]
    fn hostname(&mut self, levels: i64) -> String {
        self.inner.hostname(levels)
    }

    #[pyo3(signature = (levels = 1))]
    fn domain_name(&mut self, levels: i64) -> PyResult<String> {
        self.inner.domain_name(levels).py()
    }

    fn domain_word(&mut self) -> String {
        self.inner.domain_word()
    }

    #[pyo3(signature = (year = None, month = None, day = None, tld = None, length = None))]
    fn dga(
        &mut self,
        year: Option<i64>,
        month: Option<i64>,
        day: Option<i64>,
        tld: Option<&str>,
        length: Option<i64>,
    ) -> PyResult<String> {
        self.inner.dga(year, month, day, tld, length).py()
    }

    fn tld(&mut self) -> &'static str {
        self.inner.tld()
    }

    fn http_method(&mut self) -> &'static str {
        self.inner.http_method()
    }

    #[pyo3(signature = (include_unassigned = true))]
    fn http_status_code(&mut self, include_unassigned: bool) -> u32 {
        self.inner.http_status_code(include_unassigned)
    }

    #[pyo3(signature = (schemes = None))]
    fn url(&mut self, schemes: Option<Vec<String>>) -> String {
        let schemes = to_str_vec(&schemes);
        self.inner.url(schemes.as_deref())
    }

    #[pyo3(signature = (schemes = None, deep = None))]
    fn uri(&mut self, schemes: Option<Vec<String>>, deep: Option<i64>) -> String {
        let schemes = to_str_vec(&schemes);
        self.inner.uri(schemes.as_deref(), deep)
    }

    fn uri_page(&mut self) -> &'static str {
        self.inner.uri_page()
    }

    #[pyo3(signature = (deep = None))]
    fn uri_path(&mut self, deep: Option<i64>) -> String {
        self.inner.uri_path(deep)
    }

    fn uri_extension(&mut self) -> &'static str {
        self.inner.uri_extension()
    }

    fn ipv4_network_class(&mut self) -> char {
        self.inner.ipv4_network_class()
    }

    #[pyo3(signature = (network = false, address_class = None, private = None))]
    fn ipv4(
        &mut self,
        network: bool,
        address_class: Option<&str>,
        private: Option<Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        let private = match private {
            Some(p) if p.is_instance_of::<pyo3::types::PyBool>() => Some(p.extract::<bool>()?),
            _ => None,
        };
        Ok(self.inner.ipv4(network, address_class, private))
    }

    #[pyo3(signature = (network = false, address_class = None))]
    fn ipv4_private(&mut self, network: bool, address_class: Option<&str>) -> String {
        self.inner.ipv4_private(network, address_class)
    }

    #[pyo3(signature = (network = false, address_class = None))]
    fn ipv4_public(&mut self, network: bool, address_class: Option<&str>) -> String {
        self.inner.ipv4_public(network, address_class)
    }

    #[pyo3(signature = (network = false))]
    fn ipv6(&mut self, network: bool) -> String {
        self.inner.ipv6(network)
    }

    #[pyo3(signature = (multicast = false))]
    fn mac_address(&mut self, multicast: bool) -> String {
        self.inner.mac_address(multicast)
    }

    #[pyo3(signature = (is_system = false, is_user = false, is_dynamic = false))]
    fn port_number(&mut self, is_system: bool, is_user: bool, is_dynamic: bool) -> u16 {
        self.inner.port_number(is_system, is_user, is_dynamic)
    }

    #[pyo3(signature = (value = None))]
    fn slug(&mut self, value: Option<&str>) -> PyResult<String> {
        self.inner.slug(value).py()
    }

    #[pyo3(signature = (width = None, height = None, placeholder_url = None))]
    fn image_url(
        &mut self,
        width: Option<i64>,
        height: Option<i64>,
        placeholder_url: Option<&str>,
    ) -> String {
        self.inner.image_url(width, height, placeholder_url)
    }

    fn iana_id(&mut self) -> String {
        self.inner.iana_id()
    }

    fn ripe_id(&mut self) -> String {
        self.inner.ripe_id()
    }

    #[pyo3(signature = (suffix = "FAKE"))]
    fn nic_handle(&mut self, suffix: &str) -> PyResult<String> {
        self.inner.nic_handle(suffix).py()
    }

    #[pyo3(signature = (count = 1, suffix = "????"))]
    fn nic_handles(&mut self, count: i64, suffix: &str) -> PyResult<Vec<String>> {
        self.inner.nic_handles(count, suffix).py()
    }

    // ---- lorem ----

    #[pyo3(signature = (part_of_speech = None, ext_word_list = None))]
    fn get_words_list(
        &mut self,
        part_of_speech: Option<&str>,
        ext_word_list: Option<Vec<String>>,
    ) -> PyResult<Vec<String>> {
        let ext = to_str_vec(&ext_word_list);
        Ok(self
            .inner
            .get_words_list(part_of_speech, ext.as_deref())
            .py()?
            .iter()
            .map(|w| (*w).to_owned())
            .collect())
    }

    #[pyo3(signature = (nb = 3, ext_word_list = None, part_of_speech = None, unique = false))]
    fn words(
        &mut self,
        nb: i64,
        ext_word_list: Option<Vec<String>>,
        part_of_speech: Option<&str>,
        unique: bool,
    ) -> PyResult<Vec<String>> {
        let ext = to_str_vec(&ext_word_list);
        Ok(self
            .inner
            .words(nb, ext.as_deref(), part_of_speech, unique)
            .py()?
            .into_iter()
            .map(str::to_owned)
            .collect())
    }

    #[pyo3(signature = (part_of_speech = None, ext_word_list = None))]
    fn word(
        &mut self,
        part_of_speech: Option<&str>,
        ext_word_list: Option<Vec<String>>,
    ) -> PyResult<String> {
        let ext = to_str_vec(&ext_word_list);
        Ok(self
            .inner
            .word(part_of_speech, ext.as_deref())
            .py()?
            .to_owned())
    }

    #[pyo3(signature = (nb_words = 6, variable_nb_words = true, ext_word_list = None))]
    fn sentence(
        &mut self,
        nb_words: i64,
        variable_nb_words: bool,
        ext_word_list: Option<Vec<String>>,
    ) -> PyResult<String> {
        let ext = to_str_vec(&ext_word_list);
        self.inner
            .sentence(nb_words, variable_nb_words, ext.as_deref())
            .py()
    }

    #[pyo3(signature = (nb = 3, ext_word_list = None))]
    fn sentences(&mut self, nb: i64, ext_word_list: Option<Vec<String>>) -> PyResult<Vec<String>> {
        let ext = to_str_vec(&ext_word_list);
        self.inner.sentences(nb, ext.as_deref()).py()
    }

    #[pyo3(signature = (nb_sentences = 3, variable_nb_sentences = true, ext_word_list = None))]
    fn paragraph(
        &mut self,
        nb_sentences: i64,
        variable_nb_sentences: bool,
        ext_word_list: Option<Vec<String>>,
    ) -> PyResult<String> {
        let ext = to_str_vec(&ext_word_list);
        self.inner
            .paragraph(nb_sentences, variable_nb_sentences, ext.as_deref())
            .py()
    }

    #[pyo3(signature = (nb = 3, ext_word_list = None))]
    fn paragraphs(&mut self, nb: i64, ext_word_list: Option<Vec<String>>) -> PyResult<Vec<String>> {
        let ext = to_str_vec(&ext_word_list);
        self.inner.paragraphs(nb, ext.as_deref()).py()
    }

    #[pyo3(signature = (max_nb_chars = 200, ext_word_list = None))]
    fn text(&mut self, max_nb_chars: i64, ext_word_list: Option<Vec<String>>) -> PyResult<String> {
        let ext = to_str_vec(&ext_word_list);
        self.inner.text(max_nb_chars, ext.as_deref()).py()
    }

    #[pyo3(signature = (nb_texts = 3, max_nb_chars = 200, ext_word_list = None))]
    fn texts(
        &mut self,
        nb_texts: i64,
        max_nb_chars: i64,
        ext_word_list: Option<Vec<String>>,
    ) -> PyResult<Vec<String>> {
        let ext = to_str_vec(&ext_word_list);
        self.inner
            .texts(nb_texts, max_nb_chars, ext.as_deref())
            .py()
    }

    // ---- date_time ----

    #[pyo3(signature = (end_datetime = None, start_datetime = None))]
    fn unix_time(
        &mut self,
        end_datetime: Option<Bound<'_, PyAny>>,
        start_datetime: Option<Bound<'_, PyAny>>,
    ) -> PyResult<f64> {
        let end = parse_opt(end_datetime.as_ref())?;
        let start = parse_opt(start_datetime.as_ref())?;
        self.inner
            .unix_time(
                end.as_ref().map(ParsedDate::value),
                start.as_ref().map(ParsedDate::value),
            )
            .py()
    }

    #[pyo3(signature = (end_datetime = None))]
    fn time_delta(
        &mut self,
        end_datetime: Option<Bound<'_, PyAny>>,
    ) -> PyResult<chrono::TimeDelta> {
        let end = parse_opt(end_datetime.as_ref())?;
        self.inner
            .time_delta(end.as_ref().map(ParsedDate::value))
            .py()
    }

    #[pyo3(signature = (tzinfo = None, end_datetime = None))]
    fn date_time<'py>(
        slf: &Bound<'py, Self>,
        tzinfo: Option<Bound<'py, PyAny>>,
        end_datetime: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let tz = tzinfo_of(tzinfo.as_ref())?;
        let end = parse_opt(end_datetime.as_ref())?;
        let dt = slf
            .borrow_mut()
            .inner
            .date_time(end.as_ref().map(ParsedDate::value))
            .py()?;
        match tz {
            Some(tz) => Ok(attach_tz(py, dt, &tz)?.into_any()),
            None => Ok(dt.into_pyobject(py)?.into_any()),
        }
    }

    #[pyo3(signature = (tzinfo = None, end_datetime = None, start_datetime = None))]
    fn date_time_ad<'py>(
        slf: &Bound<'py, Self>,
        tzinfo: Option<Bound<'py, PyAny>>,
        end_datetime: Option<Bound<'py, PyAny>>,
        start_datetime: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let tz = tzinfo_of(tzinfo.as_ref())?;
        let end = parse_opt(end_datetime.as_ref())?;
        let start = parse_opt(start_datetime.as_ref())?;
        let dt = slf
            .borrow_mut()
            .inner
            .date_time_ad(
                end.as_ref().map(ParsedDate::value),
                start.as_ref().map(ParsedDate::value),
            )
            .py()?;
        match tz {
            Some(tz) => Ok(attach_tz(py, dt, &tz)?.into_any()),
            None => Ok(dt.into_pyobject(py)?.into_any()),
        }
    }

    #[pyo3(signature = (tzinfo = None, end_datetime = None, sep = "T", timespec = "auto"))]
    fn iso8601<'py>(
        slf: &Bound<'py, Self>,
        tzinfo: Option<Bound<'py, PyAny>>,
        end_datetime: Option<Bound<'py, PyAny>>,
        sep: &str,
        timespec: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let end = parse_opt(end_datetime.as_ref())?;
        let Some(tz) = tzinfo_of(tzinfo.as_ref())? else {
            let s = slf
                .borrow_mut()
                .inner
                .iso8601(end.as_ref().map(ParsedDate::value), sep, timespec)
                .py()?;
            return Ok(PyString::new(py, &s).into_any());
        };
        let dt = slf
            .borrow_mut()
            .inner
            .date_time(end.as_ref().map(ParsedDate::value))
            .py()?;
        attach_tz(py, dt, &tz)?.call_method1("isoformat", (sep, timespec))
    }

    #[pyo3(signature = (pattern = "%Y-%m-%d", end_datetime = None))]
    fn date(&mut self, pattern: &str, end_datetime: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        let end = parse_opt(end_datetime.as_ref())?;
        self.inner
            .date(pattern, end.as_ref().map(ParsedDate::value))
            .py()
    }

    #[pyo3(signature = (end_datetime = None))]
    fn date_object(&mut self, end_datetime: Option<Bound<'_, PyAny>>) -> PyResult<NaiveDate> {
        let end = parse_opt(end_datetime.as_ref())?;
        self.inner
            .date_object(end.as_ref().map(ParsedDate::value))
            .py()
    }

    #[pyo3(signature = (pattern = "%H:%M:%S", end_datetime = None))]
    fn time(&mut self, pattern: &str, end_datetime: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        let end = parse_opt(end_datetime.as_ref())?;
        self.inner
            .time(pattern, end.as_ref().map(ParsedDate::value))
            .py()
    }

    #[pyo3(signature = (end_datetime = None))]
    fn time_object(
        &mut self,
        end_datetime: Option<Bound<'_, PyAny>>,
    ) -> PyResult<chrono::NaiveTime> {
        let end = parse_opt(end_datetime.as_ref())?;
        self.inner
            .time_object(end.as_ref().map(ParsedDate::value))
            .py()
    }

    #[pyo3(signature = (start_date = None, end_date = None, tzinfo = None))]
    fn date_time_between<'py>(
        slf: &Bound<'py, Self>,
        start_date: Option<Bound<'py, PyAny>>,
        end_date: Option<Bound<'py, PyAny>>,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        between(slf, start_date, end_date, tzinfo, "-30y", "now")
    }

    #[pyo3(signature = (start_date = None, end_date = None))]
    fn date_between(
        &mut self,
        start_date: Option<Bound<'_, PyAny>>,
        end_date: Option<Bound<'_, PyAny>>,
    ) -> PyResult<NaiveDate> {
        let start = parse_or(start_date.as_ref(), "-30y")?;
        let end = parse_or(end_date.as_ref(), "today")?;
        self.inner.date_between(start.value(), end.value()).py()
    }

    #[pyo3(signature = (end_date = None, tzinfo = None))]
    fn future_datetime<'py>(
        slf: &Bound<'py, Self>,
        end_date: Option<Bound<'py, PyAny>>,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        between(slf, None, end_date, tzinfo, "+1s", "+30d")
    }

    #[pyo3(signature = (end_date = None))]
    fn future_date(&mut self, end_date: Option<Bound<'_, PyAny>>) -> PyResult<NaiveDate> {
        let end = parse_or(end_date.as_ref(), "+30d")?;
        self.inner.future_date(end.value()).py()
    }

    #[pyo3(signature = (start_date = None, tzinfo = None))]
    fn past_datetime<'py>(
        slf: &Bound<'py, Self>,
        start_date: Option<Bound<'py, PyAny>>,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        between(slf, start_date, None, tzinfo, "-30d", "-1s")
    }

    #[pyo3(signature = (start_date = None, tzinfo = None))]
    fn past_date(
        &mut self,
        start_date: Option<Bound<'_, PyAny>>,
        tzinfo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<NaiveDate> {
        let _ = tzinfo;
        let start = parse_or(start_date.as_ref(), "-30d")?;
        self.inner.past_date(start.value()).py()
    }

    #[pyo3(signature = (datetime_start = None, datetime_end = None, tzinfo = None))]
    fn date_time_between_dates<'py>(
        slf: &Bound<'py, Self>,
        datetime_start: Option<Bound<'py, PyAny>>,
        datetime_end: Option<Bound<'py, PyAny>>,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let tz = tzinfo_of(tzinfo.as_ref())?;
        let start = parse_opt(datetime_start.as_ref())?;
        let end = parse_opt(datetime_end.as_ref())?;
        let clock = if tz.is_some() {
            Clock::Utc
        } else {
            Clock::NaiveLocal
        };
        let ts = slf
            .borrow_mut()
            .inner
            .date_time_between_dates_ts(
                start.as_ref().map(ParsedDate::value),
                end.as_ref().map(ParsedDate::value),
                clock,
            )
            .py()?;
        match tz {
            Some(tz) => Ok(at_timestamp(py, ts, &tz)?.into_any()),
            None => Ok(timestamp_to_naive(ts).py()?.into_pyobject(py)?.into_any()),
        }
    }

    #[pyo3(signature = (date_start = None, date_end = None))]
    fn date_between_dates(
        &mut self,
        date_start: Option<Bound<'_, PyAny>>,
        date_end: Option<Bound<'_, PyAny>>,
    ) -> PyResult<NaiveDate> {
        let start = parse_opt(date_start.as_ref())?;
        let end = parse_opt(date_end.as_ref())?;
        self.inner
            .date_between_dates(
                start.as_ref().map(ParsedDate::value),
                end.as_ref().map(ParsedDate::value),
            )
            .py()
    }

    #[pyo3(signature = (before_now = true, after_now = false, tzinfo = None))]
    fn date_time_this_century<'py>(
        slf: &Bound<'py, Self>,
        before_now: bool,
        after_now: bool,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        this_period(
            slf,
            Period::Century,
            before_now,
            after_now,
            tzinfo,
            Generator::date_time_this_century,
        )
    }

    #[pyo3(signature = (before_now = true, after_now = false, tzinfo = None))]
    fn date_time_this_decade<'py>(
        slf: &Bound<'py, Self>,
        before_now: bool,
        after_now: bool,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        this_period(
            slf,
            Period::Decade,
            before_now,
            after_now,
            tzinfo,
            Generator::date_time_this_decade,
        )
    }

    #[pyo3(signature = (before_now = true, after_now = false, tzinfo = None))]
    fn date_time_this_year<'py>(
        slf: &Bound<'py, Self>,
        before_now: bool,
        after_now: bool,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        this_period(
            slf,
            Period::Year,
            before_now,
            after_now,
            tzinfo,
            Generator::date_time_this_year,
        )
    }

    #[pyo3(signature = (before_now = true, after_now = false, tzinfo = None))]
    fn date_time_this_month<'py>(
        slf: &Bound<'py, Self>,
        before_now: bool,
        after_now: bool,
        tzinfo: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        this_period(
            slf,
            Period::Month,
            before_now,
            after_now,
            tzinfo,
            Generator::date_time_this_month,
        )
    }

    #[pyo3(signature = (before_today = true, after_today = false))]
    fn date_this_century(&mut self, before_today: bool, after_today: bool) -> PyResult<NaiveDate> {
        self.inner.date_this_century(before_today, after_today).py()
    }

    #[pyo3(signature = (before_today = true, after_today = false))]
    fn date_this_decade(&mut self, before_today: bool, after_today: bool) -> PyResult<NaiveDate> {
        self.inner.date_this_decade(before_today, after_today).py()
    }

    #[pyo3(signature = (before_today = true, after_today = false))]
    fn date_this_year(&mut self, before_today: bool, after_today: bool) -> PyResult<NaiveDate> {
        self.inner.date_this_year(before_today, after_today).py()
    }

    #[pyo3(signature = (before_today = true, after_today = false))]
    fn date_this_month(&mut self, before_today: bool, after_today: bool) -> PyResult<NaiveDate> {
        self.inner.date_this_month(before_today, after_today).py()
    }

    fn am_pm(&mut self) -> PyResult<&'static str> {
        self.inner.am_pm().py()
    }

    fn day_of_month(&mut self) -> PyResult<String> {
        self.inner.day_of_month().py()
    }

    fn day_of_week(&mut self) -> PyResult<String> {
        self.inner.day_of_week().py()
    }

    fn month(&mut self) -> PyResult<String> {
        self.inner.month().py()
    }

    fn month_name(&mut self) -> PyResult<String> {
        self.inner.month_name().py()
    }

    fn year(&mut self) -> PyResult<String> {
        self.inner.year().py()
    }

    fn century(&mut self) -> &'static str {
        self.inner.century()
    }

    fn timezone(&mut self) -> &'static str {
        self.inner.timezone()
    }

    #[pyo3(signature = (tzinfo = None, minimum_age = 0, maximum_age = 115))]
    fn date_of_birth<'py>(
        slf: &Bound<'py, Self>,
        tzinfo: Option<Bound<'py, PyAny>>,
        minimum_age: i64,
        maximum_age: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let dob = match tzinfo_of(tzinfo.as_ref())? {
            // Only "today" depends on the zone; the result is a plain date either way.
            Some(tz) => {
                let now = now_in_tz(py, &tz)?;
                let (year, month, day) = ymd_of(&now)?;
                let today = NaiveDate::from_ymd_opt(year, month.into(), day.into())
                    .ok_or_else(|| PyOverflowError::new_err("date value out of range"))?;
                slf.borrow_mut()
                    .inner
                    .date_of_birth_from(today, minimum_age, maximum_age)
                    .py()?
            }
            None => slf
                .borrow_mut()
                .inner
                .date_of_birth(minimum_age, maximum_age)
                .py()?,
        };
        Ok(dob.into_pyobject(py)?.into_any())
    }
}

/// Faker's `date_time_between`-style methods: one timestamp, rendered naive or in `tz`.
fn between<'py>(
    slf: &Bound<'py, PyGenerator>,
    start_date: Option<Bound<'py, PyAny>>,
    end_date: Option<Bound<'py, PyAny>>,
    tzinfo: Option<Bound<'py, PyAny>>,
    default_start: &str,
    default_end: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    let tz = tzinfo_of(tzinfo.as_ref())?;
    let start = parse_or(start_date.as_ref(), default_start)?;
    let end = parse_or(end_date.as_ref(), default_end)?;
    let clock = if tz.is_some() {
        Clock::Utc
    } else {
        Clock::NaiveLocal
    };
    let ts = slf
        .borrow_mut()
        .inner
        .date_time_between_ts(start.value(), end.value(), clock)
        .py()?;
    match tz {
        Some(tz) => Ok(at_timestamp(py, ts, &tz)?.into_any()),
        None => Ok(timestamp_to_naive(ts).py()?.into_pyobject(py)?.into_any()),
    }
}

/// The period the `*_this_*` methods draw from.
#[derive(Clone, Copy)]
enum Period {
    Century,
    Decade,
    Year,
    Month,
}

impl Period {
    /// Start and end wall-clock dates of the period containing `(year, month)`.
    fn bounds(self, year: i32, month: u8) -> ((i32, u8, u8), (i32, u8, u8)) {
        match self {
            Period::Century => {
                let start = year - year.rem_euclid(100);
                ((start, 1, 1), ((start + 100).min(MAX_YEAR), 1, 1))
            }
            Period::Decade => {
                let start = year - year.rem_euclid(10);
                ((start, 1, 1), ((start + 10).min(MAX_YEAR), 1, 1))
            }
            Period::Year => ((year, 1, 1), (year + 1, 1, 1)),
            Period::Month if month == 12 => ((year, 12, 1), (year + 1, 1, 1)),
            Period::Month => ((year, month, 1), (year, month + 1, 1)),
        }
    }
}

fn this_period<'py>(
    slf: &Bound<'py, PyGenerator>,
    period: Period,
    before_now: bool,
    after_now: bool,
    tzinfo: Option<Bound<'py, PyAny>>,
    naive_method: fn(&mut Generator, bool, bool) -> Result<NaiveDateTime, Error>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    let Some(tz) = tzinfo_of(tzinfo.as_ref())? else {
        let dt = naive_method(&mut slf.borrow_mut().inner, before_now, after_now).py()?;
        return Ok(dt.into_pyobject(py)?.into_any());
    };

    // The boundaries are wall-clock times in `tz`, so their timestamps depend on that zone's
    // offset at the time; let Python apply the zone.
    let now = now_in_tz(py, &tz)?;
    if !before_now && !after_now {
        return Ok(now.into_any());
    }
    let (year, month, _) = ymd_of(&now)?;
    let (start, end) = period.bounds(year, month);
    let boundary = |(y, m, d): (i32, u8, u8)| -> PyResult<i64> {
        floor_timestamp_of(&PyDateTime::new(py, y, m, d, 0, 0, 0, 0, Some(&tz))?)
    };
    let (from, to) = match (before_now, after_now) {
        (true, true) => (boundary(start)?, boundary(end)?),
        (false, true) => (floor_timestamp_of(&now)?, boundary(end)?),
        _ => (boundary(start)?, floor_timestamp_of(&now)?),
    };
    let ts = slf
        .borrow_mut()
        .inner
        .date_time_between_dates_ts(
            Some(DateTimeValue::Timestamp(from)),
            Some(DateTimeValue::Timestamp(to)),
            Clock::Utc,
        )
        .py()?;
    Ok(at_timestamp(py, ts, &tz)?.into_any())
}

/// Public formatter names implemented in Rust (kept in sync by `tests/compat`).
const FORMATTER_NAMES: &[&str] = &[
    "random_int",
    "random_digit",
    "random_digit_not_null",
    "random_digit_above_two",
    "random_digit_or_empty",
    "random_digit_not_null_or_empty",
    "random_number",
    "random_letter",
    "random_letters",
    "random_lowercase_letter",
    "random_uppercase_letter",
    "random_elements",
    "random_choices",
    "random_element",
    "random_sample",
    "randomize_nb_elements",
    "numerify",
    "lexify",
    "bothify",
    "hexify",
    "locale",
    "language_code",
    "name",
    "name_female",
    "name_male",
    "name_nonbinary",
    "first_name",
    "first_name_female",
    "first_name_male",
    "first_name_nonbinary",
    "last_name",
    "last_name_female",
    "last_name_male",
    "last_name_nonbinary",
    "prefix",
    "prefix_female",
    "prefix_male",
    "prefix_nonbinary",
    "suffix",
    "suffix_female",
    "suffix_male",
    "suffix_nonbinary",
    "language_name",
    "address",
    "administrative_unit",
    "state",
    "building_number",
    "city",
    "city_prefix",
    "city_suffix",
    "country",
    "country_code",
    "current_country",
    "current_country_code",
    "military_apo",
    "military_dpo",
    "military_ship",
    "military_state",
    "postcode",
    "postalcode",
    "zipcode",
    "postcode_in_state",
    "postalcode_in_state",
    "zipcode_in_state",
    "zipcode_plus4",
    "postalcode_plus4",
    "secondary_address",
    "state_abbr",
    "street_address",
    "street_name",
    "street_suffix",
    "company",
    "company_suffix",
    "catch_phrase",
    "bs",
    "phone_number",
    "basic_phone_number",
    "country_calling_code",
    "msisdn",
    "email",
    "safe_email",
    "free_email",
    "company_email",
    "ascii_email",
    "ascii_safe_email",
    "ascii_free_email",
    "ascii_company_email",
    "safe_domain_name",
    "free_email_domain",
    "user_name",
    "hostname",
    "domain_name",
    "domain_word",
    "dga",
    "tld",
    "http_method",
    "http_status_code",
    "url",
    "uri",
    "uri_page",
    "uri_path",
    "uri_extension",
    "ipv4_network_class",
    "ipv4",
    "ipv4_private",
    "ipv4_public",
    "ipv6",
    "mac_address",
    "port_number",
    "slug",
    "image_url",
    "iana_id",
    "ripe_id",
    "nic_handle",
    "nic_handles",
    "get_words_list",
    "words",
    "word",
    "sentence",
    "sentences",
    "paragraph",
    "paragraphs",
    "text",
    "texts",
    "unix_time",
    "time_delta",
    "date_time",
    "date_time_ad",
    "iso8601",
    "date",
    "date_object",
    "time",
    "time_object",
    "date_time_between",
    "date_between",
    "future_datetime",
    "future_date",
    "past_datetime",
    "past_date",
    "date_time_between_dates",
    "date_between_dates",
    "date_time_this_century",
    "date_time_this_decade",
    "date_time_this_year",
    "date_time_this_month",
    "date_this_century",
    "date_this_decade",
    "date_this_year",
    "date_this_month",
    "am_pm",
    "day_of_month",
    "day_of_week",
    "month",
    "month_name",
    "year",
    "century",
    "timezone",
    "date_of_birth",
];

#[pymodule]
mod _core {
    use super::*;

    #[pymodule_export]
    use super::PyGenerator;

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("ParseError", m.py().get_type::<ParseError>())?;
        m.add("LOCALES", rusty_faker_core::LOCALE_CODES.to_vec())?;
        Ok(())
    }
}
