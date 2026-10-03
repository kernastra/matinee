//! Per-item user state: resume, progress, favorite, and played.

use std::time::Duration;

/// What the signed-in user has done with one item.
///
/// A resume position is stored only when it is non-zero, which is when the
/// title is resumable. A played percentage can exist on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserItemState {
    resume_position: Option<Duration>,
    played_percentage: Option<f32>,
    favorite: bool,
    played: bool,
    play_count: u32,
}

impl UserItemState {
    /// Build state from values already converted to domain time.
    ///
    /// A zero resume position means the title is not resumable.
    pub fn from_parts(
        resume_position: Option<Duration>,
        played_percentage: Option<f32>,
        favorite: bool,
        played: bool,
        play_count: u32,
    ) -> Self {
        let resume_position = resume_position.filter(|position| !position.is_zero());
        let played_percentage = played_percentage.filter(|value| value.is_finite());
        Self {
            resume_position,
            played_percentage,
            favorite,
            played,
            play_count,
        }
    }

    pub fn is_resumable(&self) -> bool {
        self.resume_position.is_some()
    }

    pub fn resume_position(&self) -> Option<Duration> {
        self.resume_position
    }

    pub fn played_percentage(&self) -> Option<f32> {
        self.played_percentage
    }

    pub fn is_favorite(&self) -> bool {
        self.favorite
    }

    pub fn is_played(&self) -> bool {
        self.played
    }

    pub fn play_count(&self) -> u32 {
        self.play_count
    }

    /// Resume position, watched fraction, or both.
    ///
    /// Present whenever the title is resumable, even if the server did not
    /// send a percentage. Also present when a percentage was sent without a
    /// resume position.
    pub fn viewing_progress(&self) -> Option<ViewingProgress> {
        let position = self.resume_position;
        let fraction = self
            .played_percentage
            .filter(|value| *value > 0.0)
            .map(|value| (value / 100.0).clamp(0.0, 1.0));
        if position.is_none() && fraction.is_none() {
            None
        } else {
            Some(ViewingProgress { position, fraction })
        }
    }
}

/// Progress a shelf or details view can show without inventing a percentage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewingProgress {
    pub position: Option<Duration>,
    pub fraction: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_position_is_not_resumable() {
        let state = UserItemState::from_parts(Some(Duration::ZERO), None, false, false, 0);
        assert!(!state.is_resumable());
        assert!(state.viewing_progress().is_none());
    }

    #[test]
    fn resume_stays_available_without_a_percentage() {
        let state = UserItemState::from_parts(Some(Duration::from_secs(90)), None, true, false, 2);
        assert!(state.is_resumable());
        assert!(state.is_favorite());
        assert_eq!(state.play_count(), 2);
        let progress = state.viewing_progress().unwrap();
        assert_eq!(progress.position, Some(Duration::from_secs(90)));
        assert!(progress.fraction.is_none());
    }

    #[test]
    fn percentage_is_progress_without_a_resume_position() {
        let state = UserItemState::from_parts(None, Some(25.0), false, false, 0);
        assert!(!state.is_resumable());
        let progress = state.viewing_progress().unwrap();
        assert!(progress.position.is_none());
        assert!((progress.fraction.unwrap() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn played_flag_is_independent_of_resume() {
        let state = UserItemState::from_parts(None, Some(100.0), false, true, 1);
        assert!(state.is_played());
        assert!(state.viewing_progress().unwrap().fraction.unwrap() > 0.99);
    }
}
