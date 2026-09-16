//! Faker's `internet` provider.

use std::fmt::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::OnceLock;

use crate::error::{Error, Result, invalid};
use crate::generator::Generator;
use crate::text::{slugify, to_ascii};

/// An IPv4 network as (address, prefix length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Net {
    addr: u32,
    prefix: u8,
}

impl Net {
    const fn new(a: u8, b: u8, c: u8, d: u8, prefix: u8) -> Self {
        Self {
            addr: u32::from_be_bytes([a, b, c, d]),
            prefix,
        }
    }

    fn size(self) -> u64 {
        1u64 << (32 - u32::from(self.prefix))
    }

    fn last(self) -> u32 {
        // size() ≤ 2^32, so this stays within u32.
        self.addr.wrapping_add((self.size() - 1) as u32)
    }

    fn contains(self, other: Net) -> bool {
        other.prefix >= self.prefix && other.addr >= self.addr && other.last() <= self.last()
    }

    fn overlaps(self, other: Net) -> bool {
        self.contains(other) || other.contains(self)
    }

    /// The two halves of this network (only valid for prefix < 32).
    fn split(self) -> (Net, Net) {
        let prefix = self.prefix + 1;
        let low = Net {
            addr: self.addr,
            prefix,
        };
        let high = Net {
            addr: self.addr | (1u32 << (32 - u32::from(prefix))),
            prefix,
        };
        (low, high)
    }

    /// Python's `address_exclude`: this network minus `other` (which must be inside it).
    fn exclude(self, other: Net) -> Vec<Net> {
        let mut out = Vec::new();
        let mut current = self;
        while current != other {
            let (low, high) = current.split();
            if low.contains(other) {
                out.push(high);
                current = low;
            } else {
                out.push(low);
                current = high;
            }
        }
        out
    }
}

const NETWORK_CLASSES: [(char, Net); 3] = [
    ('a', Net::new(0, 0, 0, 0, 1)),
    ('b', Net::new(128, 0, 0, 0, 2)),
    ('c', Net::new(192, 0, 0, 0, 3)),
];
const PRIVATE_NETWORKS: [Net; 3] = [
    Net::new(10, 0, 0, 0, 8),
    Net::new(172, 16, 0, 0, 12),
    Net::new(192, 168, 0, 0, 16),
];
const EXCLUDED_NETWORKS: [Net; 16] = [
    Net::new(0, 0, 0, 0, 8),
    Net::new(100, 64, 0, 0, 10),
    Net::new(127, 0, 0, 0, 8),
    Net::new(169, 254, 0, 0, 16),
    Net::new(192, 0, 0, 0, 24),
    Net::new(192, 0, 2, 0, 24),
    Net::new(192, 31, 196, 0, 24),
    Net::new(192, 52, 193, 0, 24),
    Net::new(192, 88, 99, 0, 24),
    Net::new(192, 175, 48, 0, 24),
    Net::new(198, 18, 0, 0, 15),
    Net::new(198, 51, 100, 0, 24),
    Net::new(203, 0, 113, 0, 24),
    Net::new(224, 0, 0, 0, 4),
    Net::new(240, 0, 0, 0, 4),
    Net::new(255, 255, 255, 255, 32),
];

/// Faker's `_exclude_ipv4_networks`.
fn exclude_networks(mut networks: Vec<Net>, to_exclude: &[Net]) -> Vec<Net> {
    let mut to_exclude = to_exclude.to_vec();
    to_exclude.sort_by_key(|n| n.prefix);
    for excluded in to_exclude {
        networks = networks
            .into_iter()
            .flat_map(|net| {
                if net.contains(excluded) {
                    net.exclude(excluded)
                } else if net.overlaps(excluded) {
                    Vec::new()
                } else {
                    vec![net]
                }
            })
            .collect();
    }
    networks
}

/// Networks plus cumulative address counts, so one draw picks a uniformly random address.
#[derive(Debug)]
struct Pool {
    nets: Vec<Net>,
    cum: Vec<u64>,
}

