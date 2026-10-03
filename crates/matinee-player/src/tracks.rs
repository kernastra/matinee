//! Matinee-owned track identities.

#![forbid(unsafe_code)]
//!
//! Identifiers are assigned when a track is first seen for the current media
//! and are kept across list refreshes. They are not libmpv track ids. A new
//! `load` starts a new identity space.

use std::fmt;

/// Stable id for one track of the current media item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackId(u64);

impl TrackId {
    pub fn raw(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Audio, or a subtitle burned into the frame by the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Subtitle,
}

/// Subtitle packets. Image subtitles are still drawn by the engine, not by UI text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleForm {
    Text,
    Image,
}

/// One selectable track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub form: Option<SubtitleForm>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub codec: Option<String>,
}

/// A row as reported by the engine, before identity assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTrack {
    pub engine_id: i64,
    pub kind: TrackKind,
    pub form: Option<SubtitleForm>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub codec: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug)]
struct Row {
    id: TrackId,
    engine_id: i64,
    kind: TrackKind,
    form: Option<SubtitleForm>,
    language: Option<String>,
    title: Option<String>,
    codec: Option<String>,
    selected: bool,
}

/// Maps engine track ids to [`TrackId`] for the current media item.
#[derive(Clone, Debug, Default)]
pub struct TrackTable {
    next: u64,
    rows: Vec<Row>,
}

impl TrackTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the list. Existing ids are reused when the engine id and kind match.
    /// Returns whether the public list changed.
    pub fn sync(&mut self, incoming: &[RawTrack]) -> bool {
        let mut next_rows = Vec::with_capacity(incoming.len());
        for raw in incoming {
            let id = self
                .rows
                .iter()
                .find(|row| row.engine_id == raw.engine_id && row.kind == raw.kind)
                .map(|row| row.id)
                .unwrap_or_else(|| {
                    self.next += 1;
                    TrackId(self.next)
                });
            next_rows.push(Row {
                id,
                engine_id: raw.engine_id,
                kind: raw.kind,
                form: raw.form,
                language: raw.language.clone(),
                title: raw.title.clone(),
                codec: raw.codec.clone(),
                selected: raw.selected,
            });
        }
        let changed = self.public_rows() != public_of(&next_rows)
            || self.selection() != selection_of(&next_rows);
        self.rows = next_rows;
        changed
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.next = 0;
    }

    pub fn audio(&self) -> Vec<Track> {
        self.of_kind(TrackKind::Audio)
    }

    pub fn subtitles(&self) -> Vec<Track> {
        self.of_kind(TrackKind::Subtitle)
    }

    pub fn engine_id(&self, id: TrackId) -> Option<i64> {
        self.rows
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.engine_id)
    }

    pub fn selected(&self, kind: TrackKind) -> Option<TrackId> {
        self.rows
            .iter()
            .find(|row| row.kind == kind && row.selected)
            .map(|row| row.id)
    }

    fn of_kind(&self, kind: TrackKind) -> Vec<Track> {
        self.rows
            .iter()
            .filter(|row| row.kind == kind)
            .map(Track::from)
            .collect()
    }

    fn public_rows(&self) -> Vec<Track> {
        self.rows.iter().map(Track::from).collect()
    }

    fn selection(&self) -> Vec<(TrackId, bool)> {
        self.rows.iter().map(|row| (row.id, row.selected)).collect()
    }
}

fn public_of(rows: &[Row]) -> Vec<Track> {
    rows.iter().map(Track::from).collect()
}

fn selection_of(rows: &[Row]) -> Vec<(TrackId, bool)> {
    rows.iter().map(|row| (row.id, row.selected)).collect()
}

impl From<&Row> for Track {
    fn from(row: &Row) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            form: row.form,
            language: row.language.clone(),
            title: row.title.clone(),
            codec: row.codec.clone(),
        }
    }
}

/// Classify a subtitle codec name from the engine.
pub fn subtitle_form(codec: Option<&str>) -> SubtitleForm {
    let Some(codec) = codec.map(|value| value.to_ascii_lowercase()) else {
        return SubtitleForm::Text;
    };
    const IMAGE: &[&str] = &[
        "hdmv_pgs_subtitle",
        "pgs",
        "dvd_subtitle",
        "dvdsub",
        "dvb_subtitle",
        "xsub",
        "pgssub",
    ];
    if IMAGE.iter().any(|name| codec.contains(name)) {
        SubtitleForm::Image
    } else {
        SubtitleForm::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: i64, kind: TrackKind, selected: bool) -> RawTrack {
        RawTrack {
            engine_id: id,
            kind,
            form: Some(SubtitleForm::Text),
            language: Some("eng".into()),
            title: None,
            codec: Some("subrip".into()),
            selected,
        }
    }

    #[test]
    fn ids_survive_refresh_and_reset_on_clear() {
        let mut table = TrackTable::new();
        assert!(table.sync(&[
            raw(1, TrackKind::Audio, true),
            raw(2, TrackKind::Audio, false),
            raw(1, TrackKind::Subtitle, false),
        ]));
        let audio = table.audio();
        let sub = table.subtitles()[0].id;
        assert!(table.sync(&[
            raw(1, TrackKind::Audio, true),
            raw(2, TrackKind::Audio, false),
            raw(1, TrackKind::Subtitle, true),
        ]));
        assert_eq!(table.audio()[0].id, audio[0].id);
        assert_eq!(table.audio()[1].id, audio[1].id);
        assert_eq!(table.selected(TrackKind::Subtitle), Some(sub));
        table.sync(&[
            raw(1, TrackKind::Audio, true),
            raw(4, TrackKind::Audio, false),
        ]);
        assert!(table.subtitles().is_empty());
        assert_ne!(table.audio()[1].id, audio[1].id);
        table.clear();
        table.sync(&[raw(1, TrackKind::Audio, true)]);
        assert_eq!(table.audio()[0].id.raw(), 1);
    }

    #[test]
    fn image_codecs_are_not_text() {
        assert_eq!(
            subtitle_form(Some("hdmv_pgs_subtitle")),
            SubtitleForm::Image
        );
        assert_eq!(subtitle_form(Some("subrip")), SubtitleForm::Text);
        assert_eq!(subtitle_form(None), SubtitleForm::Text);
    }
}
