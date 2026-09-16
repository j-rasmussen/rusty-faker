//! Faker's `BaseProvider`: random primitives and placeholder substitution.

use std::fmt::Write as _;

use crate::error::{Error, Result, invalid};
use crate::generator::Generator;

const ASCII_LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const HEX_LOWER: &[u8] = b"0123456789abcdef";
const HEX_UPPER: &[u8] = b"0123456789ABCDEF";

impl Generator {
    /// Random integer from `range(min, max + 1, step)` (Python `randrange` semantics).
    pub fn random_int(&mut self, min: i64, max: i64, step: i64) -> Result<i64> {
        let (start, stop, step) = (i128::from(min), i128::from(max) + 1, i128::from(step));
        if step == 0 {
            return Err(invalid("zero step for randrange()"));
        }
        let count = if step > 0 {
            (stop - start + step - 1) / step
        } else {
            (stop - start + step + 1) / step
        };
        if count <= 0 {
            return Err(invalid(format!(
                "empty range in randrange({min}, {}, {step})",
                stop
            )));
        }
        let count = u64::try_from(count).map_err(|_| Error::Overflow("range too large".into()))?;
        let offset = if count == 1 {
            0
        } else {
            self.randint(0, i64::try_from(count - 1).unwrap_or(i64::MAX))
        };
        i64::try_from(start + step * i128::from(offset))
            .map_err(|_| Error::Overflow("result out of range".into()))
    }

    /// Digit 0–9.
    pub fn random_digit(&mut self) -> u8 {
        self.digit(0)
    }

    /// Digit 1–9.
    pub fn random_digit_not_null(&mut self) -> u8 {
        self.digit(1)
    }

    /// Digit 2–9.
    pub fn random_digit_above_two(&mut self) -> u8 {
        self.digit(2)
    }

    /// Digit 0–9 half of the time, otherwise `None` (Faker returns `""`).
    pub fn random_digit_or_empty(&mut self) -> Option<u8> {
        (self.randint(0, 1) == 1).then(|| self.digit(0))
    }

    /// Digit 1–9 half of the time, otherwise `None` (Faker returns `""`).
    pub fn random_digit_not_null_or_empty(&mut self) -> Option<u8> {
        (self.randint(0, 1) == 1).then(|| self.digit(1))
    }

    fn digit(&mut self, low: u8) -> u8 {
        // Digits are tiny, so the conversion cannot fail.
        u8::try_from(self.randint(i64::from(low), 9)).unwrap_or(low)
    }

    /// Random number with up to (or, with `fix_len`, exactly) `digits` digits.
    /// `digits` defaults to a random 1–9. At most 38 digits are supported.
    pub fn random_number(&mut self, digits: Option<i64>, fix_len: bool) -> Result<u128> {
        let digits = match digits {
            Some(d) => d,
            None => i64::from(self.random_digit_not_null()),
        };
        if digits < 0 {
            return Err(invalid(
                "The digit parameter must be greater than or equal to 0.",
            ));
        }
        let digits = u32::try_from(digits)
            .ok()
            .filter(|d| *d <= 38)
            .ok_or_else(|| Error::Overflow("at most 38 digits are supported".into()))?;
        let upper = 10u128.pow(digits) - 1;
        let lower = match (fix_len, digits) {
            (false, _) => 0,
            (true, 0) => {
                return Err(invalid(
                    "A number of fixed length cannot have less than 1 digit in it.",
                ));
            }
            (true, d) => 10u128.pow(d - 1),
        };
        Ok(self.random_u128(lower, upper))
    }

    fn random_u128(&mut self, lo: u128, hi: u128) -> u128 {
        use rand::RngExt as _;
        if hi <= lo {
            return lo;
        }
        self.rng().random_range(lo..=hi)
    }

    /// Random ASCII letter (a–z, A–Z).
    pub fn random_letter(&mut self) -> char {
        char::from(ASCII_LETTERS[self.index(ASCII_LETTERS.len())])
    }

