//! Compiles `data/<locale>/<provider>.json` (produced by `tools/extract_faker_data.py`)
//! into static Rust tables. The schema below is the single source of truth for both the
//! generated per-provider structs and the per-locale statics.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

type BuildResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
enum Kind {
    /// A single string.
    Str,
    /// `str_list` or `weighted` → `Table<&str>`.
    Table,
    /// `str_list` or `weighted` of `{{token}}` templates → `Table<Template>`.
    Templates,
    /// `str_list_list` → `&[Table<&str>]`.
    Groups,
    /// `int_list` → `&[u32]`.
    IntList,
    /// `int_range_map` → `&[(&str, u32, u32)]`.
    RangeMap,
    /// `str_list_map` → `&[(&str, &[&str])]`.
    ListMap,
    /// `countries` → `&[Country]`.
    Countries,
}

struct Field {
    json: &'static str,
    rust: &'static str,
    kind: Kind,
    required: bool,
}

const fn req(json: &'static str, rust: &'static str, kind: Kind) -> Field {
    Field {
        json,
        rust,
        kind,
        required: true,
    }
}

const fn opt(json: &'static str, rust: &'static str, kind: Kind) -> Field {
    Field {
        json,
        rust,
        kind,
        required: false,
    }
}

struct Provider {
    file: &'static str,
    struct_name: &'static str,
    fields: &'static [Field],
}

use Kind::*;

const SCHEMA: &[Provider] = &[
    Provider {
        file: "base",
        struct_name: "BaseData",
        fields: &[req(
            "language_locale_codes",
            "language_locale_codes",
            ListMap,
        )],
    },
    Provider {
        file: "person",
        struct_name: "PersonData",
        fields: &[
            req("formats", "formats", Templates),
            opt("formats_female", "formats_female", Templates),
            opt("formats_male", "formats_male", Templates),
            opt("formats_nonbinary", "formats_nonbinary", Templates),
            req("first_names", "first_names", Table),
            opt("first_names_female", "first_names_female", Table),
            opt("first_names_male", "first_names_male", Table),
            opt("first_names_nonbinary", "first_names_nonbinary", Table),
            req("last_names", "last_names", Table),
            opt("last_names_female", "last_names_female", Table),
            opt("last_names_male", "last_names_male", Table),
            opt("last_names_nonbinary", "last_names_nonbinary", Table),
            opt("prefixes", "prefixes", Table),
            opt("prefixes_female", "prefixes_female", Table),
            opt("prefixes_male", "prefixes_male", Table),
            opt("prefixes_nonbinary", "prefixes_nonbinary", Table),
            opt("suffixes", "suffixes", Table),
            opt("suffixes_female", "suffixes_female", Table),
            opt("suffixes_male", "suffixes_male", Table),
            opt("suffixes_nonbinary", "suffixes_nonbinary", Table),
            req("language_names", "language_names", Table),
        ],
    },
    Provider {
        file: "address",
        struct_name: "AddressData",
        fields: &[
            req("address_formats", "address_formats", Templates),
            req("building_number_formats", "building_number_formats", Table),
            req("city_formats", "city_formats", Templates),
            opt("city_prefixes", "city_prefixes", Table),
            req("city_suffixes", "city_suffixes", Table),
            req("countries", "countries", Table),
            req("alpha_2_country_codes", "alpha_2_country_codes", Table),
            req("alpha_3_country_codes", "alpha_3_country_codes", Table),
            opt(
                "freely_associated_states_abbr",
                "freely_associated_states_abbr",
                Table,
            ),
            opt("military_apo_format", "military_apo_format", Str),
            opt("military_dpo_format", "military_dpo_format", Str),
            opt("military_ship_prefix", "military_ship_prefix", Table),
            opt("military_state_abbr", "military_state_abbr", Table),
            req("postcode_formats", "postcode_formats", Table),
            opt(
                "secondary_address_formats",
                "secondary_address_formats",
                Table,
            ),
            opt("states", "states", Table),
            opt("states_abbr", "states_abbr", Table),
            opt("states_postcode", "states_postcode", RangeMap),
            req(
                "street_address_formats",
                "street_address_formats",
                Templates,
            ),
            req("street_name_formats", "street_name_formats", Templates),
            req("street_suffixes", "street_suffixes", Table),
            opt("territories_abbr", "territories_abbr", Table),
        ],
    },
    Provider {
        file: "company",
        struct_name: "CompanyData",
        fields: &[
            req("formats", "formats", Templates),
            req("company_suffixes", "company_suffixes", Table),
            req("catch_phrase_words", "catch_phrase_words", Groups),
            req("bsWords", "bs_words", Groups),
        ],
    },
    Provider {
        file: "internet",
        struct_name: "InternetData",
        fields: &[
            req("email_formats", "email_formats", Templates),
            req("free_email_domains", "free_email_domains", Table),
            req("hostname_prefixes", "hostname_prefixes", Table),
            req("http_assigned_codes", "http_assigned_codes", IntList),
            req("http_methods", "http_methods", Table),
            req(
                "image_placeholder_services",
                "image_placeholder_services",
                Table,
            ),
            req("safe_domain_names", "safe_domain_names", Table),
            req("tlds", "tlds", Table),
            req("uri_extensions", "uri_extensions", Table),
            req("uri_pages", "uri_pages", Table),
            req("uri_paths", "uri_paths", Table),
            req("url_formats", "url_formats", Templates),
            req("user_name_formats", "user_name_formats", Templates),
        ],
    },
    Provider {
        file: "phone_number",
        struct_name: "PhoneNumberData",
        fields: &[
            opt("basic_formats", "basic_formats", Templates),
            req("country_calling_codes", "country_calling_codes", Table),
            req("formats", "formats", Table),
            req("msisdn_formats", "msisdn_formats", Table),
        ],
    },
    Provider {
        file: "lorem",
        struct_name: "LoremData",
        fields: &[
            opt("parts_of_speech", "parts_of_speech", ListMap),
            req("sentence_punctuation", "sentence_punctuation", Str),
            req("word_connector", "word_connector", Str),
            req("word_list", "word_list", Table),
        ],
    },
    Provider {
        file: "date_time",
        struct_name: "DateTimeData",
        fields: &[
            req("centuries", "centuries", Table),
            req("countries", "countries", Countries),
        ],
    },
];

