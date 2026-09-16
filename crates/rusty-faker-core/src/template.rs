//! Faker's `{{ token }}` templates, pre-tokenized at build time.

use crate::error::{Error, Result};
use crate::generator::Generator;

/// One piece of a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    Literal(&'static str),
    Token(Formatter),
}

/// A pre-tokenized template.
pub type Template = &'static [Segment];

macro_rules! formatters {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// Zero-argument formatters that templates can reference.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Formatter {
            $($variant),+
        }

        impl Formatter {
            /// All formatters, in declaration order.
            pub const ALL: &'static [Formatter] = &[$(Formatter::$variant),+];

            /// Faker's method name for this formatter.
            pub fn name(self) -> &'static str {
                match self {
                    $(Formatter::$variant => $name),+
                }
            }

            /// Resolves a Faker method name.
            pub fn from_name(name: &str) -> Option<Formatter> {
                match name {
                    $($name => Some(Formatter::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

formatters! {
    BuildingNumber => "building_number",
    City => "city",
    CityPrefix => "city_prefix",
    CitySuffix => "city_suffix",
    CompanySuffix => "company_suffix",
    DomainName => "domain_name",
    FirstName => "first_name",
    FirstNameFemale => "first_name_female",
    FirstNameMale => "first_name_male",
    FirstNameNonbinary => "first_name_nonbinary",
    FreeEmailDomain => "free_email_domain",
    LastName => "last_name",
    MilitaryApo => "military_apo",
    MilitaryDpo => "military_dpo",
    MilitaryShip => "military_ship",
    MilitaryState => "military_state",
    Postcode => "postcode",
    PrefixFemale => "prefix_female",
    PrefixMale => "prefix_male",
    PrefixNonbinary => "prefix_nonbinary",
    SecondaryAddress => "secondary_address",
    StateAbbr => "state_abbr",
    StreetAddress => "street_address",
    StreetName => "street_name",
    StreetSuffix => "street_suffix",
    SuffixFemale => "suffix_female",
    SuffixMale => "suffix_male",
    SuffixNonbinary => "suffix_nonbinary",
    UserName => "user_name",
}

impl Generator {
    /// Appends a rendered template to `out`.
    pub fn render(&mut self, template: Template, out: &mut String) {
        for segment in template {
            match *segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Token(formatter) => self.write_formatter(formatter, out),
            }
        }
    }

    /// Picks a template from `table` and renders it into a new string.
    pub(crate) fn render_pick(&mut self, table: &crate::data::Table<Template>) -> String {
        let template = *self.pick(table);
        let mut out = String::with_capacity(32);
        self.render(template, &mut out);
        out
    }

    /// Appends the output of a zero-argument formatter to `out`.
    pub fn write_formatter(&mut self, formatter: Formatter, out: &mut String) {
        match formatter {
            Formatter::BuildingNumber => self.write_building_number(out),
            Formatter::City => self.write_city(out),
            Formatter::CityPrefix => out.push_str(self.city_prefix()),
            Formatter::CitySuffix => out.push_str(self.city_suffix()),
            Formatter::CompanySuffix => out.push_str(self.company_suffix()),
            Formatter::DomainName => self.write_domain_name(1, out),
            Formatter::FirstName => out.push_str(self.first_name()),
            Formatter::FirstNameFemale => out.push_str(self.first_name_female()),
            Formatter::FirstNameMale => out.push_str(self.first_name_male()),
            Formatter::FirstNameNonbinary => out.push_str(self.first_name_nonbinary()),
            Formatter::FreeEmailDomain => out.push_str(self.free_email_domain()),
            Formatter::LastName => out.push_str(self.last_name()),
            Formatter::MilitaryApo => self.write_military_apo(out),
            Formatter::MilitaryDpo => self.write_military_dpo(out),
            Formatter::MilitaryShip => out.push_str(self.military_ship()),
            Formatter::MilitaryState => out.push_str(self.military_state()),
            Formatter::Postcode => self.write_postcode(out),
            Formatter::PrefixFemale => out.push_str(self.prefix_female()),
            Formatter::PrefixMale => out.push_str(self.prefix_male()),
            Formatter::PrefixNonbinary => out.push_str(self.prefix_nonbinary()),
            Formatter::SecondaryAddress => self.write_secondary_address(out),
            Formatter::StateAbbr => out.push_str(self.state_abbr(true, true)),
            Formatter::StreetAddress => self.write_street_address(out),
            Formatter::StreetName => self.write_street_name(out),
            Formatter::StreetSuffix => out.push_str(self.street_suffix()),
            Formatter::SuffixFemale => out.push_str(self.suffix_female()),
            Formatter::SuffixMale => out.push_str(self.suffix_male()),
            Formatter::SuffixNonbinary => out.push_str(self.suffix_nonbinary()),
            Formatter::UserName => self.write_user_name(out),
        }
    }

    /// Replaces `{{ token }}` occurrences in arbitrary text, like Faker's `Generator.parse`.
    /// Unmatched braces are kept verbatim; argument groups are not supported here.
    pub fn parse(&mut self, text: &str) -> Result<String> {
        let mut out = String::with_capacity(text.len() + 16);
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some((name, consumed)) = match_token(after) else {
                out.push('{');
                rest = &rest[start + 1..];
                continue;
            };
            let formatter = Formatter::from_name(name)
                .ok_or_else(|| Error::UnknownFormatter(name.to_owned()))?;
            self.write_formatter(formatter, &mut out);
            rest = &after[consumed..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// Matches `\s*(\w+)\s*\}\}` at the start of `s`, returning the name and bytes consumed.
fn match_token(s: &str) -> Option<(&str, usize)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let start = s.len() - s.trim_start().len();
    let len = s[start..]
        .find(|c: char| !is_word(c))
        .unwrap_or(s.len() - start);
    if len == 0 {
        return None;
    }
    let mut end = start + len;
    end += s[end..].len() - s[end..].trim_start().len();
    s[end..]
        .starts_with("}}")
        .then_some((&s[start..start + len], end + 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for &formatter in Formatter::ALL {
            assert_eq!(Formatter::from_name(formatter.name()), Some(formatter));
        }
    }

    #[test]
    fn parse_replaces_tokens_and_keeps_other_text() {
        let mut fake = Generator::seeded("en_US", 7).unwrap();
        let out = fake.parse("Hi {{ first_name }}! {not} {{nope").unwrap();
        assert!(
            out.starts_with("Hi ") && out.ends_with("! {not} {{nope"),
            "{out}"
        );
        assert!(!out.contains("first_name"));
        assert!(matches!(
            fake.parse("{{bogus}}"),
            Err(Error::UnknownFormatter(_))
        ));
    }
}
