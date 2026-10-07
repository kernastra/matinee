//! Review scenes. Fixture metadata and generated abstract artwork; no
//! server, no socket, no real film frames.

use std::sync::Arc;
use std::time::Duration;

use atelier_ui::prelude::*;
use matinee_core::{
    AudioStream, Chapter, CollectionContext, Credit, DynamicRange, ImageTag, ItemHierarchy, ItemId,
    ItemIdentity, ItemKind, ItemMetadata, MediaItem, MediaStream, Person, TechnicalMedia,
    UserItemState, VideoStream,
};

use super::model::{DetailsFailure, DetailsModel, Response};
use super::screen::DetailsScreen;
use crate::artwork::{Artwork, ArtworkLoader};
use crate::runtime::ServiceRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetailsPreview {
    Movie,
    MovieResume,
    Series,
    Season,
    Loading,
    Error,
}

const BACKDROP: &[u8] = include_bytes!("../../assets/review/backdrop.jpg");
const BACKDROP_SERIES: &[u8] = include_bytes!("../../assets/review/backdrop-series.jpg");
const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");
const POSTER_SERIES: &[u8] = include_bytes!("../../assets/review/poster-series.jpg");
const THUMBS: [&[u8]; 3] = [
    include_bytes!("../../assets/review/thumb-1.jpg"),
    include_bytes!("../../assets/review/thumb-2.jpg"),
    include_bytes!("../../assets/review/thumb-3.jpg"),
];
const PORTRAITS: [&[u8]; 2] = [
    include_bytes!("../../assets/review/portrait-1.jpg"),
    include_bytes!("../../assets/review/portrait-2.jpg"),
];

impl DetailsScreen {
    pub(crate) fn preview(
        runtime: Arc<ServiceRuntime>,
        loader: ArtworkLoader,
        scene: DetailsPreview,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = preview_model(scene);
        let mut screen = Self::with_model(
            runtime,
            None,
            crate::model::review_session(),
            loader,
            model,
            cx,
        );
        let wanted = screen.wanted_artwork();
        let mut related = 0usize;
        for request in wanted {
            let url = request.url.as_str();
            let bytes = if url.contains("/Images/Backdrop") {
                Some(if url.contains("series") {
                    BACKDROP_SERIES
                } else {
                    BACKDROP
                })
            } else if url.contains("/Items/person-") {
                (url.contains("person-1") || url.contains("person-2"))
                    .then(|| PORTRAITS[usize::from(url.contains("person-2"))])
            } else if let Some(index) = url.find("/Items/ep-") {
                let digit = url[index..]
                    .bytes()
                    .find(u8::is_ascii_digit)
                    .unwrap_or(b'1');
                Some(THUMBS[usize::from(digit - b'0') % THUMBS.len()])
            } else if url.contains("/Items/northwind/") {
                Some(POSTER)
            } else if url.contains("/Items/harbor-lights/") {
                Some(POSTER_SERIES)
            } else {
                related += 1;
                match related % 3 {
                    0 => None,
                    1 => Some(POSTER_SERIES),
                    _ => Some(POSTER),
                }
            };
            let art = bytes
                .and_then(|bytes| DecodedImage::decode(bytes, MAX_DECODED_SIDE).ok())
                .map(Artwork::Ready)
                .unwrap_or(Artwork::Missing);
            screen.art.insert(request.url, art);
        }
        screen
    }
}

fn id(value: &str) -> ItemId {
    ItemId::parse(value).expect("fixture id")
}

fn tag(value: &str) -> Option<ImageTag> {
    ImageTag::parse(value)
}

fn base(value: &str, name: &str, kind: ItemKind) -> MediaItem {
    MediaItem {
        identity: ItemIdentity {
            id: id(value),
            name: name.into(),
        },
        kind,
        metadata: ItemMetadata::default(),
        artwork: Default::default(),
        user: UserItemState::default(),
        hierarchy: ItemHierarchy::default(),
        media: TechnicalMedia::default(),
        people: Vec::new(),
        chapters: Vec::new(),
    }
}

fn person(index: usize, name: &str, role: &str, credit: Credit) -> Person {
    Person {
        id: ItemId::parse(format!("person-{index}")).ok(),
        name: name.into(),
        role: (!role.is_empty()).then(|| role.to_string()),
        credit,
        image: tag("face"),
    }
}