fn main() -> BuildResult<()> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let data_dir = manifest_dir.join("data");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", data_dir.display());

    let mut locales: Vec<String> = Vec::new();
    for entry in fs::read_dir(&data_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", entry.path().display());
        locales.push(entry.file_name().to_string_lossy().into_owned());
    }
    locales.sort();
    if locales.is_empty() {
        return Err(format!("no locale directories in {}", data_dir.display()).into());
    }

    let mut out = String::from("// @generated by build.rs from data/**.json. Do not edit.\n\n");
    emit_structs(&mut out)?;
    emit_locale_registry(&mut out, &locales)?;
    for locale in &locales {
        emit_locale(&mut out, &data_dir, locale)?;
    }

    let out_path = PathBuf::from(std::env::var("OUT_DIR")?).join("data.rs");
    fs::write(out_path, out)?;
    Ok(())
}

fn rust_type(kind: Kind) -> &'static str {
    match kind {
        Str => "&'static str",
        Table => "Table<&'static str>",
        Templates => "Table<Template>",
        Groups => "&'static [Table<&'static str>]",
        IntList => "&'static [u32]",
        RangeMap => "&'static [(&'static str, u32, u32)]",
        ListMap => "&'static [(&'static str, &'static [&'static str])]",
        Countries => "&'static [Country]",
    }
}

fn emit_structs(out: &mut String) -> BuildResult<()> {
    for provider in SCHEMA {
        writeln!(out, "/// Data tables for the `{}` provider.", provider.file)?;
        writeln!(
            out,
            "#[derive(Debug)]\npub struct {} {{",
            provider.struct_name
        )?;
        for field in provider.fields {
            let ty = rust_type(field.kind);
            if field.required {
                writeln!(out, "    pub {}: {ty},", field.rust)?;
            } else {
                writeln!(out, "    pub {}: Option<{ty}>,", field.rust)?;
            }
        }
        writeln!(out, "}}\n")?;
    }
    Ok(())
}

fn locale_ident(locale: &str) -> String {
    locale.to_ascii_uppercase()
}

fn locale_variant(locale: &str) -> String {
    camel_case(&locale.to_ascii_lowercase())
}

