//! Where the signed-in window is.
//!
//! Home is the authenticated root destination. It is not on this stack: it
//! is created once per sign-in, stays alive underneath, and keeps its state
//! while pages cover it. Pages stack above it: Details over Home, the Player
//! over Details (or over Home, from the hero). Back removes the top page,
//! and the page or root underneath is told it is visible again. Library,
//! Search, and the other screens will become further page kinds (or root
//! destinations); the stack does not change.

/// A stack of pages above the root. Empty means the root is showing.
pub(crate) struct Navigation<T> {
    stack: Vec<T>,
    /// Playback happened since the root was last showing.
    root_stale: bool,
}

impl<T> Default for Navigation<T> {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            root_stale: false,
        }
    }
}

impl<T> Navigation<T> {
    pub(crate) fn push(&mut self, page: T) {
        self.stack.push(page);
    }

    /// Note that something above the root changed what the root shows (the
    /// Player reported progress).
    pub(crate) fn mark_root_stale(&mut self) {
        self.root_stale = true;
    }

    /// Once the root is showing again: whether it should refresh. Clears
    /// the mark, so one return refreshes once.
    pub(crate) fn take_root_stale(&mut self) -> bool {
        self.stack.is_empty() && std::mem::take(&mut self.root_stale)
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

    /// Whether any open page matches, for checking that a report came from
    /// a page that is still open.
    pub(crate) fn any(&self, matches: impl FnMut(&T) -> bool) -> bool {
        self.stack.iter().any(matches)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Every page, top first, for sign-out and exit.
    pub(crate) fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.root_stale = false;
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
    fn back_walks_player_then_details_then_root() {
        let mut nav = Navigation::default();
        assert!(nav.is_empty(), "the root is the base");
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

    #[test]
    fn closed_pages_are_no_longer_current() {
        let mut nav = Navigation::default();
        nav.push(Page::Details("movie-1"));
        nav.push(Page::Player("movie-1"));
        assert!(nav.any(|page| *page == Page::Player("movie-1")));
        // Session end drains every page; a late report from one is stale.
        let _ = nav.drain().count();
        assert!(!nav.any(|page| *page == Page::Player("movie-1")));
        assert!(!nav.any(|page| *page == Page::Details("movie-1")));
        // Draining again is harmless.
        assert_eq!(nav.drain().count(), 0);
        assert!(!nav.take_root_stale());
    }

    #[test]
    fn playback_above_the_root_refreshes_it_once_on_return() {
        let mut nav = Navigation::default();
        nav.push(Page::Details("movie-1"));
        assert!(!nav.take_root_stale(), "Details alone changes nothing");
        nav.push(Page::Player("movie-1"));
        nav.mark_root_stale();
        nav.pop_if(|_| true);
        assert!(!nav.take_root_stale(), "Details still covers the root");
        nav.pop_if(|_| true);
        assert!(nav.take_root_stale());
        assert!(!nav.take_root_stale(), "once");
    }
}
