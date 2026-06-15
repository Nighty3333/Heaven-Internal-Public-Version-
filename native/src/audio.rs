//! Heaven — native intro-song playback.
//!
//! Plays the intro track while the video runs (title scene), stops on skip/leave. The OGG
//! is read from `intro_song.ogg` next to the DLL (same drop-in model as `intro_full.bin`),
//! so swapping the intro never needs a rebuild. rodio's OutputStream is `!Send`, so a
//! dedicated thread owns the device + sink and obeys commands posted via an atomic.

#![allow(dead_code)]

use std::io::Cursor;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rodio::Source; // for `.repeat_infinite()` — the song loops with the looping video

/// The wall-clock instant at which the intro song actually began playing (device open + first
/// sample queued). The video player uses THIS as its frame-clock origin so the two stay locked
/// — the video holds frame 0 until audio truly starts, then both advance together. `None` while
/// not playing.
static START_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// When the intro audio actually started (for the video to sync its clock to). `None` if not
/// playing yet.
pub fn playback_start() -> Option<Instant> {
    START_AT.lock().ok().and_then(|g| *g)
}

/// The intro track, read once from `intro_song.ogg` next to the DLL. `None` if absent
/// (the video then plays without our audio).
fn song() -> Option<&'static [u8]> {
    static SONG: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    SONG.get_or_init(|| std::fs::read(crate::paths::local_file("intro_song.ogg")).ok())
        .as_deref()
}

// 0 = no command, 1 = play (from start), 2 = stop.
static CMD: AtomicU8 = AtomicU8::new(0);

pub fn play() {
    CMD.store(1, Ordering::Relaxed);
}
pub fn stop() {
    CMD.store(2, Ordering::Relaxed);
}

fn log(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true)
        .open(crate::paths::log_file("heaven-native.log")) { let _ = writeln!(f, "{msg}"); }
}

/// Spawn the audio worker. The output device (and its WASAPI/COM backend thread) is
/// opened ONLY while the intro song plays and dropped the moment it stops or finishes.
/// A rodio `OutputStream` kept alive for the whole process never gets dropped cleanly,
/// so its backend thread deadlocks during `ExitProcess` → the game hangs on close.
/// Scoping the stream to playback means that by the time the user quits (normally
/// outside the title) no device is open and shutdown is clean.
pub fn spawn() {
    std::thread::spawn(|| {
        // (stream, _handle, sink) dropped together when playback ends.
        let mut active: Option<(rodio::OutputStream, rodio::OutputStreamHandle, rodio::Sink)> =
            None;
        loop {
            match CMD.swap(0, Ordering::Relaxed) {
                1 => {
                    active = None; // drop any previous stream first
                    let Some(bytes) = song() else {
                        // No song: still mark a start instant so the video plays (silently).
                        if let Ok(mut g) = START_AT.lock() { *g = Some(Instant::now()); }
                        log("[audio] no intro_song.ogg — skipping audio");
                        continue;
                    };
                    match rodio::OutputStream::try_default() {
                        Ok((stream, handle)) => match rodio::Sink::try_new(&handle) {
                            Ok(sink) => match rodio::Decoder::new(Cursor::new(bytes)) {
                                Ok(src) => {
                                    // The intro video loops (worker wraps frames), so loop the
                                    // song too — otherwise the audio falls silent after one pass.
                                    sink.append(src.repeat_infinite());
                                    // Mark the true playback origin AFTER device open + queue, so
                                    // the video clock aligns to when sound actually starts.
                                    if let Ok(mut g) = START_AT.lock() { *g = Some(Instant::now()); }
                                    active = Some((stream, handle, sink));
                                    log("[audio] play");
                                }
                                Err(e) => log(&format!("[audio] decode err: {e}")),
                            },
                            Err(e) => log(&format!("[audio] sink err: {e}")),
                        },
                        Err(e) => log(&format!("[audio] no output device: {e}")),
                    }
                }
                2 => {
                    if let Ok(mut g) = START_AT.lock() { *g = None; }
                    if active.take().is_some() {
                        log("[audio] stop");
                    }
                }
                _ => {
                    // Song finished on its own → close the device so it can't linger
                    // and block process shutdown later.
                    if let Some((_, _, sink)) = active.as_ref() {
                        if sink.empty() {
                            active = None;
                            log("[audio] finished");
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    });
}