fn emit_locale_registry(out: &mut String, locales: &[String]) -> BuildResult<()> {
    writeln!(out, "/// Locales compiled into this build.")?;
    writeln!(
        out,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum LocaleId {{"
    )?;
    for locale in locales {
        writeln!(out, "    {},", locale_variant(locale))?;
    }
    writeln!(out, "}}\n")?;

    writeln!(
        out,
        "/// All data for one locale.\n#[derive(Debug)]\npub struct Locale {{"
    )?;
    writeln!(out, "    pub id: LocaleId,\n    pub code: &'static str,")?;
    for provider in SCHEMA {
        writeln!(out, "    pub {}: {},", provider.file, provider.struct_name)?;
    }
    writeln!(out, "}}\n")?;

    writeln!(out, "/// Codes of all compiled-in locales, sorted.")?;
    writeln!(out, "pub const LOCALE_CODES: &[&str] = &{locales:?};\n")?;
    writeln!(
        out,
        "/// Looks up a compiled-in locale by its code (e.g. `en_US`)."
    )?;
    writeln!(
        out,
        "pub fn find_locale(code: &str) -> Option<&'static Locale> {{\n    match code {{"
    )?;
    for locale in locales {
        writeln!(
            out,
            "        {locale:?} => Some(&{}),",
            locale_ident(locale)
        )?;
    }
    writeln!(out, "        _ => None,\n    }}\n}}\n")?;
    Ok(())
}

fn emit_locale(out: &mut String, data_dir: &Path, locale: &str) -> BuildResult<()> {
    writeln!(
        out,
        "pub static {}: Locale = Locale {{",
        locale_ident(locale)
    )?;
    writeln!(
        out,
        "    id: LocaleId::{},\n    code: {locale:?},",
        locale_variant(locale)
    )?;
    for provider in SCHEMA {
        let path = data_dir
            .join(locale)
            .join(format!("{}.json", provider.file));
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let attributes = json
            .get("attributes")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("{}: missing `attributes` object", path.display()))?;

        writeln!(out, "    {}: {} {{", provider.file, provider.struct_name)?;
        for field in provider.fields {
            let context = format!("{locale}/{}.{}", provider.file, field.json);
            let Some(value) = attributes.get(field.json) else {
                if field.required {
                    return Err(format!("{context}: required attribute is missing").into());
                }
                writeln!(out, "        {}: None,", field.rust)?;
                continue;
            };
            let expr = emit_value(field.kind, value, &context)?;
            if field.required {
                writeln!(out, "        {}: {expr},", field.rust)?;
            } else {
                writeln!(out, "        {}: Some({expr}),", field.rust)?;
            }
        }
        writeln!(out, "    }},")?;
    }
    writeln!(out, "}};\n")?;
    Ok(())
}

fn type_of<'a>(value: &'a Value, context: &str) -> BuildResult<&'a str> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{context}: missing `type`").into())
}

fn array<'a>(value: &'a Value, key: &str, context: &str) -> BuildResult<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{context}: missing array `{key}`").into())
}

fn string(value: &Value, context: &str) -> BuildResult<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{context}: expected a string, found {value}").into())
}

fn strings(values: &[Value], context: &str) -> BuildResult<Vec<String>> {
    values.iter().map(|v| string(v, context)).collect()
}

fn emit_value(kind: Kind, value: &Value, context: &str) -> BuildResult<String> {
    let ty = type_of(value, context)?;
    let mismatch = || -> Box<dyn Error> {
        format!("{context}: data type `{ty}` does not fit the schema").into()
    };
    match kind {
        Str => {
            if ty != "str" {
                return Err(mismatch());
            }
            let s = string(value.get("value").unwrap_or(&Value::Null), context)?;
            Ok(format!("{s:?}"))
        }
        Table | Templates => {
            let (items, weights) = match ty {
                "str_list" => (strings(array(value, "items", context)?, context)?, None),
                "weighted" => {
                    let items = strings(array(value, "items", context)?, context)?;
                    let weights = array(value, "weights", context)?
                        .iter()
                        .map(|w| {
                            w.as_f64()
                                .ok_or_else(|| format!("{context}: non-numeric weight {w}"))
                        })
                        .collect::<Result<Vec<f64>, String>>()?;
                    if weights.len() != items.len() {
                        return Err(format!(
                            "{context}: {} items but {} weights",
                            items.len(),
                            weights.len()
                        )
                        .into());
                    }
                    (items, Some(weights))
                }
                _ => return Err(mismatch()),
            };
            if items.is_empty() {
                return Err(format!("{context}: table must not be empty").into());
            }
            let rendered: Vec<String> = match kind {
                Templates => items
                    .iter()
                    .map(|s| template_expr(s, context))
                    .collect::<BuildResult<_>>()?,
                _ => items.iter().map(|s| format!("{s:?}")).collect(),
            };
            Ok(table_expr(&rendered, weights.as_deref())?)
        }
        Groups => {
            if ty != "str_list_list" {
                return Err(mismatch());
            }
            let mut groups = Vec::new();
            for group in array(value, "items", context)? {
                let items = group
                    .as_array()
                    .ok_or_else(|| format!("{context}: group is not an array"))?;
                let rendered: Vec<String> = strings(items, context)?
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect();
                if rendered.is_empty() {
                    return Err(format!("{context}: group must not be empty").into());
                }
                groups.push(table_expr(&rendered, None)?);
            }
            Ok(format!("&[{}]", groups.join(", ")))
        }
        IntList => {
            if ty != "int_list" {
                return Err(mismatch());
            }
            let ints = array(value, "items", context)?
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| format!("{context}: bad int {v}"))
                })
                .collect::<Result<Vec<u32>, String>>()?;
            Ok(format!("&{ints:?}"))
        }
        RangeMap => {
            if ty != "int_range_map" {
                return Err(mismatch());
            }
            let mut entries = Vec::new();
            for entry in array(value, "entries", context)? {
                let parts = entry
                    .as_array()
                    .ok_or_else(|| format!("{context}: entry is not an array"))?;
                let [key, lo, hi] = parts.as_slice() else {
                    return Err(format!("{context}: entry must be [key, lo, hi]").into());
                };
                let bound = |v: &Value| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| format!("{context}: bad bound {v}"))
                };
                entries.push(format!(
                    "({:?}, {}, {})",
                    string(key, context)?,
                    bound(lo)?,
                    bound(hi)?
                ));
            }
            Ok(format!("&[{}]", entries.join(", ")))
        }
        ListMap => {
            if ty != "str_list_map" {
                return Err(mismatch());
            }
            let mut entries = Vec::new();
            for entry in array(value, "entries", context)? {
                let parts = entry
                    .as_array()
                    .ok_or_else(|| format!("{context}: entry is not an array"))?;
                let [key, items] = parts.as_slice() else {
                    return Err(format!("{context}: entry must be [key, items]").into());
                };
                let items = items
                    .as_array()
                    .ok_or_else(|| format!("{context}: entry items are not an array"))?;
                entries.push(format!(
                    "({:?}, &{:?})",
                    string(key, context)?,
                    strings(items, context)?
                ));
            }
            Ok(format!("&[{}]", entries.join(", ")))
        }
        Countries => {
            if ty != "countries" {
                return Err(mismatch());
            }
            let mut countries = Vec::new();
            for country in array(value, "items", context)? {
                let field = |name: &str| string(country.get(name).unwrap_or(&Value::Null), context);
                let timezones = strings(array(country, "timezones", context)?, context)?;
                countries.push(format!(
                    "Country {{ name: {:?}, alpha_2_code: {:?}, alpha_3_code: {:?}, continent: {:?}, capital: {:?}, timezones: &{timezones:?} }}",
                    field("name")?,
                    field("alpha_2_code")?,
                    field("alpha_3_code")?,
                    field("continent")?,
                    field("capital")?,
                ));
            }
            Ok(format!("&[{}]", countries.join(", ")))
        }
    }
}