impl Pool {
    fn new(nets: Vec<Net>) -> Self {
        let mut total = 0;
        let cum = nets
            .iter()
            .map(|n| {
                total += n.size();
                total
            })
            .collect();
        Self { nets, cum }
    }

    fn total(&self) -> u64 {
        self.cum.last().copied().unwrap_or(0)
    }
}

#[derive(Clone, Copy)]
enum PoolKind {
    All,
    Private,
    Public,
}

/// Index 0 = no class, 1–3 = classes a–c.
fn pool(kind: PoolKind, class: usize) -> &'static Pool {
    static POOLS: [[OnceLock<Pool>; 4]; 3] = [const { [const { OnceLock::new() }; 4] }; 3];
    let kind_index = kind as usize;
    POOLS[kind_index][class].get_or_init(|| {
        let class_net = class.checked_sub(1).map(|i| NETWORK_CLASSES[i].1);
        let nets = match kind {
            PoolKind::All => exclude_networks(
                vec![class_net.unwrap_or(Net::new(0, 0, 0, 0, 0))],
                &EXCLUDED_NETWORKS,
            ),
            PoolKind::Private => {
                let supernet = class_net.unwrap_or(NETWORK_CLASSES[0].1);
                let private = PRIVATE_NETWORKS
                    .iter()
                    .copied()
                    .filter(|n| n.overlaps(supernet))
                    .collect();
                exclude_networks(private, &EXCLUDED_NETWORKS)
            }
            PoolKind::Public => {
                let supernet = class_net.unwrap_or(NETWORK_CLASSES[0].1);
                let excluded: Vec<Net> = PRIVATE_NETWORKS
                    .iter()
                    .chain(EXCLUDED_NETWORKS.iter())
                    .copied()
                    .collect();
                exclude_networks(vec![supernet], &excluded)
            }
        };
        Pool::new(nets)
    })
}

fn class_index(address_class: Option<&str>) -> Option<usize> {
    let class = address_class?;
    NETWORK_CLASSES
        .iter()
        .position(|(name, _)| class.len() == 1 && class.starts_with(*name))
        .map(|i| i + 1)
}

impl Generator {
    pub fn email(&mut self, safe: bool, domain: Option<&str>) -> String {
        let mut out = String::with_capacity(32);
        self.write_user_name(&mut out);
        out.push('@');
        match (domain.filter(|d| !d.is_empty()), safe) {
            (Some(domain), _) => out.push_str(domain),
            (None, true) => out.push_str(self.safe_domain_name()),
            (None, false) => {
                out.clear();
                let template = *self.pick(&self.locale_data().internet.email_formats);
                let mut rendered = String::with_capacity(32);
                self.render(template, &mut rendered);
                out.extend(rendered.chars().filter(|&c| c != ' '));
            }
        }
        out.to_lowercase()
    }

