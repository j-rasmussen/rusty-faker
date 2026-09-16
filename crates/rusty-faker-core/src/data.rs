//! Static data tables compiled from `data/**.json` by `build.rs`.

use rand::RngExt;

use crate::template::{Formatter, Segment, Template};

/// A table of items, optionally weighted (Faker's `OrderedDict` value→weight tables).
#[derive(Debug)]
pub struct Table<T: 'static> {
    pub items: &'static [T],
    /// Running totals of the weights; `None` means uniform.
    pub cum_weights: Option<&'static [f64]>,
}

impl<T> Table<T> {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn total_weight(&self) -> f64 {
        self.cum_weights
            .and_then(|cum| cum.last().copied())
            .unwrap_or(0.0)
    }

    /// Picks one item. Weights are honored when present and `use_weighting` is set.
    pub fn pick<R: RngExt + ?Sized>(&self, rng: &mut R, use_weighting: bool) -> &'static T {
        let items = self.items;
        match self.cum_weights {
            Some(cum) if use_weighting => {
                &items[weighted_index(cum, rng.random::<f64>() * self.total_weight())]
            }
            _ => &items[rng.random_range(0..items.len())],
        }
    }
}

/// Index of the first running total greater than `x` (Python's `bisect_right`), clamped
/// so float rounding at the top end can never index out of bounds.
fn weighted_index(cum: &[f64], x: f64) -> usize {
    cum.partition_point(|&c| c <= x)
        .min(cum.len().saturating_sub(1))
}

/// Picks from several tables as if they were concatenated (Faker's `add_ordereddicts`).
pub fn pick_concat<T, R: RngExt + ?Sized>(
    tables: &[&Table<T>],
    rng: &mut R,
    use_weighting: bool,
) -> Option<&'static T> {
    let weighted = use_weighting && tables.iter().all(|t| t.cum_weights.is_some());
    if weighted {
        let total: f64 = tables.iter().map(|t| t.total_weight()).sum();
        let mut x = rng.random::<f64>() * total;
        for table in tables {
            let weight = table.total_weight();
            if x < weight {
                let cum = table.cum_weights?;
                return Some(&table.items[weighted_index(cum, x)]);
            }
            x -= weight;
        }
        return tables.last().and_then(|t| t.items.last());
    }
    let total: usize = tables.iter().map(|t| t.len()).sum();
    if total == 0 {
        return None;
    }
    let mut index = rng.random_range(0..total);
    for table in tables {
        if index < table.len() {
            return Some(&table.items[index]);
        }
        index -= table.len();
    }
    None
}

/// A country record from Faker's `date_time` provider.
#[derive(Debug)]
pub struct Country {
    pub name: &'static str,
    pub alpha_2_code: &'static str,
    pub alpha_3_code: &'static str,
    pub continent: &'static str,
    pub capital: &'static str,
    pub timezones: &'static [&'static str],
}

include!(concat!(env!("OUT_DIR"), "/data.rs"));