    /// `length` random ASCII letters (with replacement).
    pub fn random_letters(&mut self, length: usize) -> Vec<char> {
        (0..length).map(|_| self.random_letter()).collect()
    }

    pub fn random_lowercase_letter(&mut self) -> char {
        char::from(ASCII_LETTERS[self.index(26)])
    }

    pub fn random_uppercase_letter(&mut self) -> char {
        char::from(ASCII_LETTERS[26 + self.index(26)])
    }

    /// Indices for Faker's `random_elements` over a collection of `len` items.
    ///
    /// * `length`: `None` picks a random length from 1 to `len`.
    /// * `unique`: sample without replacement (weighted when `weights` is given).
    /// * `weights`: per-item weights; `None` means uniform.
    pub fn random_element_indices(
        &mut self,
        len: usize,
        weights: Option<&[f64]>,
        length: Option<usize>,
        unique: bool,
    ) -> Result<Vec<usize>> {
        if let Some(weights) = weights
            && weights.len() != len
        {
            return Err(invalid("weights must have the same length as elements"));
        }
        let length = match length {
            Some(length) => length,
            None if len == 0 => return Err(invalid("empty range in randrange(1, 1)")),
            None => usize::try_from(self.randint(1, i64::try_from(len).unwrap_or(i64::MAX)))
                .unwrap_or(1),
        };
        if unique && length > len {
            return Err(invalid(
                "Sample length cannot be longer than the number of unique elements to pick from.",
            ));
        }
        if length > 0 && len == 0 {
            return Err(Error::EmptySequence);
        }
        match (unique, weights) {
            (false, None) => Ok((0..length).map(|_| self.index(len)).collect()),
            (false, Some(weights)) => self.weighted_choices(weights, length),
            (true, None) => Ok(self.sample_indices(len, length)),
            (true, Some(weights)) => self.weighted_unique(weights, length),
        }
    }

    fn weighted_choices(&mut self, weights: &[f64], length: usize) -> Result<Vec<usize>> {
        let cum = cumulative(weights)?;
        let total = cum.last().copied().unwrap_or(0.0);
        Ok((0..length)
            .map(|_| {
                let x = self.unit() * total;
                cum.partition_point(|&c| c <= x).min(cum.len() - 1)
            })
            .collect())
    }

    fn weighted_unique(&mut self, weights: &[f64], length: usize) -> Result<Vec<usize>> {
        cumulative(weights)?;
        let mut remaining: Vec<(usize, f64)> = weights.iter().copied().enumerate().collect();
        let mut chosen = Vec::with_capacity(length);
        for _ in 0..length {
            let total: f64 = remaining.iter().map(|(_, w)| w).sum();
            let mut x = self.unit() * total;
            let mut pos = remaining.len() - 1;
            for (i, (_, w)) in remaining.iter().enumerate() {
                if x < *w {
                    pos = i;
                    break;
                }
                x -= w;
            }
            chosen.push(remaining.remove(pos).0);
        }
        Ok(chosen)
    }

    /// `length` distinct indices from `0..len` in random order (partial Fisher–Yates).
    pub fn sample_indices(&mut self, len: usize, length: usize) -> Vec<usize> {
        let length = length.min(len);
        let mut pool: Vec<usize> = (0..len).collect();
        for i in 0..length {
            let j = i + self.index(len - i);
            pool.swap(i, j);
        }
        pool.truncate(length);
        pool
    }

    /// Faker's `randomize_nb_elements`: `number` scaled by a random 60–140%.
    pub fn randomize_nb_elements(
        &mut self,
        number: i64,
        le: bool,
        ge: bool,
        min: Option<i64>,
        max: Option<i64>,
    ) -> i64 {
        if le && ge {
            return number;
        }
        let low = if ge { 100 } else { 60 };
        let high = if le { 100 } else { 140 };
        let percent = self.randint(low, high);
        let scaled = i128::from(number) * i128::from(percent);
        let mut nb = (scaled as f64 / 100.0).trunc() as i64;
        if let Some(min) = min
            && nb < min
        {
            nb = min;
        }
        if let Some(max) = max
            && nb > max
        {
            nb = max;
        }
        nb
    }

