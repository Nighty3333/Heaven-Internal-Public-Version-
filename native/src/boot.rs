//! Native bootstrap.
//!
//! On DLL attach we cannot touch IL2CPP yet (GameAssembly.dll loads after our
//! DllMain), so we spawn a worker thread that:
//!   1) waits for GameAssembly.dll,
//!   2) resolves the IL2CPP C API + attaches the thread to the domain,
//!   3) installs every native module (SuperSkip, FPS, race),
//!   4) marks the engine ready.
//! From then on the game's own threads drive our hooks and the overlay renders
//! the shared state.
//!
//! A concise startup report is written to logs/heaven-native.log.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use crate::fps;
use crate::htt;
use crate::il2cpp;
use crate::ipc;
#[cfg(feature = "raceread")]
use crate::race;
use crate::settings;
use crate::skip;

fn log(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::paths::log_file("heaven-native.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

/// Spawn the native engine thread. Called once from the overlay hook init.
pub fn spawn() {
    std::thread::spawn(|| {
        log("==== Heaven native engine starting ====");
        log("Heaven MOD — made by Night DC : nighty3333");
        ipc::set_status("waiting for GameAssembly…");

        // 1) Wait for GameAssembly.dll.
        let mut waited: u64 = 0;
        while !il2cpp::game_loaded() {
            std::thread::sleep(Duration::from_millis(250));
            waited += 250;
            if waited > 180_000 {
                log("TIMEOUT: GameAssembly.dll never appeared");
                return;
            }
        }
        log(&format!("step1: GameAssembly loaded ({waited}ms)"));

        // 2) Resolve the IL2CPP exports (GetProcAddress only — no managed calls).
        if let Err(e) = il2cpp::init() {
            log(&format!("step2: il2cpp::init FAILED: {e}"));
            return;
        }
        log("step2: exports resolved");

        // 3) Wait for the IL2CPP domain to exist (domain_get = no alloc, safe).
        ipc::set_status("waiting for IL2CPP runtime…");
        let mut rwait: u64 = 0;
        while il2cpp::domain().is_null() {
            std::thread::sleep(Duration::from_millis(250));
            rwait += 250;
            if rwait > 180_000 {
                log("step3: TIMEOUT domain");
                return;
            }
        }
        log(&format!("step3: domain present ({rwait}ms)"));

        // 3b) Let the runtime/GC fully settle before we touch it. With the proxy
        //     loader we reach this point during early init; attaching into a
        //     freshly-created domain races the GC. A short settle window makes
        //     the proxy path behave like the (working) late-injection path.
        std::thread::sleep(Duration::from_secs(5));
        log("step3b: settle done");

        // 4) Attach this thread, then confirm classes resolve.
        let heaven_thread = il2cpp::attach_current_thread();
        log("step4: thread attached");
        let mut cwait: u64 = 0;
        while il2cpp::class("Gallop.ButtonCommon").is_null() {
            std::thread::sleep(Duration::from_millis(250));
            cwait += 250;
            if cwait > 60_000 {
                log("step4: TIMEOUT classes");
                return;
            }
        }
        log(&format!("step5: classes resolvable — runtime ready ({}ms total)", waited + rwait + cwait));

        // Arm the crash detector before installing our hooks, so a fault in any of them is
        // logged with a breadcrumb to heaven-crash.log.
        crate::crashlog::install();

        // Active-scene probe (gates the intro player on the title screen) + intro-song
        // audio worker + BGM mute API. `banner` build only; the video player's
        // device capture runs separately and early (from new_with_engine).
        #[cfg(feature = "banner")]
        {
            crate::startup_probe::spawn();
            crate::audio::spawn();
            crate::bgm::init();
        }

        // 3) Install modules. Each is independent; one failing never blocks the
        //    others (keeps the proven core alive even if an experimental part
        //    can't resolve on a future game patch).
        let (tr_ok, ev_ok, snotes) = skip::install();
        log(&format!("superskip: training={tr_ok} events={ev_ok} [{}]", snotes.trim_end()));
        match skip::install_race_result() {
            Ok(note) => log(&format!("race-result (off by default): armed [{}]", note.trim_end())),
            Err(e) => log(&format!("race-result: not armed ({e})")),
        }
        match fps::install() {
            Ok(()) => log("fps control: OK"),
            Err(e) => log(&format!("fps control: FAIL ({e})")),
        }
        match crate::ui_tempo::install() {
            Ok(()) => log("ui tempo: OK"),
            Err(e) => log(&format!("ui tempo: deferred ({e})")),
        }
        crate::crashlog::crumb(4);
        match crate::cyspring::install() {
            Ok(()) => log("cyspring uncap: OK"),
            Err(e) => log(&format!("cyspring uncap: deferred ({e})")),
        }
        crate::crashlog::crumb(1);
        match crate::graphics::install() {
            Ok(()) => log("graphics tweaks: OK"),
            Err(e) => log(&format!("graphics tweaks: deferred ({e})")),
        }
        crate::crashlog::crumb(2);
        match crate::display::install() {
            Ok(()) => log("display tweaks: OK"),
            Err(e) => log(&format!("display tweaks: deferred ({e})")),
        }
        crate::crashlog::crumb(3);
        crate::display::install_window();
        crate::crashlog::crumb(0);
        #[cfg(feature = "raceread")]
        log(&format!("race reader: {}", race::install()));

        #[cfg(feature = "freecam")]
        log(&format!("freecam: {}", crate::freecam::install()));

        // Player-horse identity parse from the msgpack race response, so the
        // race-result skip's "only when you WON" gate works.
        #[cfg(feature = "racenet")]
        {
            crate::race_net::install();
            log("race_net: armed (player-id only)");
        }

        // HorseTheTrails — native Team Trials capture (hooks TeamStadiumResult).
        // Runs while this boot thread is still IL2CPP-attached (scan needs it).
        log(&format!("HorseTheTrails: {}", htt::install()));

        // Apply persisted toggle state (SuperSkip / Race-result / FPS / TT).
        settings::apply_on_boot();
        log("settings: applied persisted state");

        // Self-update check (downloads a newer release DLL in the background; the
        // version.dll proxy swaps it in on the next launch).
        crate::update::auto_update();
        log("auto-update: check started");

        // Install is done. Hooks now run on the GAME's (already-attached) threads,
        // so this boot thread no longer needs to be attached. DETACH it cleanly
        // and let it exit — leaving it attached + alive made the shutdown GC
        // "collect from an unknown thread" when the game closes. Detaching
        // unregisters it from the GC so teardown is clean.
        il2cpp::detach_thread(heaven_thread);
        ipc::set_status("Heaven native engine ready");
        log("==== ready (boot thread detached) ====");
    });
}