    pub fn safe_domain_name(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.safe_domain_names)
    }

    pub fn safe_email(&mut self) -> String {
        self.email(true, None)
    }

    pub fn free_email(&mut self) -> String {
        let mut out = self.user_name();
        out.push('@');
        out.push_str(self.free_email_domain());
        out.to_lowercase()
    }

    pub fn company_email(&mut self) -> String {
        let mut out = self.user_name();
        out.push('@');
        self.write_domain_name(1, &mut out);
        out.to_lowercase()
    }

    pub fn free_email_domain(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.free_email_domains)
    }

    pub fn ascii_email(&mut self) -> String {
        to_ascii(&self.email(false, None)).to_lowercase()
    }

    pub fn ascii_safe_email(&mut self) -> String {
        to_ascii(&self.safe_email()).to_lowercase()
    }

    pub fn ascii_free_email(&mut self) -> String {
        to_ascii(&self.free_email()).to_lowercase()
    }

    pub fn ascii_company_email(&mut self) -> String {
        to_ascii(&self.company_email()).to_lowercase()
    }

    pub fn user_name(&mut self) -> String {
        let mut out = String::with_capacity(20);
        self.write_user_name(&mut out);
        out
    }

    pub(crate) fn write_user_name(&mut self, out: &mut String) {
        let template = *self.pick(&self.locale_data().internet.user_name_formats);
        let mut rendered = String::with_capacity(24);
        self.render(template, &mut rendered);
        let mut filled = String::with_capacity(rendered.len());
        self.write_bothify_default(&rendered, &mut filled);
        let lowered = filled.to_lowercase();
        out.push_str(&slugify(&to_ascii(&lowered), true));
    }

    /// Host name such as `db-01.nichols-phillips.com`.
    pub fn hostname(&mut self, levels: i64) -> String {
        let mut out = String::with_capacity(32);
        out.push_str(self.pick(&self.locale_data().internet.hostname_prefixes));
        out.push('-');
        self.write_numerify("##", &mut out);
        if levels >= 1 {
            out.push('.');
            self.write_domain_name(u32::try_from(levels).unwrap_or(u32::MAX), &mut out);
        }
        out.to_lowercase()
    }

    /// Domain name with `levels` labels before the TLD (at least 1).
    pub fn domain_name(&mut self, levels: i64) -> Result<String> {
        if levels < 1 {
            return Err(invalid("levels must be greater than or equal to 1"));
        }
        let mut out = String::with_capacity(24);
        self.write_domain_name(u32::try_from(levels).unwrap_or(u32::MAX), &mut out);
        Ok(out)
    }

    pub(crate) fn write_domain_name(&mut self, levels: u32, out: &mut String) {
        for _ in 1..levels.max(1) {
            out.push_str(&self.domain_word());
            out.push('.');
        }
        out.push_str(&self.domain_word());
        out.push('.');
        out.push_str(&self.tld().to_lowercase());
    }

    /// First word of a company name, slugified (e.g. `nichols-phillips`).
    pub fn domain_word(&mut self) -> String {
        let company = self.company();
        let first = company.split(' ').next().unwrap_or("");
        slugify(&to_ascii(first), true).to_lowercase()
    }

    /// Domain generation algorithm output for the given (or random) date and TLD.
    pub fn dga(
        &mut self,
        year: Option<i64>,
        month: Option<i64>,
        day: Option<i64>,
        tld: Option<&str>,
        length: Option<i64>,
    ) -> Result<String> {
        let mut year = match year.filter(|&y| y != 0) {
            Some(y) => y,
            None => self.random_int(1, 9999, 1)?,
        };
        let mut month = match month.filter(|&m| m != 0) {
            Some(m) => m,
            None => self.random_int(1, 12, 1)?,
        };
        let mut day = match day.filter(|&d| d != 0) {
            Some(d) => d,
            None => self.random_int(1, 30, 1)?,
        };
        let tld = match tld.filter(|t| !t.is_empty()) {
            Some(tld) => tld.to_owned(),
            None => self.tld().to_owned(),
        };
        let length = match length.filter(|&l| l != 0) {
            Some(l) => l,
            None => self.random_int(2, 63, 1)?,
        };
        let mut domain =
            String::with_capacity(usize::try_from(length).unwrap_or(0) + tld.len() + 1);
        for _ in 0..length.max(0) {
            year = ((year ^ year.wrapping_mul(8)) >> 11) ^ ((year & 0xFFFF_FFF0).wrapping_shl(17));
            month =
                ((month ^ month.wrapping_mul(4)) >> 25) ^ (month & 0xFFFF_FFF8).wrapping_mul(16);
            day = ((day ^ day.wrapping_shl(13)) >> 19) ^ ((day & 0xFFFF_FFFE).wrapping_shl(12));
            let letter = (year ^ month ^ day).rem_euclid(25);
            domain.push(char::from(b'a' + u8::try_from(letter).unwrap_or(0)));
        }
        domain.push('.');
        domain.push_str(&tld);
        Ok(domain)
    }

    pub fn tld(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.tlds)
    }

    pub fn http_method(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.http_methods)
    }

    /// HTTP status code; any 100–599 when `include_unassigned`, else an assigned one.
    pub fn http_status_code(&mut self, include_unassigned: bool) -> u32 {
        if include_unassigned {
            return u32::try_from(self.randint(100, 599)).unwrap_or(200);
        }
        let codes = self.locale_data().internet.http_assigned_codes;
        codes[self.index(codes.len())]
    }

    /// URL such as `https://www.example.com/`. `schemes` defaults to http/https;
    /// an empty list gives a scheme-less `://host/` URL.
    pub fn url(&mut self, schemes: Option<&[&str]>) -> String {
        let mut out = String::with_capacity(40);
        self.write_url_base(schemes, &mut out);
        out
    }

    fn write_url_base(&mut self, schemes: Option<&[&str]>, out: &mut String) {
        let schemes = schemes.unwrap_or(&["http", "https"]);
        if !schemes.is_empty() {
            out.push_str(schemes[self.index(schemes.len())]);
        }
        out.push_str("://");
        let template = *self.pick(&self.locale_data().internet.url_formats);
        self.render(template, out);
    }

    pub fn ipv4_network_class(&mut self) -> char {
        NETWORK_CLASSES[self.index(3)].0
    }

    /// Random IPv4 address or network. `private`: `Some(true)` private only,
    /// `Some(false)` public only, `None` any non-reserved address.
    pub fn ipv4(
        &mut self,
        network: bool,
        address_class: Option<&str>,
        private: Option<bool>,
    ) -> String {
        match private {
            Some(true) => self.ipv4_private(network, address_class),
            Some(false) => self.ipv4_public(network, address_class),
            None => {
                let pool = pool(PoolKind::All, class_index(address_class).unwrap_or(0));
                self.address_from_pool(pool, network)
            }
        }
    }

    pub fn ipv4_private(&mut self, network: bool, address_class: Option<&str>) -> String {
        let class = self.class_or_random(address_class);
        self.address_from_pool(pool(PoolKind::Private, class), network)
    }

    pub fn ipv4_public(&mut self, network: bool, address_class: Option<&str>) -> String {
        let class = self.class_or_random(address_class);
        self.address_from_pool(pool(PoolKind::Public, class), network)
    }

    fn class_or_random(&mut self, address_class: Option<&str>) -> usize {
        class_index(address_class).unwrap_or_else(|| 1 + self.index(3))
    }

    fn address_from_pool(&mut self, pool: &Pool, network: bool) -> String {
        use rand::RngExt as _;
        let total = pool.total();
        if total == 0 {
            return String::from("0.0.0.0");
        }
        let draw = self.rng().random_range(0..total);
        let i = pool
            .cum
            .partition_point(|&c| c <= draw)
            .min(pool.nets.len() - 1);
        let net = pool.nets[i];
        let start = if i == 0 { 0 } else { pool.cum[i - 1] };
        let addr = net
            .addr
            .wrapping_add(u32::try_from(draw - start).unwrap_or(0));
        if !network {
            return Ipv4Addr::from(addr).to_string();
        }
        let prefix = u32::try_from(self.randint(i64::from(net.prefix), 32)).unwrap_or(32);
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        format!("{}/{prefix}", Ipv4Addr::from(addr & mask))
    }

    /// Random IPv6 address (above the IPv4 range) or network.
    pub fn ipv6(&mut self, network: bool) -> String {
        use rand::RngExt as _;
        let addr: u128 = self.rng().random_range((1u128 << 32)..=u128::MAX);
        if !network {
            return Ipv6Addr::from(addr).to_string();
        }
        let prefix = u32::try_from(self.randint(0, 128)).unwrap_or(128);
        let mask = if prefix == 0 {
            0
        } else {
            u128::MAX << (128 - prefix)
        };
        format!("{}/{prefix}", Ipv6Addr::from(addr & mask))
    }

    /// MAC address; the first octet is odd (multicast) or even (unicast).
    pub fn mac_address(&mut self, multicast: bool) -> String {
        let first = if multicast {
            1 + 2 * self.randint(0, 126)
        } else {
            2 * self.randint(0, 126)
        };
        let mut out = format!("{first:02x}");
        for _ in 0..5 {
            let _ = write!(out, ":{:02x}", self.randint(0, 255));
        }
        out
    }

    /// Port number, optionally restricted to the system, user or dynamic range.
    pub fn port_number(&mut self, is_system: bool, is_user: bool, is_dynamic: bool) -> u16 {
        let (lo, hi) = match (is_system, is_user, is_dynamic) {
            (true, _, _) => (0, 1023),
            (_, true, _) => (1024, 49151),
            (_, _, true) => (49152, 65535),
            _ => (0, 65535),
        };
        u16::try_from(self.randint(lo, hi)).unwrap_or(0)
    }

    pub fn uri_page(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.uri_pages)
    }

    /// Path of `deep` segments (random 1–3 when `None` or 0).
    pub fn uri_path(&mut self, deep: Option<i64>) -> String {
        let deep = match deep.filter(|&d| d != 0) {
            Some(d) => d,
            None => self.randint(1, 3),
        };
        let paths = &self.locale_data().internet.uri_paths;
        let mut out = String::with_capacity(24);
        for i in 0..deep.max(0) {
            if i > 0 {
                out.push('/');
            }
            out.push_str(paths.items[self.index(paths.len())]);
        }
        out
    }

    pub fn uri_extension(&mut self) -> &'static str {
        self.pick(&self.locale_data().internet.uri_extensions)
    }

    pub fn uri(&mut self, schemes: Option<&[&str]>, deep: Option<i64>) -> String {
        let mut out = String::with_capacity(64);
        self.write_url_base(schemes, &mut out);
        out.push_str(&self.uri_path(deep));
        out.push_str(self.uri_page());
        out.push_str(self.uri_extension());
        out
    }

    /// Slug of `value`, or of random text when `None`.
    pub fn slug(&mut self, value: Option<&str>) -> Result<String> {
        match value {
            Some(value) => Ok(slugify(value, false)),
            None => {
                let words = self.locale_data().lorem.word_list.items;
                Ok(slugify(&self.text(20, Some(words))?, false))
            }
        }
    }

    /// Placeholder image URL; `{width}` and `{height}` in `placeholder_url` are filled in.
    pub fn image_url(
        &mut self,
        width: Option<i64>,
        height: Option<i64>,
        placeholder_url: Option<&str>,
    ) -> String {
        let width = width
            .filter(|&w| w != 0)
            .unwrap_or_else(|| self.randint(0, 1024));
        let height = height
            .filter(|&h| h != 0)
            .unwrap_or_else(|| self.randint(0, 1024));
        let template = match placeholder_url {
            Some(url) => url,
            None => self.pick(&self.locale_data().internet.image_placeholder_services),
        };
        template
            .replace("{width}", &width.to_string())
            .replace("{height}", &height.to_string())
    }

    pub fn iana_id(&mut self) -> String {
        self.randint(1, 8_888_888).to_string()
    }

    pub fn ripe_id(&mut self) -> String {
        let letters = "?".repeat(usize::try_from(self.randint(2, 4)).unwrap_or(2));
        let digits = "%".repeat(usize::try_from(self.randint(1, 5)).unwrap_or(1));
        let mut out = String::with_capacity(16);
        self.write_bothify_default(&format!("ORG-{letters}{digits}-RIPE"), &mut out);
        out.to_uppercase()
    }

    /// NIC handle such as `ABC123-FAKE`; `suffix` must have at least 2 characters.
    pub fn nic_handle(&mut self, suffix: &str) -> Result<String> {
        if suffix.chars().count() < 2 {
            return Err(invalid("suffix length must be greater than or equal to 2"));
        }
        let letters = "?".repeat(usize::try_from(self.randint(2, 4)).unwrap_or(2));
        let digits = "%".repeat(usize::try_from(self.randint(1, 5)).unwrap_or(1));
        let mut out = String::with_capacity(16);
        self.write_bothify_default(&format!("{letters}{digits}-{suffix}"), &mut out);
        Ok(out.to_uppercase())
    }

    pub fn nic_handles(&mut self, count: i64, suffix: &str) -> Result<Vec<String>> {
        (0..count.max(0)).map(|_| self.nic_handle(suffix)).collect()
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(err: std::net::AddrParseError) -> Self {
        invalid(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_net(s: &str) -> Net {
        let (addr, prefix) = s.split_once('/').unwrap();
        Net {
            addr: u32::from(addr.parse::<Ipv4Addr>().unwrap()),
            prefix: prefix.parse().unwrap(),
        }
    }

    fn in_any(addr: u32, nets: &[Net]) -> bool {
        nets.iter().any(|n| addr >= n.addr && addr <= n.last())
    }

    #[test]
    fn exclusion_matches_python_address_exclude() {
        // ipaddress.ip_network('10.0.0.0/8').address_exclude(ip_network('10.1.0.0/16'))
        let out = parse_net("10.0.0.0/8").exclude(parse_net("10.1.0.0/16"));
        assert_eq!(out.len(), 8);
        assert_eq!(
            out.iter().map(|n| n.size()).sum::<u64>(),
            (1 << 24) - (1 << 16)
        );
    }

    #[test]
    fn ipv4_pools_respect_exclusions() {
        let mut g = Generator::seeded("en_US", 6).unwrap();
        for _ in 0..2000 {
            let any: u32 = g
                .ipv4(false, None, None)
                .parse::<Ipv4Addr>()
                .unwrap()
                .into();
            assert!(!in_any(any, &EXCLUDED_NETWORKS));
            let private: u32 = g
                .ipv4_private(false, None)
                .parse::<Ipv4Addr>()
                .unwrap()
                .into();
            assert!(in_any(private, &PRIVATE_NETWORKS));
            let public: u32 = g
                .ipv4_public(false, Some("b"))
                .parse::<Ipv4Addr>()
                .unwrap()
                .into();
            assert!(!in_any(public, &PRIVATE_NETWORKS) && !in_any(public, &EXCLUDED_NETWORKS));
            assert!((0x8000_0000..0xC000_0000).contains(&public));
            let network = g.ipv4(true, None, None);
            let net = parse_net(&network);
            assert_eq!(
                net.addr
                    & !(u32::MAX
                        .checked_shl(32 - u32::from(net.prefix))
                        .unwrap_or(0)),
                0,
                "{network}"
            );
        }
    }

    #[test]
    fn internet_formats() {
        let mut g = Generator::seeded("en_US", 6).unwrap();
        for _ in 0..500 {
            let email = g.email(true, None);
            let (user, domain) = email.split_once('@').unwrap();
            assert!(
                !user.is_empty() && domain.starts_with("example."),
                "{email}"
            );
            let free = g.email(false, None);
            assert!(
                free.contains('@') && !free.contains(' ') && free == free.to_lowercase(),
                "{free}"
            );
            assert_eq!(g.mac_address(false).len(), 17);
            assert!(g.ipv6(false).parse::<Ipv6Addr>().is_ok());
            let url = g.url(None);
            assert!(url.starts_with("http") && url.ends_with('/'), "{url}");
            assert_eq!(g.url(Some(&[])).get(..3), Some("://"));
            assert!(g.domain_name(2).unwrap().matches('.').count() == 2);
            assert!(g.hostname(0).contains('-'));
            assert!(!g.slug(None).unwrap().contains(' '));
        }
        assert_eq!(g.slug(Some("Hello World!")).unwrap(), "hello-world");
        assert!(g.nic_handle("X").is_err());
        assert!(
            g.dga(Some(2020), Some(1), Some(1), Some("com"), Some(10))
                .unwrap()
                .ends_with(".com")
        );
        assert_eq!(
            g.image_url(Some(10), Some(20), Some("x/{width}/{height}")),
            "x/10/20"
        );
    }
}