fn table_expr(items: &[String], weights: Option<&[f64]>) -> BuildResult<String> {
    let cum = match weights {
        None => "None".to_owned(),
        Some(weights) => {
            let mut total = 0.0_f64;
            let mut cum = Vec::with_capacity(weights.len());
            for &w in weights {
                if !w.is_finite() || w < 0.0 {
                    return Err(format!("invalid weight {w}").into());
                }
                total += w;
                cum.push(format!("{total:?}"));
            }
            if total <= 0.0 {
                return Err("weights must not all be zero".into());
            }
            format!("Some(&[{}])", cum.join(", "))
        }
    };
    Ok(format!(
        "Table {{ items: &[{}], cum_weights: {cum} }}",
        items.join(", ")
    ))
}

/// Tokenizes a Faker template the way `Generator.parse` does: `{{ name }}` becomes a
/// formatter call and everything else (including unmatched braces) stays literal.
fn template_expr(template: &str, context: &str) -> BuildResult<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut literal = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        literal.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match parse_token(after) {
            Some((name, group, consumed)) => {
                if group {
                    return Err(format!(
                        "{context}: argument groups are not supported in data: {template:?}"
                    )
                    .into());
                }
                if !literal.is_empty() {
                    segments.push(format!("Segment::Literal({literal:?})"));
                    literal.clear();
                }
                segments.push(format!("Segment::Token(Formatter::{})", camel_case(name)));
                rest = &after[consumed..];
            }
            None => {
                literal.push('{');
                rest = &rest[start + 1..];
            }
        }
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        segments.push(format!("Segment::Literal({literal:?})"));
    }
    Ok(format!("&[{}]", segments.join(", ")))
}

/// Matches `\s*(\w+)(:\s*\w+?)?\s*\}\}` at the start of `s`.
/// Returns the token name, whether an argument group was present, and bytes consumed.
fn parse_token(s: &str) -> Option<(&str, bool, usize)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = s.len() - s.trim_start().len();
    let name_len = s[i..].find(|c: char| !is_word(c)).unwrap_or(s.len() - i);
    if name_len == 0 {
        return None;
    }
    let name = &s[i..i + name_len];
    i += name_len;
    let mut group = false;
    if s[i..].starts_with(':') {
        let after_colon = &s[i + 1..];
        let ws = after_colon.len() - after_colon.trim_start().len();
        let word = after_colon[ws..]
            .find(|c: char| !is_word(c))
            .unwrap_or(after_colon.len() - ws);
        if word == 0 {
            return None;
        }
        group = true;
        i += 1 + ws + word;
    }
    i += s[i..].len() - s[i..].trim_start().len();
    s[i..].starts_with("}}").then_some((name, group, i + 2))
}

fn camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    for part in snake.split('_').filter(|p| !p.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}