    /// Replaces `#` (0–9), `%` (1–9), `$` (2–9), `!` (0–9 or nothing) and `@` (1–9 or nothing).
    pub fn numerify(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        self.write_numerify(text, &mut out);
        out
    }

    pub(crate) fn write_numerify(&mut self, text: &str, out: &mut String) {
        for c in text.chars() {
            match c {
                '#' => out.push(char::from(b'0' + self.random_digit())),
                '%' => out.push(char::from(b'0' + self.random_digit_not_null())),
                '$' => out.push(char::from(b'0' + self.random_digit_above_two())),
                '!' => {
                    if let Some(d) = self.random_digit_or_empty() {
                        out.push(char::from(b'0' + d));
                    }
                }
                '@' => {
                    if let Some(d) = self.random_digit_not_null_or_empty() {
                        out.push(char::from(b'0' + d));
                    }
                }
                _ => out.push(c),
            }
        }
    }

    /// Replaces `?` with a random character from `letters` (default: ASCII letters).
    pub fn lexify(&mut self, text: &str, letters: Option<&str>) -> Result<String> {
        let mut out = String::with_capacity(text.len());
        self.write_replaced(
            text,
            '?',
            letters.unwrap_or(""),
            letters.is_none(),
            &mut out,
        )?;
        Ok(out)
    }

    /// `numerify` followed by `lexify`.
    pub fn bothify(&mut self, text: &str, letters: Option<&str>) -> Result<String> {
        let numbered = self.numerify(text);
        self.lexify(&numbered, letters)
    }

    /// Replaces `^` with a random hex digit.
    pub fn hexify(&mut self, text: &str, upper: bool) -> String {
        let digits = if upper { HEX_UPPER } else { HEX_LOWER };
        text.chars()
            .map(|c| {
                if c == '^' {
                    char::from(digits[self.index(16)])
                } else {
                    c
                }
            })
            .collect()
    }

    /// `bothify` with the default letters, which cannot fail.
    pub(crate) fn write_bothify_default(&mut self, text: &str, out: &mut String) {
        for c in text.chars() {
            match c {
                '?' => out.push(self.random_letter()),
                '#' | '%' | '$' | '!' | '@' => {
                    let mut buf = [0u8; 4];
                    self.write_numerify(c.encode_utf8(&mut buf), out);
                }
                _ => out.push(c),
            }
        }
    }

    fn write_replaced(
        &mut self,
        text: &str,
        marker: char,
        letters: &str,
        default_letters: bool,
        out: &mut String,
    ) -> Result<()> {
        if !text.contains(marker) {
            out.push_str(text);
            return Ok(());
        }
        if default_letters {
            for c in text.chars() {
                out.push(if c == marker { self.random_letter() } else { c });
            }
            return Ok(());
        }
        let pool: Vec<char> = letters.chars().collect();
        if pool.is_empty() {
            return Err(Error::EmptySequence);
        }
        for c in text.chars() {
            out.push(if c == marker {
                pool[self.index(pool.len())]
            } else {
                c
            });
        }
        Ok(())
    }

    /// Random language code from the locale's `language_locale_codes` (e.g. `de`).
    pub fn language_code(&mut self) -> &'static str {
        let codes = self.locale_data().base.language_locale_codes;
        codes[self.index(codes.len())].0
    }

    /// Random locale code such as `de_AT`.
    pub fn locale(&mut self) -> String {
        let codes = self.locale_data().base.language_locale_codes;
        let (language, regions) = codes[self.index(codes.len())];
        let mut out = String::with_capacity(language.len() + 3);
        out.push_str(language);
        if !regions.is_empty() {
            let _ = write!(out, "_{}", regions[self.index(regions.len())]);
        }
        out
    }
}

