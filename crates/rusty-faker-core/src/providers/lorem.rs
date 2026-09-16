//! Faker's `lorem` provider.

use crate::error::{Error, Result, invalid};
use crate::generator::Generator;
use crate::text::{capitalize_first, title};

/// Faker would loop forever when no word fits; give up after this many attempts instead.
const MAX_TEXT_ATTEMPTS: usize = 10_000;

impl Generator {
    /// The word list to draw from: `ext_word_list` if given, else the part of speech, else all words.
    pub fn get_words_list<'a>(
        &self,
        part_of_speech: Option<&str>,
        ext_word_list: Option<&'a [&'a str]>,
    ) -> Result<&'a [&'a str]> {
        if let Some(words) = ext_word_list {
            return Ok(words);
        }
        let lorem = &self.locale_data().lorem;
        let Some(part) = part_of_speech.filter(|p| !p.is_empty()) else {
            return Ok(lorem.word_list.items);
        };
        lorem
            .parts_of_speech
            .and_then(|parts| parts.iter().find(|(name, _)| *name == part))
            .map(|(_, words)| *words)
            .ok_or_else(|| invalid(format!("{part} is not recognized as a part of speech.")))
    }

    /// `nb` words, with replacement unless `unique`.
    pub fn words<'a>(
        &mut self,
        nb: i64,
        ext_word_list: Option<&'a [&'a str]>,
        part_of_speech: Option<&str>,
        unique: bool,
    ) -> Result<Vec<&'a str>> {
        let words = self.get_words_list(part_of_speech, ext_word_list)?;
        if unique {
            let nb = usize::try_from(nb)
                .map_err(|_| invalid("Sample larger than population or is negative"))?;
            if nb > words.len() {
                return Err(invalid(
                    "Sample length cannot be longer than the number of unique elements to pick from.",
                ));
            }
            return Ok(self
                .sample_indices(words.len(), nb)
                .into_iter()
                .map(|i| words[i])
                .collect());
        }
        let nb = usize::try_from(nb).unwrap_or(0);
        if nb > 0 && words.is_empty() {
            return Err(Error::EmptySequence);
        }
        Ok((0..nb).map(|_| words[self.index(words.len())]).collect())
    }

    pub fn word<'a>(
        &mut self,
        part_of_speech: Option<&str>,
        ext_word_list: Option<&'a [&'a str]>,
    ) -> Result<&'a str> {
        let words = self.get_words_list(part_of_speech, ext_word_list)?;
        if words.is_empty() {
            return Err(Error::EmptySequence);
        }
        Ok(words[self.index(words.len())])
    }

    /// A sentence of about `nb_words` words (±40% when `variable_nb_words`).
    pub fn sentence(
        &mut self,
        nb_words: i64,
        variable_nb_words: bool,
        ext_word_list: Option<&[&str]>,
    ) -> Result<String> {
        if nb_words <= 0 {
            return Ok(String::new());
        }
        let nb_words = if variable_nb_words {
            self.randomize_nb_elements(nb_words, false, false, Some(1), None)
        } else {
            nb_words
        };
        let words = self.words(nb_words, ext_word_list, None, false)?;
        let lorem = &self.locale_data().lorem;
        let mut out = String::with_capacity(words.iter().map(|w| w.len() + 1).sum::<usize>() + 1);
        for (i, word) in words.iter().enumerate() {
            if i == 0 {
                out.push_str(&title(word));
            } else {
                out.push_str(lorem.word_connector);
                out.push_str(word);
            }
        }
        out.push_str(lorem.sentence_punctuation);
        Ok(out)
    }

    pub fn sentences(&mut self, nb: i64, ext_word_list: Option<&[&str]>) -> Result<Vec<String>> {
        (0..nb.max(0))
            .map(|_| self.sentence(6, true, ext_word_list))
            .collect()
    }

    /// A paragraph of about `nb_sentences` sentences (±40% when `variable_nb_sentences`).
    pub fn paragraph(
        &mut self,
        nb_sentences: i64,
        variable_nb_sentences: bool,
        ext_word_list: Option<&[&str]>,
    ) -> Result<String> {
        if nb_sentences <= 0 {
            return Ok(String::new());
        }
        let nb = if variable_nb_sentences {
            self.randomize_nb_elements(nb_sentences, false, false, Some(1), None)
        } else {
            nb_sentences
        };
        let sentences = self.sentences(nb, ext_word_list)?;
        Ok(sentences.join(self.locale_data().lorem.word_connector))
    }

    pub fn paragraphs(&mut self, nb: i64, ext_word_list: Option<&[&str]>) -> Result<Vec<String>> {
        (0..nb.max(0))
            .map(|_| self.paragraph(3, true, ext_word_list))
            .collect()
    }

    /// Text of at most `max_nb_chars` characters, built from words (<25), sentences (<100)
    /// or paragraphs.
    pub fn text(&mut self, max_nb_chars: i64, ext_word_list: Option<&[&str]>) -> Result<String> {
        if max_nb_chars < 5 {
            return Err(invalid(
                "text() can only generate text of at least 5 characters",
            ));
        }
        let limit = usize::try_from(max_nb_chars).unwrap_or(usize::MAX);
        let connector = self.locale_data().lorem.word_connector;
        let punctuation = self.locale_data().lorem.sentence_punctuation;

        for _ in 0..MAX_TEXT_ATTEMPTS {
            let mut parts: Vec<String> = Vec::new();
            let mut size = 0usize;
            while size < limit {
                let separator = if size == 0 {
                    ""
                } else if max_nb_chars < 100 {
                    connector
                } else {
                    "\n"
                };
                let body = match max_nb_chars {
                    ..25 => self.word(None, ext_word_list)?.to_owned(),
                    25..100 => self.sentence(6, true, ext_word_list)?,
                    _ => self.paragraph(3, true, ext_word_list)?,
                };
                let part = format!("{separator}{body}");
                size += part.chars().count();
                parts.push(part);
            }
            parts.pop();
            if parts.is_empty() {
                continue;
            }
            if max_nb_chars < 25 {
                parts[0] = capitalize_first(&parts[0]);
                if let Some(last) = parts.last_mut() {
                    last.push_str(punctuation);
                }
            }
            return Ok(parts.concat());
        }
        Err(invalid(format!(
            "could not generate text of at most {max_nb_chars} characters from the word list"
        )))
    }

    pub fn texts(
        &mut self,
        nb_texts: i64,
        max_nb_chars: i64,
        ext_word_list: Option<&[&str]>,
    ) -> Result<Vec<String>> {
        (0..nb_texts.max(0))
            .map(|_| self.text(max_nb_chars, ext_word_list))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Error, Generator};

    #[test]
    fn sentences_and_text() {
        let mut g = Generator::seeded("en_US", 4).unwrap();
        for max in [5, 20, 24, 25, 99, 100, 200, 1000] {
            for _ in 0..50 {
                let text = g.text(max, None).unwrap();
                assert!(text.chars().count() <= max as usize + 1, "{max}: {text:?}");
                assert!(!text.is_empty());
            }
        }
        let sentence = g.sentence(6, false, None).unwrap();
        assert_eq!(sentence.split(' ').count(), 6);
        assert!(sentence.ends_with('.') && sentence.starts_with(|c: char| c.is_uppercase()));
        assert_eq!(g.sentence(0, true, None).unwrap(), "");
    }

    #[test]
    fn words_rules() {
        let mut g = Generator::seeded("en_US", 4).unwrap();
        let ext = ["a", "b", "c"];
        let unique = g.words(3, Some(&ext), None, true).unwrap();
        assert_eq!(unique.len(), 3);
        assert!(g.words(4, Some(&ext), None, true).is_err());
        assert!(g.words(1, None, Some("noun"), false).is_ok());
        assert!(g.words(1, None, Some("nonsense"), false).is_err());
        assert_eq!(g.word(None, Some(&[])), Err(Error::EmptySequence));
        assert!(g.text(5, Some(&["waytoolong"])).is_err());
    }
}