fn poster(value: &str, name: &str, year: i32) -> MediaItem {
    let mut item = base(value, name, ItemKind::Movie);
    item.metadata.year = Some(year);
    item.artwork.primary = tag("poster");
    item
}

fn northwind(resume: Option<Duration>) -> MediaItem {
    let mut movie = base("northwind", "Northwind", ItemKind::Movie);
    movie.metadata = ItemMetadata {
        overview: Some("A lighthouse keeper on a failing northern island finds a ship's log that predicts the next storm to the hour. As the village prepares, she has to decide how much of the rest of the log she is willing to believe.".into()),
        year: Some(2019),
        runtime: Some(Duration::from_secs(2 * 3600 + 4 * 60)),
        community_rating: Some(7.8),
        critic_rating: Some(91.0),
        official_rating: Some("PG-13".into()),
        genres: vec!["Drama".into(), "Mystery".into(), "Adventure".into()],
        taglines: vec!["Every storm was written down.".into()],
        studios: vec!["Harbor Pictures".into()],
        ..ItemMetadata::default()
    };
    movie.artwork.primary = tag("poster");
    movie.artwork.backdrops = tag("backdrop").into_iter().collect();
    movie.user = UserItemState::from_parts(resume, resume.map(|_| 34.0), false, false, 0);
    movie.people = vec![
        person(1, "Mara Ellis", "Ingrid Holm", Credit::Actor),
        person(2, "Tomas Reyes", "Aksel", Credit::Actor),
        person(3, "June Okafor", "The Harbormaster", Credit::Actor),
        person(4, "Leif Andersen", "Old Bjørn", Credit::Actor),
        person(5, "Sofia Brandt", "Liv", Credit::Actor),
        person(6, "Ana Duarte", "", Credit::Director),
        person(7, "Ana Duarte", "", Credit::Writer),
        person(8, "Peter Lunde", "", Credit::Writer),
    ];
    movie.chapters = [
        "Opening",
        "The Log",
        "First Warning",
        "Village Hall",
        "Night Watch",
        "The Crossing",
        "Landfall",
        "Morning",
    ]
    .iter()
    .enumerate()
    .map(|(index, name)| Chapter {
        index: index as u32,
        name: Some((*name).into()),
        start: Duration::from_secs(index as u64 * 15 * 60 + 42),
        image: None,
    })
    .collect();
    movie.media = TechnicalMedia::new(
        Vec::new(),
        vec![
            MediaStream::Video(VideoStream {
                index: 0,
                codec: Some("hevc".into()),
                title: None,
                language: None,
                width: Some(3840),
                height: Some(1608),
                range: DynamicRange::Hdr {
                    label: "HDR10".into(),
                },
                bit_depth: Some(10),
                frame_rate: Some(23.976),
                bitrate: None,
                profile: None,
                pixel_format: None,
                color_space: None,
                is_default: true,
            }),
            MediaStream::Audio(AudioStream {
                index: 1,
                codec: Some("truehd".into()),
                title: None,
                display_title: Some("English - Dolby TrueHD Atmos 7.1".into()),
                language: Some("eng".into()),
                channels: Some(8),
                channel_layout: None,
                sample_rate: None,
                bitrate: None,
                is_default: true,
            }),
        ],
    );
    movie
}

fn harbor_lights() -> MediaItem {
    let mut series = base("harbor-lights", "Harbor Lights", ItemKind::Series);
    series.metadata = ItemMetadata {
        overview: Some("Three generations of a fishing family keep the last working harbor light on the coast running, while the town around them argues about whether it should be kept at all.".into()),
        year: Some(2021),
        community_rating: Some(8.4),
        official_rating: Some("TV-14".into()),
        genres: vec!["Drama".into(), "Family".into()],
        ..ItemMetadata::default()
    };
    series.artwork.primary = tag("poster");
    series.artwork.backdrops = tag("backdrop").into_iter().collect();
    series.people = vec![
        person(1, "June Okafor", "Nell Varga", Credit::Actor),
        person(2, "Tomas Reyes", "Elias Varga", Credit::Actor),
        person(3, "Ruth Kim", "Marit", Credit::Actor),
    ];
    series
}

fn season(number: u32) -> MediaItem {
    let mut season = base(
        &format!("season-{number}"),
        &format!("Season {number}"),
        ItemKind::Season,
    );
    season.hierarchy.index = Some(number);
    season
}

