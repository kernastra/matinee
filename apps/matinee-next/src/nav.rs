//! Where the signed-in window is.
//!
//! The authenticated shell is the base. Screens stack above it: Details over
//! the shell, the Player over Details. Back removes the top screen, and the screen
//! underneath is told it is visible again so it can refresh. Home and the
//! library screens will become further page kinds; the stack does not change.

/// A stack of pages above the shell. Empty means the shell is showing.
pub(crate) struct Navigation<T> {
    stack: Vec<T>,
}

impl<T> Default for Navigation<T> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<T> Navigation<T> {
    pub(crate) fn push(&mut self, page: T) {
        self.stack.push(page);
    }

    /// Remove the top page when `matches` accepts it. The caller finishes it
    /// and tells the new top it is visible again.
    pub(crate) fn pop_if(&mut self, matches: impl FnOnce(&T) -> bool) -> Option<T> {
        if self.stack.last().is_some_and(matches) {
            self.stack.pop()
        } else {
            None
        }
    }

    pub(crate) fn top(&self) -> Option<&T> {
        self.stack.last()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Every page, top first, for sign-out and exit.
    pub(crate) fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.stack.drain(..).rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Page {
        Details(&'static str),
        Player(&'static str),
    }

    #[test]
    fn back_walks_player_then_details_then_shell() {
        let mut nav = Navigation::default();
        assert!(nav.is_empty(), "the shell is the base");
        nav.push(Page::Details("movie-1"));
        nav.push(Page::Player("movie-1"));
        assert_eq!(nav.pop_if(|_| true), Some(Page::Player("movie-1")));
        assert_eq!(
            nav.top(),
            Some(&Page::Details("movie-1")),
            "back from the Player lands on Details"
        );
        assert_eq!(nav.pop_if(|_| true), Some(Page::Details("movie-1")));
        assert!(nav.is_empty());
        assert_eq!(nav.pop_if(|_| true), None);
    }

    #[test]
    fn pop_if_only_removes_the_expected_page() {
        let mut nav = Navigation::default();
        nav.push(Page::Details("movie-1"));
        assert_eq!(nav.pop_if(|page| matches!(page, Page::Player(_))), None);
        assert_eq!(nav.top(), Some(&Page::Details("movie-1")));
        nav.push(Page::Player("movie-1"));
        let drained: Vec<_> = nav.drain().collect();
        assert_eq!(
            drained,
            vec![Page::Player("movie-1"), Page::Details("movie-1")]
        );
    }
}
