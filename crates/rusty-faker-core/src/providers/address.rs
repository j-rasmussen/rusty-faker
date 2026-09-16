//! Faker's `address` provider (including the `en_US` overrides).

use std::fmt::Write as _;

use crate::data::{LocaleId, Table, pick_concat};
use crate::error::{Error, Result, invalid};
use crate::generator::Generator;

impl Generator {
    pub fn address(&mut self) -> String {
        self.render_pick(&self.locale_data().address.address_formats)
    }

    /// A state or other first-level administrative unit.
    pub fn administrative_unit(&mut self) -> Result<&'static str> {
        let states = self
            .locale_data()
            .address
            .states
            .as_ref()
            .ok_or_else(|| missing("administrative_unit"))?;
        Ok(self.pick(states))
    }

    /// Alias of [`Generator::administrative_unit`].
    pub fn state(&mut self) -> Result<&'static str> {
        self.administrative_unit()
    }

    pub fn building_number(&mut self) -> String {
        let mut out = String::with_capacity(5);
        self.write_building_number(&mut out);
        out
    }

    pub(crate) fn write_building_number(&mut self, out: &mut String) {
        let format = *self.pick(&self.locale_data().address.building_number_formats);
        self.write_numerify(format, out);
    }

    pub fn city(&mut self) -> String {
        let mut out = String::with_capacity(16);
        self.write_city(&mut out);
        out
    }

    pub(crate) fn write_city(&mut self, out: &mut String) {
        let template = *self.pick(&self.locale_data().address.city_formats);
        self.render(template, out);
    }

    /// City prefix such as `North`; empty when the locale has none.
    pub fn city_prefix(&mut self) -> &'static str {
        match &self.locale_data().address.city_prefixes {
            Some(table) => self.pick(table),
            None => "",
        }
    }

    pub fn city_suffix(&mut self) -> &'static str {
        self.pick(&self.locale_data().address.city_suffixes)
    }

    pub fn street_suffix(&mut self) -> &'static str {
        self.pick(&self.locale_data().address.street_suffixes)
    }

    pub fn country(&mut self) -> &'static str {
        self.pick(&self.locale_data().address.countries)
    }

    /// Random country code; `representation` is `alpha-2` or `alpha-3`.
    pub fn country_code(&mut self, representation: &str) -> Result<&'static str> {
        let address = &self.locale_data().address;
        match representation {
            "alpha-2" => Ok(self.pick(&address.alpha_2_country_codes)),
            "alpha-3" => Ok(self.pick(&address.alpha_3_country_codes)),
            _ => Err(invalid(
                "`representation` must be one of `alpha-2` or `alpha-3`.",
            )),
        }
    }

    /// The region part of the generator's locale code (e.g. `US`).
    pub fn current_country_code(&self) -> Result<&'static str> {
        self.locale_data().code.split('_').nth(1).ok_or_else(|| {
            Error::Unsupported("Country code cannot be determined from locale".into())
        })
    }

    /// The country name for the generator's locale.
    pub fn current_country(&self) -> Result<&'static str> {
        let code = self.current_country_code()?;
        let mut matches = self
            .locale_data()
            .date_time
            .countries
            .iter()
            .filter(|c| c.alpha_2_code == code);
        match (matches.next(), matches.next()) {
            (Some(country), None) => Ok(country.name),
            (Some(_), Some(_)) => Err(invalid(format!(
                "Ambiguous country for country code {code}"
            ))),
            (None, _) => Err(invalid(format!(
                "No appropriate country for country code {code}"
            ))),
        }
    }

    pub fn military_apo(&mut self) -> String {
        let mut out = String::with_capacity(18);
        self.write_military_apo(&mut out);
        out
    }

    pub(crate) fn write_military_apo(&mut self, out: &mut String) {
        if let Some(format) = self.locale_data().address.military_apo_format {
            self.write_numerify(format, out);
        }
    }

    pub fn military_dpo(&mut self) -> String {
        let mut out = String::with_capacity(18);
        self.write_military_dpo(&mut out);
        out
    }

    pub(crate) fn write_military_dpo(&mut self, out: &mut String) {
        if let Some(format) = self.locale_data().address.military_dpo_format {
            self.write_numerify(format, out);
        }
    }

    pub fn military_ship(&mut self) -> &'static str {
        match &self.locale_data().address.military_ship_prefix {
            Some(table) => self.pick(table),
            None => "",
        }
    }

    pub fn military_state(&mut self) -> &'static str {
        match &self.locale_data().address.military_state_abbr {
            Some(table) => self.pick(table),
            None => "",
        }
    }

    /// Postal code. `en_US` draws a 5-digit ZIP; other locales bothify `postcode_formats`.
    pub fn postcode(&mut self) -> String {
        let mut out = String::with_capacity(10);
        self.write_postcode(&mut out);
        out
    }

    pub(crate) fn write_postcode(&mut self, out: &mut String) {
        if matches!(self.locale_data().id, LocaleId::EnUs) {
            let _ = write!(out, "{:05}", self.randint(501, 99950));
            return;
        }
        let format = *self.pick(&self.locale_data().address.postcode_formats);
        let mut raw = String::with_capacity(format.len());
        self.write_bothify_default(format, &mut raw);
        out.push_str(&raw.to_uppercase());
    }

    /// Postal code inside a state (random state when `None`).
    pub fn postcode_in_state(&mut self, state_abbr: Option<&str>) -> Result<String> {
        let address = &self.locale_data().address;
        let ranges = address
            .states_postcode
            .ok_or_else(|| missing("postcode_in_state"))?;
        let state = match state_abbr {
            Some(state) => state,
            None => self.pick(
                address
                    .states_abbr
                    .as_ref()
                    .ok_or_else(|| missing("postcode_in_state"))?,
            ),
        };
        let known = [
            &address.states_abbr,
            &address.territories_abbr,
            &address.freely_associated_states_abbr,
        ]
        .into_iter()
        .flatten()
        .any(|table| table.items.contains(&state));
        if !known {
            return Err(Error::StateNotFound);
        }
        let &(_, lo, hi) = ranges
            .iter()
            .find(|(abbr, _, _)| *abbr == state)
            .ok_or(Error::StateNotFound)?;
        Ok(format!("{:05}", self.randint(i64::from(lo), i64::from(hi))))
    }

    /// ZIP+4 code such as `12345-6789`.
    pub fn zipcode_plus4(&mut self) -> String {
        let mut out = self.postcode();
        let _ = write!(out, "-{:04}", self.randint(1, 9999));
        out
    }

    pub fn secondary_address(&mut self) -> String {
        let mut out = String::with_capacity(10);
        self.write_secondary_address(&mut out);
        out
    }

    pub(crate) fn write_secondary_address(&mut self, out: &mut String) {
        if let Some(table) = &self.locale_data().address.secondary_address_formats {
            let format = *self.pick(table);
            self.write_numerify(format, out);
        }
    }

    /// Two-letter state code, optionally including territories and freely associated states.
    pub fn state_abbr(
        &mut self,
        include_territories: bool,
        include_freely_associated_states: bool,
    ) -> &'static str {
        let address = &self.locale_data().address;
        let Some(states) = &address.states_abbr else {
            return "";
        };
        let mut tables: Vec<&Table<&'static str>> = Vec::with_capacity(3);
        tables.push(states);
        if include_territories && let Some(t) = &address.territories_abbr {
            tables.push(t);
        }
        if include_freely_associated_states && let Some(t) = &address.freely_associated_states_abbr
        {
            tables.push(t);
        }
        let use_weighting = self.use_weighting();
        pick_concat(&tables, self.rng(), use_weighting)
            .copied()
            .unwrap_or("")
    }

    pub fn street_address(&mut self) -> String {
        let mut out = String::with_capacity(32);
        self.write_street_address(&mut out);
        out
    }

    pub(crate) fn write_street_address(&mut self, out: &mut String) {
        let template = *self.pick(&self.locale_data().address.street_address_formats);
        self.render(template, out);
    }

    pub fn street_name(&mut self) -> String {
        let mut out = String::with_capacity(24);
        self.write_street_name(&mut out);
        out
    }

    pub(crate) fn write_street_name(&mut self, out: &mut String) {
        let template = *self.pick(&self.locale_data().address.street_name_formats);
        self.render(template, out);
    }
}

