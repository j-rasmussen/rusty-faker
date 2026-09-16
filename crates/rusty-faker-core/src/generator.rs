//! The generator: an RNG bound to one locale's data.

use rand::{RngExt, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::data::{Locale, Table, find_locale};
use crate::error::{Error, Result};

/// Generates fake data for one locale.
///
/// Cheap to create; the locale data is static. Not thread-safe by itself (`&mut self`),
/// matching Faker, whose generators are not thread-safe either.
#[derive(Debug, Clone)]
pub struct Generator {
    rng: Pcg64Mcg,
    locale: &'static Locale,
    use_weighting: bool,
}

impl Generator {
    /// Creates a generator seeded from OS entropy.
    pub fn new(locale: &str) -> Result<Self> {
        let locale = find_locale(locale).ok_or_else(|| Error::UnknownLocale(locale.to_owned()))?;
        Ok(Self::for_locale(locale))
    }

    /// Creates a deterministic generator.
    pub fn seeded(locale: &str, seed: u64) -> Result<Self> {
        let mut generator = Self::new(locale)?;
        generator.seed(seed);
        Ok(generator)
    }

    /// Creates a generator for already-resolved locale data, seeded from OS entropy.
    pub fn for_locale(locale: &'static Locale) -> Self {
        Self {
            rng: rand::make_rng(),
            locale,
            use_weighting: true,
        }
    }

    /// Reseeds deterministically.
    pub fn seed(&mut self, seed: u64) {
        self.rng = Pcg64Mcg::seed_from_u64(seed);
    }

    /// Reseeds from OS entropy.
    pub fn reseed_from_entropy(&mut self) {
        self.rng = rand::make_rng();
    }

    /// The locale data this generator draws from.
    pub fn locale_data(&self) -> &'static Locale {
        self.locale
    }

    /// Whether weighted tables honor their weights (Faker's `use_weighting`, default `true`).
    pub fn use_weighting(&self) -> bool {
        self.use_weighting
    }

    pub fn set_use_weighting(&mut self, use_weighting: bool) {
        self.use_weighting = use_weighting;
    }

    /// Direct access to the RNG for callers composing their own values.
    pub fn rng(&mut self) -> &mut impl RngExt {
        &mut self.rng
    }

    pub(crate) fn pick<T>(&mut self, table: &Table<T>) -> &'static T {
        table.pick(&mut self.rng, self.use_weighting)
    }

    /// Uniform index in `0..len`. `len` must be non-zero.
    pub(crate) fn index(&mut self, len: usize) -> usize {
        self.rng.random_range(0..len)
    }

    /// Uniform integer in `lo..=hi`; returns `lo` when the range is empty.
    pub(crate) fn randint(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        self.rng.random_range(lo..=hi)
    }

    /// Uniform float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        self.rng.random::<f64>()
    }

    /// Uniform float between `a` and `b` (Python's `random.uniform`).
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_locale_is_an_error() {
        assert_eq!(
            Generator::new("xx_XX").unwrap_err(),
            Error::UnknownLocale("xx_XX".into())
        );
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Generator::seeded("en_US", 99).unwrap();
        let mut b = Generator::seeded("en_US", 99).unwrap();
        let run = |g: &mut Generator| {
            (0..50)
                .map(|_| format!("{}|{}|{}", g.name(), g.address(), g.email(true, None)))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(&mut a), run(&mut b));
        let mut c = Generator::seeded("en_US", 100).unwrap();
        assert_ne!(run(&mut a), run(&mut c));
    }
}
