//! Faker's `person` provider.

use crate::data::{Table, pick_concat};
use crate::generator::Generator;

impl Generator {
    pub fn name(&mut self) -> String {
        let person = &self.locale_data().person;
        self.render_pick(&person.formats)
    }

    pub fn name_female(&mut self) -> String {
        let person = &self.locale_data().person;
        self.render_pick(person.formats_female.as_ref().unwrap_or(&person.formats))
    }

    pub fn name_male(&mut self) -> String {
        let person = &self.locale_data().person;
        self.render_pick(person.formats_male.as_ref().unwrap_or(&person.formats))
    }

    pub fn name_nonbinary(&mut self) -> String {
        let person = &self.locale_data().person;
        self.render_pick(person.formats_nonbinary.as_ref().unwrap_or(&person.formats))
    }

    pub fn first_name(&mut self) -> &'static str {
        self.pick(&self.locale_data().person.first_names)
    }

    pub fn first_name_female(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .first_names_female
                .as_ref()
                .unwrap_or(&person.first_names),
        )
    }

    pub fn first_name_male(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .first_names_male
                .as_ref()
                .unwrap_or(&person.first_names),
        )
    }

    pub fn first_name_nonbinary(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .first_names_nonbinary
                .as_ref()
                .unwrap_or(&person.first_names),
        )
    }

    pub fn last_name(&mut self) -> &'static str {
        self.pick(&self.locale_data().person.last_names)
    }

    pub fn last_name_female(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .last_names_female
                .as_ref()
                .unwrap_or(&person.last_names),
        )
    }

    pub fn last_name_male(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .last_names_male
                .as_ref()
                .unwrap_or(&person.last_names),
        )
    }

    pub fn last_name_nonbinary(&mut self) -> &'static str {
        let person = &self.locale_data().person;
        self.pick(
            person
                .last_names_nonbinary
                .as_ref()
                .unwrap_or(&person.last_names),
        )
    }

    /// Name prefix such as `Dr.`; empty when the locale has none.
    pub fn prefix(&mut self) -> &'static str {
        let p = &self.locale_data().person;
        self.pick_gendered(
            p.prefixes.as_ref(),
            p.prefixes_male.as_ref(),
            p.prefixes_female.as_ref(),
            p.prefixes_nonbinary.as_ref(),
        )
    }

    pub fn prefix_female(&mut self) -> &'static str {
        match &self.locale_data().person.prefixes_female {
            Some(table) => self.pick(table),
            None => self.prefix(),
        }
    }

    pub fn prefix_male(&mut self) -> &'static str {
        match &self.locale_data().person.prefixes_male {
            Some(table) => self.pick(table),
            None => self.prefix(),
        }
    }

    pub fn prefix_nonbinary(&mut self) -> &'static str {
        match &self.locale_data().person.prefixes_nonbinary {
            Some(table) => self.pick(table),
            None => self.prefix(),
        }
    }

    /// Name suffix such as `PhD`; empty when the locale has none.
    pub fn suffix(&mut self) -> &'static str {
        let p = &self.locale_data().person;
        self.pick_gendered(
            p.suffixes.as_ref(),
            p.suffixes_male.as_ref(),
            p.suffixes_female.as_ref(),
            p.suffixes_nonbinary.as_ref(),
        )
    }

    pub fn suffix_female(&mut self) -> &'static str {
        match &self.locale_data().person.suffixes_female {
            Some(table) => self.pick(table),
            None => self.suffix(),
        }
    }

    pub fn suffix_male(&mut self) -> &'static str {
        match &self.locale_data().person.suffixes_male {
            Some(table) => self.pick(table),
            None => self.suffix(),
        }
    }

    pub fn suffix_nonbinary(&mut self) -> &'static str {
        match &self.locale_data().person.suffixes_nonbinary {
            Some(table) => self.pick(table),
            None => self.suffix(),
        }
    }

    pub fn language_name(&mut self) -> &'static str {
        self.pick(&self.locale_data().person.language_names)
    }

    /// Faker's prefix/suffix fallback: a combined table, else all gendered tables merged,
    /// else male or female chosen at random, else empty.
    fn pick_gendered(
        &mut self,
        combined: Option<&'static Table<&'static str>>,
        male: Option<&'static Table<&'static str>>,
        female: Option<&'static Table<&'static str>>,
        nonbinary: Option<&'static Table<&'static str>>,
    ) -> &'static str {
        if let Some(table) = combined {
            return self.pick(table);
        }
        let use_weighting = self.use_weighting();
        match (male, female, nonbinary) {
            (Some(m), Some(f), Some(n)) => pick_concat(&[m, f, n], self.rng(), use_weighting)
                .copied()
                .unwrap_or(""),
            (Some(m), Some(f), None) => {
                let table = if self.index(2) == 0 { m } else { f };
                self.pick(table)
            }
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Generator;

    #[test]
    fn names_are_rendered() {
        let mut g = Generator::seeded("en_US", 3).unwrap();
        for _ in 0..500 {
            let name = g.name();
            assert!(!name.contains("{{") && name.contains(' '), "{name}");
            assert!(!g.name_female().is_empty());
            assert!(!g.prefix().is_empty());
            assert!(!g.suffix().is_empty());
        }
    }

    #[test]
    fn weighted_picks_track_weights() {
        // prefixes_male: Mr. 0.7, Dr. 0.3
        let mut g = Generator::seeded("en_US", 11).unwrap();
        let n = 20_000;
        let mr = (0..n).filter(|_| g.prefix_male() == "Mr.").count();
        let ratio = mr as f64 / n as f64;
        assert!((0.67..0.73).contains(&ratio), "{ratio}");
        g.set_use_weighting(false);
        let mr = (0..n).filter(|_| g.prefix_male() == "Mr.").count();
        let ratio = mr as f64 / n as f64;
        assert!((0.47..0.53).contains(&ratio), "{ratio}");
    }
}