fn missing(method: &str) -> Error {
    Error::Unsupported(format!("{method} is not available for this locale"))
}

#[cfg(test)]
mod tests {
    use crate::{Error, Generator};

    #[test]
    fn address_parts() {
        let mut g = Generator::seeded("en_US", 5).unwrap();
        for _ in 0..500 {
            let address = g.address();
            assert!(
                address.contains('\n') && !address.contains("{{"),
                "{address}"
            );
            let zip = g.postcode();
            assert_eq!(zip.len(), 5);
            assert!(zip.bytes().all(|b| b.is_ascii_digit()));
            let plus4 = g.zipcode_plus4();
            assert_eq!(plus4.len(), 10, "{plus4}");
            assert_eq!(g.state_abbr(false, false).len(), 2);
        }
        assert_eq!(g.current_country_code().unwrap(), "US");
        assert_eq!(g.current_country().unwrap(), "United States");
    }

    #[test]
    fn postcode_in_state_uses_ranges() {
        let mut g = Generator::seeded("en_US", 5).unwrap();
        for _ in 0..200 {
            let zip: u32 = g.postcode_in_state(Some("CT")).unwrap().parse().unwrap();
            assert!((6001..=6389).contains(&zip));
        }
        assert_eq!(g.postcode_in_state(Some("PR")).unwrap().len(), 5);
        assert_eq!(g.postcode_in_state(Some("XX")), Err(Error::StateNotFound));
        assert!(g.country_code("alpha-4").is_err());
        assert_eq!(g.country_code("alpha-3").unwrap().len(), 3);
    }
}
