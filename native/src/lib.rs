//! Heaven MOD — in-game overlay DLL entry point.
//!
//! On attach we install the D3D11 + imgui overlay via hudhook. From then on the
//! game's own render thread calls our `HeavenOverlay::render`, drawing the HUD
//! inside the swapchain — true in-game rendering, no external window.

// Intro player support (native song playback, original-BGM mute, title-scene probe).
// Gated with the `banner` feature, like the video player itself.
#[cfg(feature = "banner")]
mod audio;
#[cfg(feature = "banner")]
mod bgm;
mod boot;
mod crashlog;
mod cyspring;
mod data;
mod display;
#[cfg(feature = "freecam")]
mod freecam;
mod fps;
mod graphics;
mod hooks;
mod il2cpp;
#[cfg(feature = "banner")]
mod intro_player;
mod htt;
mod htt_il2cpp;
mod ipc;
mod menu_model;
mod overlay;
mod paths;
// Live race reader (Race panel + race-result win-gate). The race-result skip
// needs finish placement.
#[cfg(feature = "raceread")]
mod race;
// Player-horse identity from the network response (msgpack).
#[cfg(feature = "racenet")]
mod race_net;
mod settings;
mod skip;
#[cfg(feature = "banner")]
mod startup_probe;
mod ui_tempo;
mod update;

use hudhook::hooks::dx11::ImguiDx11Hooks;

use overlay::HeavenOverlay;

hudhook::hudhook!(ImguiDx11Hooks, HeavenOverlay::new_with_engine());

impl HeavenOverlay {
    /// Construct the render loop and start the native engine. The engine thread
    /// waits for GameAssembly, resolves IL2CPP, installs every native module
    /// (SuperSkip, FPS, race), and publishes into the shared state the overlay
    /// renders.
    pub fn new_with_engine() -> Self {
        boot::spawn();
        // Start the video player's D3D11 device capture early (independent of the IL2CPP
        // boot) so the intro can draw over the splash logos within ~1 s of launch.
        #[cfg(feature = "banner")]
        intro_player::spawn_capture();
        HeavenOverlay::new()
    }
}
