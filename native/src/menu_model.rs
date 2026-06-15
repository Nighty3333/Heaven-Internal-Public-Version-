//! menu_model — the SINGLE source of truth for the overlay menu's controls.
//!
//! Both renderers (premium `overlay::draw_menu` and the classic `overlay::draw_controls`)
//! consume `model()`. Neither defines its own control list any more, so the two menus can
//! no longer drift apart — adding or moving a control is a one-line edit here and it shows
//! up in both styles automatically.
//!
//! This file is LOGIC ONLY. Every premium visual (Cinzel/Inter/Orbitron fonts, glass icons,
//! sakura petals, the silhouette, animated cards + pills, background textures) lives in the
//! renderers and is untouched. The model just says WHAT controls exist and HOW they're wired
//! to their getters/setters; each renderer decides HOW to draw them.
//!
//! Anything too bespoke for a generic widget (the freecam follow/preset panel, the tri-state
//! FPS card, the intro/updates/about blocks) is represented by `Ctrl::Custom(..)` and drawn by
//! the renderer's existing hand-written code — so nothing premium is lost.

#![allow(dead_code)]

/// One control in a section. Getters/setters are plain module fns (no captures).
pub enum Ctrl {
    /// On/off. Renderer flips it: `set(!get())` then persists.
    Toggle {
        id: &'static str,
        label: &'static str,
        get: fn() -> bool,
        set: fn(bool),
    },
    /// Float slider with a unit suffix for the readout (e.g. "x").
    SliderF32 {
        id: &'static str,
        label: &'static str,
        min: f32,
        max: f32,
        get: fn() -> f32,
        set: fn(f32),
        unit: &'static str,
    },
    /// Cycles through a fixed set of states on click (e.g. screen mode).
    Cycle {
        id: &'static str,
        label: &'static str,
        label_of: fn() -> &'static str,
        next: fn(),
    },
    /// A plain action button.
    Button {
        id: &'static str,
        label: &'static str,
        action: fn(),
    },
    /// Static descriptive line under a section header.
    Note(&'static str),
    /// Hand-drawn block the renderer dispatches to its own bespoke code (preserves
    /// every premium custom widget unchanged).
    Custom(Custom),
}

/// Bespoke blocks each renderer draws with its existing code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Custom {
    Fps,             // tri-state cap / unlimited / slider + live readout
    Freecam,         // enable + follow controls + preset manager
    TeamTrials,      // capture toggle + "N saved" count
    Intro,           // intro status + replay button
    Updates,         // version + check/pull/releases
    AboutLayout,     // centered / dock side / toggle-key rebind / classic-menu toggle
    Credits,         // Ko-fi / GitHub / changelog / version / author
}

pub struct Section {
    pub title: &'static str,
    /// MDL2 glyph (drawn with the icon font by the premium renderer; classic ignores it).
    /// Distinct per section — replaces the repeated gear icon. Tweak freely.
    pub icon: char,
    pub blurb: &'static str,
    pub controls: Vec<Ctrl>,
}

pub struct Tab {
    pub name: &'static str,
    pub icon: char,
    pub sections: Vec<Section>,
}

// ── compound actions that need a wrapper (fn pointers can't capture) ──────────
fn cycle_display_mode() {
    crate::settings::set_display_mode((crate::settings::display_mode() + 1) % 4);
}
fn display_mode_label() -> &'static str {
    match crate::settings::display_mode() {
        1 => "Borderless",
        2 => "Exclusive",
        3 => "Windowed",
        _ => "Default",
    }
}

