//! Shared in-process state store.
//!
//! Everything is in-process: the native race reader writes directly here and the
//! overlay reads from it. A plain RwLock-backed store.

use crate::data::{GameState, RaceState};
use once_cell::sync::Lazy;
use std::sync::{Mutex, RwLock};

/// The live game state.
static STATE: Lazy<RwLock<GameState>> = Lazy::new(|| RwLock::new(GameState::default()));
/// One-line engine status shown in the overlay footer.
static STATUS: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new("starting…".into()));

/// Mutate the race state in place (for incremental frame/event updates).
pub fn with_race<F: FnOnce(&mut RaceState)>(f: F) {
    if let Ok(mut g) = STATE.write() {
        f(&mut g.race);
    }
}

pub fn status() -> String {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}

pub fn set_status(s: impl Into<String>) {
    if let Ok(mut g) = STATUS.lock() {
        *g = s.into();
    }
}
