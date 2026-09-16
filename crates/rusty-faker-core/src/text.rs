//! Python string helpers used by providers: slugify, ASCII folding and title casing.

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

/// Python's `str.isspace` set (Rust's `is_whitespace` misses U+001C–U+001F).
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python regex `\w` (Unicode mode).
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Transliterates to ASCII, like Faker's `unidecode`. ASCII input is returned as-is.
pub fn to_ascii(value: &str) -> Cow<'_, str> {
    if value.is_ascii() {
        return Cow::Borrowed(value);
    }
    Cow::Owned(deunicode::deunicode(value))
}

/// Faker's `slugify`: normalize, drop characters other than word characters, whitespace
/// and hyphens, trim, lowercase, and collapse runs of hyphens/whitespace to one `-`.
pub fn slugify(value: &str, allow_unicode: bool) -> String {
    let normalized: Cow<'_, str> = match (value.is_ascii(), allow_unicode) {
        (true, _) => Cow::Borrowed(value),
        (false, true) => Cow::Owned(value.nfkc().collect()),
        (false, false) => Cow::Owned(value.nfkd().filter(char::is_ascii).collect()),
    };
    let kept: String = normalized
        .chars()
        .filter(|&c| is_word(c) || is_py_space(c) || c == '-')
        .collect();
    let lowered = kept.trim_matches(is_py_space).to_lowercase();

    let mut out = String::with_capacity(lowered.len());
    let mut in_separator = false;
    for c in lowered.chars() {
        if c == '-' || is_py_space(c) {
            if !in_separator {
                out.push('-');
                in_separator = true;
            }
            continue;
        }
        in_separator = false;
        out.push(c);
    }
    out
}

/// Python's `str.title`: uppercase letters that follow a non-letter, lowercase the rest.
pub fn title(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_is_letter = false;
    for c in value.chars() {
        if previous_is_letter {
            out.extend(c.to_lowercase());
        } else {
            out.extend(c.to_uppercase());
        }
        previous_is_letter = c.is_alphabetic();
    }
    out
}

/// Uppercases only the first character, leaving the rest unchanged.
pub fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_django_behavior() {
        assert_eq!(slugify("  Hello, World!  ", false), "hello-world");
        assert_eq!(slugify("a - b\t\tc", false), "a-b-c");
        assert_eq!(slugify("Crème Brûlée", false), "creme-brulee");
        assert_eq!(slugify("Crème Brûlée", true), "crème-brûlée");
        assert_eq!(slugify("Smith,", true), "smith");
    }

    #[test]
    fn ascii_and_title() {
        assert_eq!(to_ascii("abc"), "abc");
        assert_eq!(to_ascii("Straße"), "Strasse");
        assert_eq!(title("hELLO wORLD"), "Hello World");
        assert_eq!(title("they're"), "They'Re");
        assert_eq!(capitalize_first("hello World"), "Hello World");
    }
}