fn cumulative(weights: &[f64]) -> Result<Vec<f64>> {
    let mut total = 0.0;
    let mut cum = Vec::with_capacity(weights.len());
    for &w in weights {
        if !w.is_finite() || w < 0.0 {
            return Err(invalid("weights must be non-negative finite numbers"));
        }
        total += w;
        cum.push(total);
    }
    if total <= 0.0 && !weights.is_empty() {
        return Err(invalid("Total of weights must be greater than zero"));
    }
    Ok(cum)
}

#[cfg(test)]
mod tests {
    use crate::{Error, Generator};

    fn fake() -> Generator {
        Generator::seeded("en_US", 1).unwrap()
    }

    #[test]
    fn random_int_respects_range_and_step() {
        let mut g = fake();
        for _ in 0..1000 {
            let n = g.random_int(10, 20, 5).unwrap();
            assert!([10, 15, 20].contains(&n), "{n}");
        }
        assert_eq!(g.random_int(5, 5, 1).unwrap(), 5);
        assert!(g.random_int(5, 4, 1).is_err());
        assert!(g.random_int(0, 10, 0).is_err());
        let n = g.random_int(10, 0, -2).unwrap();
        assert!((1..=10).contains(&n) && n % 2 == 0);
    }

    #[test]
    fn numerify_leaves_no_placeholders() {
        let mut g = fake();
        for _ in 0..200 {
            let s = g.numerify("#-%-$-!-@");
            assert!(!s.contains(['#', '%', '$', '!', '@']), "{s}");
            let first = s.chars().next().unwrap();
            assert!(first.is_ascii_digit());
        }
    }

    #[test]
    fn bothify_lexify_hexify() {
        let mut g = fake();
        let s = g.bothify("## ??", None).unwrap();
        assert_eq!(s.len(), 5);
        assert!(
            s[..2].chars().all(|c| c.is_ascii_digit())
                && s[3..].chars().all(|c| c.is_ascii_alphabetic())
        );
        assert_eq!(g.lexify("??", Some("x")).unwrap(), "xx");
        assert_eq!(g.lexify("?", Some("")), Err(Error::EmptySequence));
        assert!(
            g.hexify("^^^^", true)
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );
    }

    #[test]
    fn random_number_bounds() {
        let mut g = fake();
        for _ in 0..500 {
            let n = g.random_number(Some(3), true).unwrap();
            assert!((100..=999).contains(&n));
        }
        assert!(g.random_number(Some(0), true).is_err());
        assert!(g.random_number(Some(-1), false).is_err());
        assert_eq!(g.random_number(Some(0), false).unwrap(), 0);
    }

    #[test]
    fn element_indices_rules() {
        let mut g = fake();
        let unique = g.random_element_indices(5, None, Some(5), true).unwrap();
        let mut sorted = unique.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
        assert!(g.random_element_indices(2, None, Some(3), true).is_err());
        let weighted = g
            .random_element_indices(3, Some(&[0.0, 1.0, 0.0]), Some(50), false)
            .unwrap();
        assert!(weighted.iter().all(|&i| i == 1));
        let wu = g
            .random_element_indices(3, Some(&[1.0, 2.0, 3.0]), Some(3), true)
            .unwrap();
        let mut wu_sorted = wu.clone();
        wu_sorted.sort_unstable();
        assert_eq!(wu_sorted, vec![0, 1, 2]);
    }

    #[test]
    fn randomize_nb_elements_bounds() {
        let mut g = fake();
        assert_eq!(g.randomize_nb_elements(10, true, true, None, None), 10);
        for _ in 0..500 {
            let n = g.randomize_nb_elements(10, false, false, Some(7), Some(13));
            assert!((7..=13).contains(&n));
        }
    }

    #[test]
    fn locale_codes_look_right() {
        let mut g = fake();
        let locale = g.locale();
        assert!(locale.contains('_'), "{locale}");
    }
}
