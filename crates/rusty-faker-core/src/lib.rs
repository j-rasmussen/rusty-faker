//! Fake data generation with Python Faker's data compiled into static tables.
//!
//! ```
//! let mut fake = rusty_faker_core::Generator::seeded("en_US", 42)?;
//! let name = fake.name();
//! assert!(!name.is_empty());
//! # Ok::<(), rusty_faker_core::Error>(())
//! ```

pub mod data;
pub mod error;
pub mod generator;
mod providers;
pub mod template;
pub mod text;

pub use data::{LOCALE_CODES, Locale, LocaleId, find_locale};
pub use error::{Error, Result};
pub use generator::Generator;
pub use providers::date_time::{Clock, DateTimeValue, now_timestamp, timestamp_to_naive};
pub use template::Formatter;