/// Build the whole menu. Order = display order. cfg-gated controls simply aren't pushed
/// in builds that lack the feature, so both renderers stay correct per build.
pub fn model() -> Vec<Tab> {
    let mut tabs: Vec<Tab> = Vec::new();

    // ── 1) GAMEPLAY ──────────────────────────────────────────────────────────
    #[allow(unused_mut)]
    let mut gameplay = vec![
        Section {
            title: "Superskip",
            icon: '\u{E768}',
            blurb: "Skip events, training cut-ins and race results.",
            controls: vec![
                Ctrl::Toggle { id: "ev", label: "Events", get: crate::skip::is_event_enabled, set: crate::skip::set_event_enabled },
                Ctrl::Toggle { id: "tr", label: "Training", get: crate::skip::is_train_enabled, set: crate::skip::set_train_enabled },
                Ctrl::Toggle { id: "rr", label: "Races (won only)", get: crate::skip::is_race_result_enabled, set: crate::skip::set_race_result_enabled },
            ],
        },
        Section {
            title: "Game speed",
            icon: '\u{E916}',
            blurb: "Speed up UI animations and story / event text.",
            controls: vec![Ctrl::SliderF32 {
                id: "speed", label: "Speed", min: 1.0, max: 10.0,
                get: crate::ui_tempo::tempo, set: crate::ui_tempo::set_tempo, unit: "x",
            }],
        },
    ];
    tabs.push(Tab { name: "Gameplay", icon: '\u{E768}', sections: gameplay });

    // ── 2) CAMERA ────────────────────────────────────────────────────────────
    #[cfg(feature = "freecam")]
    tabs.push(Tab {
        name: "Camera",
        icon: '\u{E722}',
        sections: vec![Section {
            title: "Free camera",
            icon: '\u{E722}',
            blurb: "3rd-person race camera with per-circuit presets.",
            controls: vec![Ctrl::Custom(Custom::Freecam)],
        }],
    });

    // ── 3) VISUALS ───────────────────────────────────────────────────────────
    tabs.push(Tab {
        name: "Visuals",
        icon: '\u{E790}',
        sections: vec![
            Section {
                title: "Graphics",
                icon: '\u{E790}',
                blurb: "Force full 3D model quality, beyond the in-game cap.",
                controls: vec![
                    Ctrl::Toggle { id: "gq", label: "Max 3D quality", get: crate::settings::gfx_quality, set: crate::settings::set_gfx_quality },
                    Ctrl::Toggle { id: "ge", label: "Enhanced textures & shadows", get: crate::settings::gfx_extras, set: crate::settings::set_gfx_extras },
                    Ctrl::Note("Applies on the next scene / character load."),
                ],
            },
            Section {
                title: "Cloth physics",
                icon: '\u{EA86}',
                blurb: "Uncap hair / cloth physics so they stay smooth at high FPS.",
                controls: vec![Ctrl::Toggle { id: "cyspring", label: "Uncap cloth physics", get: crate::settings::cyspring_uncap, set: crate::settings::set_cyspring_uncap }],
            },
        ],
    });

    // ── 4) PERFORMANCE ───────────────────────────────────────────────────────
    tabs.push(Tab {
        name: "Performance",
        icon: '\u{E9D9}',
        sections: vec![
            Section {
                title: "Low resources mode",
                icon: '\u{E950}',
                blurb: "Potato mode: lowest quality, no shadows/AA, lighter physics.",
                controls: vec![
                    Ctrl::Toggle { id: "lowspec", label: "Low resources mode", get: crate::settings::low_spec, set: crate::settings::set_low_spec },
                    Ctrl::Note("Overrides Visuals. Applies on next scene load."),
                ],
            },
            Section {
                title: "Frame rate",
                icon: '\u{E9D9}',
                blurb: "",
                controls: vec![Ctrl::Custom(Custom::Fps)],
            },
        ],
    });

    // ── 5) INTERFACE ─────────────────────────────────────────────────────────
    let mut interface = Vec::new();
    interface.push(Section {
        title: "Window",
        icon: '\u{E737}',
        blurb: "",
        controls: vec![
            Ctrl::Toggle { id: "aot", label: "Always on top", get: crate::settings::always_on_top, set: crate::settings::set_always_on_top },
            Ctrl::Toggle { id: "bm", label: "Block minimize", get: crate::settings::block_minimize, set: crate::settings::set_block_minimize },
            Ctrl::Cycle { id: "dm", label: "Screen mode", label_of: display_mode_label, next: cycle_display_mode },
        ],
    });
    interface.push(Section {
        title: "Layout",
        icon: '\u{E8A1}',
        blurb: "",
        controls: vec![Ctrl::Custom(Custom::AboutLayout)],
    });
    #[cfg(feature = "banner")]
    interface.push(Section {
        title: "Intro video",
        icon: '\u{E714}',
        blurb: "",
        controls: vec![Ctrl::Custom(Custom::Intro)],
    });
    tabs.push(Tab { name: "Interface", icon: '\u{E8A9}', sections: interface });

    // ── 6) ABOUT ─────────────────────────────────────────────────────────────
    #[allow(unused_mut)]
    let mut about = vec![
        Section {
            title: "Team Trials",
            icon: '\u{E74E}',
            blurb: "Saved results are read by the Heaven dashboard.",
            controls: vec![Ctrl::Custom(Custom::TeamTrials)],
        },
        Section {
            title: "Updates",
            icon: '\u{E72C}',
            blurb: "",
            controls: vec![Ctrl::Custom(Custom::Updates)],
        },
        Section {
            title: "About",
            icon: '\u{E946}',
            blurb: "",
            controls: vec![Ctrl::Custom(Custom::Credits)],
        },
    ];
    tabs.push(Tab { name: "About", icon: '\u{E946}', sections: about });

    tabs
}
