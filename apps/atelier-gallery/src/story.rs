//! The Gallery's story registry.
//!
//! Adding a demo = writing one `fn(&mut Window, &mut App) -> AnyElement`
//! and listing it in [`STORIES`]. Preview settings (theme, reduced motion)
//! are global, so every story automatically reacts to them.

use atelier_ui::prelude::*;

use crate::stories;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Foundations,
    Components,
}

impl Section {
    pub const ALL: [Section; 2] = [Section::Foundations, Section::Components];

    pub const fn title(self) -> &'static str {
        match self {
            Section::Foundations => "Foundations",
            Section::Components => "Components",
        }
    }
}

pub type RenderStory = fn(&mut Window, &mut App) -> AnyElement;

#[derive(Clone, Copy)]
pub struct Story {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub section: Section,
    pub render: RenderStory,
}

impl std::fmt::Debug for Story {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Story").field("id", &self.id).finish()
    }
}

pub const STORIES: &[Story] = &[
    Story {
        id: "colors",
        title: "Color",
        summary: "Semantic color roles. Components read only these; themes decide the values.",
        section: Section::Foundations,
        render: stories::colors::render,
    },
    Story {
        id: "typography",
        title: "Typography",
        summary: "Text roles map to family, size, line height, and weight from the active theme.",
        section: Section::Foundations,
        render: stories::typography::render,
    },
    Story {
        id: "spacing",
        title: "Spacing",
        summary: "A 4px-based scale. Layout never uses raw pixel values.",
        section: Section::Foundations,
        render: stories::spacing::render,
    },
    Story {
        id: "shape",
        title: "Radius & Elevation",
        summary: "Corner radii and layered shadows, both themable.",
        section: Section::Foundations,
        render: stories::shape::render,
    },
    Story {
        id: "motion",
        title: "Motion",
        summary: "Duration and spring tokens. Reduced motion turns movement into an immediate state change.",
        section: Section::Foundations,
        render: stories::motion::render,
    },
    Story {
        id: "icons",
        title: "Icons",
        summary: "Built-in generic interface glyphs, tinted by the current text color.",
        section: Section::Foundations,
        render: stories::icons::render,
    },
    Story {
        id: "button",
        title: "Button",
        summary: "Labelled actions. Hover, press, Tab to focus, Enter or Space to activate.",
        section: Section::Components,
        render: stories::buttons::render,
    },
    Story {
        id: "icon-button",
        title: "Icon Button",
        summary: "Square, icon-only actions with a mandatory label (tooltip and accessible name).",
        section: Section::Components,
        render: stories::icon_buttons::render,
    },
    Story {
        id: "text-field",
        title: "Text Field",
        summary: "Single-line editing through GPUI's text input handler, including caret, selection, and IME.",
        section: Section::Components,
        render: stories::text_field::render,
    },
    Story {
        id: "search-field",
        title: "Search Field",
        summary: "A text field with a search icon, a clear button, and Escape to clear or dismiss.",
        section: Section::Components,
        render: stories::search_field::render,
    },
    Story {
        id: "switch",
        title: "Switch",
        summary: "On/off control. Pointer and keyboard activation. The thumb uses the snappy spring.",
        section: Section::Components,
        render: stories::switch::render,
    },
    Story {
        id: "checkbox",
        title: "Checkbox",
        summary: "Unchecked, checked, and indeterminate. Activation resolves indeterminate to checked.",
        section: Section::Components,
        render: stories::checkbox::render,
    },
    Story {
        id: "segmented-control",
        title: "Segmented Control",
        summary: "One tab stop. Arrow keys move the selection and skip disabled segments.",
        section: Section::Components,
        render: stories::segmented::render,
    },
    Story {
        id: "slider",
        title: "Slider",
        summary: "A generic value control with dragging, stepping, and keyboard adjustment.",
        section: Section::Components,
        render: stories::slider::render,
    },
    Story {
        id: "scroll-view",
        title: "Scroll View",
        summary: "A clipping scroll container for vertical or horizontal content.",
        section: Section::Components,
        render: stories::composition::scroll_view,
    },
    Story {
        id: "list",
        title: "List",
        summary: "Single selection, disabled rows, keyboard movement, and a long list.",
        section: Section::Components,
        render: stories::composition::list,
    },
    Story {
        id: "image",
        title: "Image",
        summary: "Fixed frames with fit, fill, a loading placeholder, and a failure fallback.",
        section: Section::Components,
        render: stories::composition::image,
    },
    Story {
        id: "external-frame",
        title: "External Frame",
        summary: "A latest BGRA picture from outside the framework. Fit letterboxes. Fill covers. Nothing here knows about codecs.",
        section: Section::Components,
        render: stories::external_frame::render,
    },
    Story {
        id: "progress",
        title: "Progress",
        summary: "Determinate and indeterminate meters. Reduced motion holds the indeterminate segment still.",
        section: Section::Components,
        render: stories::composition::progress,
    },
    Story {
        id: "tooltip",
        title: "Tooltip",
        summary: "Text tips on hover. They wait before appearing and do not take focus.",
        section: Section::Components,
        render: stories::composition::tooltip,
    },
    Story {
        id: "popover",
        title: "Popover",
        summary: "An anchored layer that closes on Escape or an outside press and restores focus.",
        section: Section::Components,
        render: stories::composition::popover,
    },
    Story {
        id: "menu",
        title: "Menu",
        summary: "Items, separators, disabled and destructive rows, and keyboard activation.",
        section: Section::Components,
        render: stories::composition::menu,
    },
    Story {
        id: "context-menu",
        title: "Context Menu",
        summary: "The same menu, opened at a secondary-click position.",
        section: Section::Components,
        render: stories::composition::context_menu,
    },
    Story {
        id: "dialog",
        title: "Dialog",
        summary: "A modal with focus containment. Destructive confirmation focuses Cancel.",
        section: Section::Components,
        render: stories::composition::dialog,
    },
    Story {
        id: "sidebar",
        title: "Sidebar",
        summary: "Sectioned navigation with icons, a disabled item, and a footer.",
        section: Section::Components,
        render: stories::composition::sidebar,
    },
    Story {
        id: "toolbar",
        title: "Toolbar",
        summary: "Leading tools, a centered title, and trailing actions. No window chrome.",
        section: Section::Components,
        render: stories::composition::toolbar,
    },
    Story {
        id: "split-view",
        title: "Split View",
        summary: "A sized leading pane, a flexible trailing pane, and a draggable divider.",
        section: Section::Components,
        render: stories::composition::split_view,
    },
    Story {
        id: "empty-state",
        title: "Empty State",
        summary: "A restrained explanation with optional actions when a region has nothing to list.",
        section: Section::Components,
        render: stories::composition::empty_state,
    },
    Story {
        id: "desktop",
        title: "Desktop Composition",
        summary: "A documents window: toolbar, sidebar, split view, searchable list, menus, and a dialog.",
        section: Section::Components,
        render: stories::desktop::render,
    },
    Story {
        id: "text",
        title: "Text",
        summary: "Role and tone are semantic; color overrides are an escape hatch.",
        section: Section::Components,
        render: stories::text::render,
    },
    Story {
        id: "surface",
        title: "Surface",
        summary: "Themed containers for grouping content at a given level.",
        section: Section::Components,
        render: stories::surfaces::render,
    },
];

pub fn find(id: &str) -> Option<&'static Story> {
    STORIES.iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_ids_are_unique() {
        let mut ids: Vec<_> = STORIES.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), STORIES.len());
    }

    #[test]
    fn every_section_has_stories() {
        for section in Section::ALL {
            assert!(STORIES.iter().any(|s| s.section == section), "{section:?}");
        }
    }
}