const EPISODES: [(&str, &str); 6] = [
    (
        "Low Tide",
        "The light fails for the first time in forty years, and Nell has to decide who to call.",
    ),
    (
        "Ballast",
        "Elias brings home a buyer from the city. Marit refuses to let him past the gate.",
    ),
    (
        "Fog Bell",
        "A school trip goes missing in the fog, and the harbor light is the only way back.",
    ),
    (
        "Spring Tide",
        "Old arguments surface when the council votes on the harbor's future.",
    ),
    (
        "Keeper",
        "Nell finds her grandfather's logbooks and a name she has never heard.",
    ),
    (
        "Landfall",
        "The storm arrives early. Everyone ends up at the light.",
    ),
];

fn episode(season: u32, index: u32, watched: bool, resume: Option<Duration>) -> MediaItem {
    let (name, overview) = EPISODES[(index as usize - 1) % EPISODES.len()];
    let mut episode = base(&format!("ep-{index}-{season}"), name, ItemKind::Episode);
    episode.metadata.overview = Some(overview.into());
    episode.metadata.runtime = Some(Duration::from_secs((44 + u64::from(index % 3)) * 60));
    episode.hierarchy = ItemHierarchy {
        series_id: Some(id("harbor-lights")),
        series_name: Some("Harbor Lights".into()),
        season_id: Some(id(&format!("season-{season}"))),
        index: Some(index),
        parent_index: Some(season),
        ..ItemHierarchy::default()
    };
    episode.artwork.primary = tag("still");
    episode.user = UserItemState::from_parts(
        resume,
        resume.map(|_| 40.0),
        false,
        watched,
        u32::from(watched),
    );
    episode
}

fn preview_model(scene: DetailsPreview) -> DetailsModel {
    match scene {
        DetailsPreview::Loading => DetailsModel::open(id("northwind")).0,
        DetailsPreview::Error => {
            let (mut model, requests) = DetailsModel::open(id("missing-item"));
            model.apply(
                requests[0].ticket(),
                Response::Item(Err(DetailsFailure::NotFound)),
            );
            model
        }
        DetailsPreview::Movie | DetailsPreview::MovieResume => {
            let resume =
                (scene == DetailsPreview::MovieResume).then(|| Duration::from_secs(42 * 60 + 18));
            let movie = northwind(resume);
            let (mut model, requests) = DetailsModel::open(movie.id().clone());
            let follow = model.apply(requests[0].ticket(), Response::Item(Ok(movie)));
            model.apply(
                follow[0].ticket(),
                Response::Related {
                    similar: Some(vec![
                        poster("tidewater", "Tidewater", 2017),
                        poster("salt-road", "The Salt Road", 2020),
                        poster("long-winter", "Long Winter", 2015),
                        poster("signal", "Signal Fires", 2022),
                        poster("keeper", "The Keeper's Daughter", 2018),
                        poster("ferry", "Last Ferry", 2016),
                    ]),
                    collections: Some(vec![CollectionContext {
                        collection: base("trilogy", "Northwind Trilogy", ItemKind::Collection),
                        items: vec![
                            poster("southwind", "Southwind", 2021),
                            poster("eastwind", "Eastwind", 2023),
                        ],
                    }]),
                },
            );
            model
        }
        DetailsPreview::Series | DetailsPreview::Season => {
            let series = harbor_lights();
            let (mut model, requests) = DetailsModel::open(series.id().clone());
            let follow = model.apply(requests[0].ticket(), Response::Item(Ok(series)));
            let next_up = episode(2, 5, false, Some(Duration::from_secs(18 * 60 + 2)));
            let follow = model.apply(
                follow[0].ticket(),
                Response::Outline {
                    seasons: Some(vec![season(1), season(2), season(3)]),
                    next_up: Some(next_up),
                },
            );
            let season_two = (1..=6)
                .map(|index| match index {
                    1..=4 => episode(2, index, true, None),
                    5 => episode(2, 5, false, Some(Duration::from_secs(18 * 60 + 2))),
                    _ => episode(2, index, false, None),
                })
                .collect();
            model.apply(follow[0].ticket(), Response::Episodes(Some(season_two)));
            if scene == DetailsPreview::Season {
                let requests = model.select_season(id("season-1"));
                let season_one = (1..=6).map(|index| episode(1, index, true, None)).collect();
                model.apply(requests[0].ticket(), Response::Episodes(Some(season_one)));
            }
            model
        }
    }
}
