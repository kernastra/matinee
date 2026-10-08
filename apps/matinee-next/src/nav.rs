//! Where the signed-in window is.
//!
//! Two layers. A [`RootDestination`] is a place the person goes to from the
//! app bar: Home, Library, Search, or Calendar today; Settings joins later. Each root screen is created once per sign-in and stays alive
//! while another root or a page is showing, so it keeps its state.
//!
//! Pages stack above whichever root is current: Details, and the Player
//! over Details (or over Home, from the hero). Back removes the top page,
//! and the page or root underneath is told it is visible again. Choosing a
//! root does not touch the stack; the app bar is only on roots.

use matinee_core::LibraryKind;

/// A place reached from the app bar. Not a page: it is not pushed, and
/// moving between roots keeps each one as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RootDestination {
    Home,
    /// Library, showing movies or series.
    Library(LibraryKind),
    /// Search: type a title and open it. Its text and results are kept while
    /// another root shows.
    Search,
    /// Calendar: upcoming movie and episode releases, by day. Its month, day,
    /// and loaded answers are kept while another root shows.
    Calendar,
}

impl RootDestination {
    /// App bar entries, in order. Only destinations that exist natively.
    pub(crate) const BAR: [RootDestination; 5] = [
        RootDestination::Home,
        RootDestination::Library(LibraryKind::Movies),
        RootDestination::Library(LibraryKind::Series),
        RootDestination::Search,
        RootDestination::Calendar,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Library(kind) => kind.title(),
            Self::Search => "Search",
            Self::Calendar => "Calendar",
        }
    }

    /// Which root screen shows this destination.
    pub(crate) fn root(self) -> Root {
        match self {
            Self::Home => Root::Home,
            Self::Library(_) => Root::Library,
            Self::Search => Root::Search,
            Self::Calendar => Root::Calendar,
        }
    }
}

/// A root screen. Library is one screen for both kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Root {
    Home,
    Library,
    Search,
    Calendar,
}

/// Which roots must reload what playback may have changed the next time
/// they show. Playback marks every root; showing a root takes its mark.
#[derive(Debug, Default)]
pub(crate) struct StaleRoots {
    home: bool,
    library: bool,
    search: bool,
}

impl StaleRoots {
    pub(crate) fn mark_all(&mut self) {
        self.home = true;
        self.library = true;
        self.search = true;
    }

    /// Whether `root` should refresh now. Clears its mark. Calendar never
    /// takes one: releases and monitoring do not come from playback.
    pub(crate) fn take(&mut self, root: Root) -> bool {
        match root {
            Root::Home => std::mem::take(&mut self.home),
            Root::Library => std::mem::take(&mut self.library),
            Root::Search => std::mem::take(&mut self.search),
            Root::Calendar => false,
        }
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

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
    fn playback_marks_every_root_and_each_refreshes_once() {
        let mut stale = StaleRoots::default();
        assert!(!stale.take(Root::Library));
        stale.mark_all();
        assert!(stale.take(Root::Library), "the root returned to");
        assert!(!stale.take(Root::Library), "once");
        assert!(stale.take(Root::Home), "the other root, when it shows");
        assert!(!stale.take(Root::Home));
        stale.mark_all();
        stale.clear();
        assert!(!stale.take(Root::Home));
    }

    #[test]
    fn the_bar_lists_only_native_destinations() {
        let labels: Vec<&str> = RootDestination::BAR.iter().map(|d| d.label()).collect();
        assert_eq!(
            labels,
            vec!["Home", "Movies", "Series", "Search", "Calendar"]
        );
        assert_eq!(RootDestination::Calendar.root(), Root::Calendar);
        assert!(!StaleRoots::default().take(Root::Calendar));
        assert_eq!(RootDestination::Search.root(), Root::Search);
        assert_eq!(RootDestination::Home.root(), Root::Home);
        assert_eq!(
            RootDestination::Library(LibraryKind::Series).root(),
            Root::Library
        );
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
