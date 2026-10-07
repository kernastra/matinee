//! What a search asks for. The rules for turning typed text into a query
//! live here, so the app and the server client agree on them.

/// Fewest characters a query may have. Shipping Matinee's overlay waits for
/// two (`trimmed.length < 2`), and one letter matches most of a library.
pub const SEARCH_MIN_CHARS: usize = 2;

/// A query the server will be asked. Only exists when the text is long enough.
///
/// The text is trimmed of surrounding whitespace and otherwise kept as typed:
/// case, punctuation, and inner spacing are the server's to match.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SearchQuery {
    term: String,
}

impl SearchQuery {
    /// The query for `input`, or `None` when it is too short to search.
    pub fn parse(input: &str) -> Option<Self> {
        let term = input.trim();
        (term.chars().count() >= SEARCH_MIN_CHARS).then(|| Self {
            term: term.to_string(),
        })
    }

    pub fn term(&self) -> &str {
        &self.term
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_are_not_queries() {
        assert_eq!(SearchQuery::parse(""), None);
        assert_eq!(SearchQuery::parse("   \t "), None);
        assert_eq!(SearchQuery::parse("  a  "), None, "one letter is too short");
    }

    #[test]
    fn trims_edges_and_keeps_the_rest() {
        let query = SearchQuery::parse("  Blade Runner  ").unwrap();
        assert_eq!(query.term(), "Blade Runner");
        assert_eq!(SearchQuery::parse("Alien").unwrap().term(), "Alien");
        assert_eq!(
            SearchQuery::parse("  ALIEN ").unwrap().term(),
            "ALIEN",
            "case is kept"
        );
        assert_eq!(
            SearchQuery::parse("Mr. Robot").unwrap().term(),
            "Mr. Robot",
            "inner punctuation and spacing are kept"
        );
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        assert!(SearchQuery::parse("éa").is_some(), "two characters");
        assert!(SearchQuery::parse("é").is_none(), "one character");
    }
}
