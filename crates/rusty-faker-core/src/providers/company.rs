//! Faker's `company` provider.

use crate::data::Table;
use crate::generator::Generator;

impl Generator {
    pub fn company(&mut self) -> String {
        self.render_pick(&self.locale_data().company.formats)
    }

    pub fn company_suffix(&mut self) -> &'static str {
        self.pick(&self.locale_data().company.company_suffixes)
    }

    /// Phrase such as `Robust full-range hub`.
    pub fn catch_phrase(&mut self) -> String {
        self.join_groups(self.locale_data().company.catch_phrase_words)
    }

    /// Phrase such as `integrate extensible convergence`.
    pub fn bs(&mut self) -> String {
        self.join_groups(self.locale_data().company.bs_words)
    }

    fn join_groups(&mut self, groups: &'static [Table<&'static str>]) -> String {
        let mut out = String::with_capacity(40);
        for (i, group) in groups.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(self.pick(group));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::Generator;

    #[test]
    fn company_values() {
        let mut g = Generator::seeded("en_US", 8).unwrap();
        for _ in 0..200 {
            assert!(!g.company().contains("{{"));
            assert!(g.catch_phrase().split(' ').count() >= 3);
            assert!(!g.bs().is_empty());
        }
    }
}
