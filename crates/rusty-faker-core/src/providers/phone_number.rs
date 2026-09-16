//! Faker's `phone_number` provider.

use crate::generator::Generator;

impl Generator {
    pub fn phone_number(&mut self) -> String {
        let format = *self.pick(&self.locale_data().phone_number.formats);
        self.numerify(format)
    }

    pub fn country_calling_code(&mut self) -> &'static str {
        self.pick(&self.locale_data().phone_number.country_calling_codes)
    }

    /// Mobile subscriber number (13 digits for most locales).
    pub fn msisdn(&mut self) -> String {
        let format = *self.pick(&self.locale_data().phone_number.msisdn_formats);
        self.numerify(format)
    }

    /// 10-digit phone number without extension; falls back to `phone_number`.
    pub fn basic_phone_number(&mut self) -> String {
        let Some(formats) = &self.locale_data().phone_number.basic_formats else {
            return self.phone_number();
        };
        let template = *self.pick(formats);
        let mut rendered = String::with_capacity(16);
        self.render(template, &mut rendered);
        self.numerify(&rendered)
    }
}

#[cfg(test)]
mod tests {
    use crate::Generator;

    #[test]
    fn phone_numbers_are_numerified() {
        let mut g = Generator::seeded("en_US", 2).unwrap();
        for _ in 0..200 {
            let number = g.phone_number();
            assert!(!number.contains(['#', '$']), "{number}");
            assert_eq!(g.msisdn().len(), 13);
            let basic = g.basic_phone_number();
            assert_eq!(
                basic.bytes().filter(u8::is_ascii_digit).count(),
                10,
                "{basic}"
            );
        }
    }
}
