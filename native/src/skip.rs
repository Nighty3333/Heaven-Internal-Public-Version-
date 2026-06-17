//! Heaven — native SuperSkip.
//!
//! This covers the two "bread and butter" skips:
//!   1) TRAINING cut-in  → SingleModeTrainingCutInHelper.SkipRuntime()
//!   2) EVENTS/recreation/Rest → StoryViewController.SkipStory() (guarded by
//!      trainCutt / TimelineController.IsPlaying + a 1200ms debounce)
//! plus race-result auto-advance.
//!
//! Every method we INVOKE is called via its compiled methodPointer with the
//! trailing hidden MethodInfo* arg. Every method we HOOK guards against logical
//! recursion with hooks::ReentryGuard / in_heaven().

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use retour::RawDetour;

use crate::hooks::{in_heaven, ReentryGuard};
use crate::il2cpp;

const EVENT_DEBOUNCE_MS: u64 = 1200;

// ── enable flags — SuperSkip is split into Training and Events so the overlay
//    can toggle each leg independently (Races = race-result auto, further down).
static SKIP_ENABLED: AtomicBool = AtomicBool::new(true); // TRAINING cut-ins
static EVENT_ENABLED: AtomicBool = AtomicBool::new(true); // EVENT / story timelines
static SHOP_ENABLED: AtomicBool = AtomicBool::new(true); // PRO SHOP buy/use animations
static RIVAL_ENABLED: AtomicBool = AtomicBool::new(true); // rival-race entry "RIVAL <name>" card

// Training
pub fn set_enabled(on: bool) {
    SKIP_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_enabled() -> bool {
    SKIP_ENABLED.load(Ordering::Relaxed)
}
pub fn set_train_enabled(on: bool) {
    SKIP_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_train_enabled() -> bool {
    SKIP_ENABLED.load(Ordering::Relaxed)
}
// Events
pub fn set_event_enabled(on: bool) {
    EVENT_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_event_enabled() -> bool {
    EVENT_ENABLED.load(Ordering::Relaxed)
}
// Pro Shop (buy/use performance animations)
pub fn set_shop_enabled(on: bool) {
    SHOP_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_shop_enabled() -> bool {
    SHOP_ENABLED.load(Ordering::Relaxed)
}
// Rival-race entry card ("RIVAL <name>" splash before a rival race)
pub fn set_rival_enabled(on: bool) {
    RIVAL_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_rival_enabled() -> bool {
    RIVAL_ENABLED.load(Ordering::Relaxed)
}

// ── method ABIs (this, MethodInfo*) ─────────────────────────────────────────
type VoidM = unsafe extern "C" fn(*mut c_void, *mut c_void);
type PtrM = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
type BoolM = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;

/// A resolved invokable method: (compiled code ptr, MethodInfo*).
#[derive(Clone, Copy)]
struct Invokable {
    code: usize,
    mi: usize,
}
impl Invokable {
    const NONE: Invokable = Invokable { code: 0, mi: 0 };
    fn ok(&self) -> bool {
        self.code != 0
    }
    unsafe fn call_void(&self, this: *mut c_void) {
        if self.code != 0 {
            let f: VoidM = std::mem::transmute(self.code);
            f(this, self.mi as *mut c_void);
        }
    }
    unsafe fn call_ptr(&self, this: *mut c_void) -> *mut c_void {
        if self.code == 0 {
            return std::ptr::null_mut();
        }
        let f: PtrM = std::mem::transmute(self.code);
        f(this, self.mi as *mut c_void)
    }
    unsafe fn call_bool(&self, this: *mut c_void) -> bool {
        if self.code == 0 {
            return false;
        }
        let f: BoolM = std::mem::transmute(self.code);
        f(this, self.mi as *mut c_void)
    }
}

fn resolve(klass: il2cpp::Class, name: &str, argc: i32) -> Invokable {
    let m = il2cpp::method(klass, name, argc);
    if m.is_null() {
        return Invokable::NONE;
    }
    let code = il2cpp::method_pointer(m) as usize;
    Invokable { code, mi: m as usize }
}

// Invokables (set in install).
static SKIP_RUNTIME: OnceLock<Invokable> = OnceLock::new(); // training
static SKIP_STORY: OnceLock<Invokable> = OnceLock::new(); // events
static GET_TL: OnceLock<Invokable> = OnceLock::new(); // StoryViewController.get_TimelineController
static IS_PLAYING: OnceLock<Invokable> = OnceLock::new(); // StoryTimelineController.get_IsPlaying
static TRAIN_CUTT: OnceLock<Invokable> = OnceLock::new(); // get_IsPlayingOrWillPlayTrainingCutt

// Stats.
static TRAIN_SKIPS: AtomicU64 = AtomicU64::new(0);
static EVENT_SKIPS: AtomicU64 = AtomicU64::new(0);
pub fn stats() -> (u64, u64) {
    (TRAIN_SKIPS.load(Ordering::Relaxed), EVENT_SKIPS.load(Ordering::Relaxed))
}

fn clock() -> &'static Instant {
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    CLOCK.get_or_init(Instant::now)
}

// ── trampolines (keep detours alive + call originals) ───────────────────────
macro_rules! hook_slot {
    ($tramp:ident, $det:ident) => {
        static $tramp: AtomicUsize = AtomicUsize::new(0);
        static $det: OnceLock<RawDetour> = OnceLock::new();
    };
}
hook_slot!(TR_START, D_START);
hook_slot!(TR_PLAY, D_PLAY);
hook_slot!(TR_MAIN, D_MAIN);
hook_slot!(TR_TIMELINE, D_TIMELINE);
hook_slot!(TR_TAGIN, D_TAGIN); // SingleModeMainViewTagTrainingCutInPlayer.PlayCutIn
hook_slot!(TR_TAGOUT, D_TAGOUT); // .PlayCutInOut

#[inline]
unsafe fn call_orig(tramp: &AtomicUsize, this: *mut c_void, method: *mut c_void) {
    let t = tramp.load(Ordering::Relaxed);
    if t != 0 {
        let f: VoidM = std::mem::transmute(t);
        f(this, method);
    }
}

// ── TRAINING: run SkipRuntime after a cut-in start. ─────────────────────────
fn do_training_skip(this: *mut c_void) {
    if !is_enabled() || in_heaven() || this.is_null() {
        return;
    }
    if let Some(sr) = SKIP_RUNTIME.get() {
        if sr.ok() {
            let _g = ReentryGuard::enter();
            unsafe { sr.call_void(this) };
            TRAIN_SKIPS.fetch_add(1, Ordering::Relaxed);
        }
    }
}
unsafe extern "C" fn on_start_cutin(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_START, this, m);
    do_training_skip(this);
}
unsafe extern "C" fn on_play_cutin(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_PLAY, this, m);
    do_training_skip(this);
}
unsafe extern "C" fn on_play_main_cutin(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_MAIN, this, m);
    do_training_skip(this);
}

// ── EVENTS: SkipStory on OnStartPlayingTimeline (guarded + debounced). ──────
static LAST_EVENT_SKIP_MS: AtomicU64 = AtomicU64::new(0);
fn try_event_skip(this: *mut c_void) {
    if !is_event_enabled() || in_heaven() || this.is_null() {
        return;
    }
    let now = clock().elapsed().as_millis() as u64;
    if now.wrapping_sub(LAST_EVENT_SKIP_MS.load(Ordering::Relaxed)) < EVENT_DEBOUNCE_MS {
        return;
    }
    // Guard the whole critical section: any re-entry into our hooks passes thru.
    let _g = ReentryGuard::enter();
    unsafe {
        // Don't skip while a training cut-in is (or will be) playing.
        if let Some(tc) = TRAIN_CUTT.get() {
            if tc.ok() && tc.call_bool(this) {
                return;
            }
        }
        // Only skip when a timeline is actually playing.
        if let (Some(gtl), Some(isp)) = (GET_TL.get(), IS_PLAYING.get()) {
            if gtl.ok() && isp.ok() {
                let tl = gtl.call_ptr(this);
                if tl.is_null() || !isp.call_bool(tl) {
                    return;
                }
            }
        }
        if let Some(ss) = SKIP_STORY.get() {
            if ss.ok() {
                ss.call_void(this);
                LAST_EVENT_SKIP_MS.store(now, Ordering::Relaxed);
                EVENT_SKIPS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
unsafe extern "C" fn on_start_timeline(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_TIMELINE, this, m);
    try_event_skip(this);
}

// ── TAG (friendship/rainbow) TRAINING cut-in splash ─────────────────────────
// The "FRIENDSHIP TRAINING!" splash is SingleModeMainViewTagTrainingCutInPlayer.
// PlayCutIn(List<SupportCardData>, Action onDone). We skip the ~1.5s animation by
// firing the onDone callback immediately (so the turn proceeds with no splash).
// Gated by the training-skip toggle.
type TagInFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void);
type TagOutFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);

// Strategy: let the ORIGINAL PlayCutIn run (so its setup + the later PlayCutInOut work,
// no freeze) but fire its onDone callback EARLY (deferred to the next frame, on a clean
// stack) so the flow advances immediately instead of waiting ~1.1s for the splash. The
// original's own later onDone re-fires — assumed idempotent (it just unblocks the
// execute coroutine). Net: friendship training skips ~as fast as a normal one.
static ACTION_INVOKE: OnceLock<Invokable> = OnceLock::new(); // System.Action.Invoke
static SET_ACTIVE: OnceLock<Invokable> = OnceLock::new(); // GameObject.SetActive(bool)
static PENDING_TAG_CB: AtomicUsize = AtomicUsize::new(0);
const O_TAG_ROOT: usize = 0x60; // SingleModeMainViewTagTrainingCutInPlayer._tagCutInRootObject

// GameObject.SetActive takes a bool arg → fn(this, bool, MethodInfo*).
type SetActiveFn = unsafe extern "C" fn(*mut c_void, bool, *mut c_void);

unsafe fn hide_tag_visual(this: *mut c_void) {
    if this.is_null() {
        return;
    }
    let go = *((this as usize + O_TAG_ROOT) as *const *mut c_void);
    if go.is_null() {
        return;
    }
    if let Some(sa) = SET_ACTIVE.get() {
        if sa.ok() {
            let f: SetActiveFn = std::mem::transmute(sa.code);
            f(go, false, sa.mi as *mut c_void);
        }
    }
}

unsafe extern "C" fn on_tag_play_cutin(this: *mut c_void, cards: *mut c_void, cb: *mut c_void, m: *mut c_void) {
    let t = TR_TAGIN.load(Ordering::Relaxed);
    if t != 0 {
        let f: TagInFn = std::mem::transmute(t);
        f(this, cards, cb, m); // full original flow — keeps state valid for PlayCutInOut
    }
    if is_enabled() && !in_heaven() && !cb.is_null() {
        hide_tag_visual(this); // hide the "FRIENDSHIP TRAINING!" splash content (no flicker)
        PENDING_TAG_CB.store(cb as usize, Ordering::Relaxed); // fire onDone early next frame
        TRAIN_SKIPS.fetch_add(1, Ordering::Relaxed);
    }
}
unsafe extern "C" fn on_tag_play_cutin_out(this: *mut c_void, cb: *mut c_void, m: *mut c_void) {
    let t = TR_TAGOUT.load(Ordering::Relaxed);
    if t != 0 {
        let f: TagOutFn = std::mem::transmute(t);
        f(this, cb, m);
    }
    if is_enabled() && !in_heaven() && !cb.is_null() {
        PENDING_TAG_CB.store(cb as usize, Ordering::Relaxed);
    }
}

unsafe fn fire_action(cb: usize) {
    if cb == 0 {
        return;
    }
    if let Some(inv) = ACTION_INVOKE.get() {
        if inv.ok() {
            let _g = ReentryGuard::enter();
            inv.call_void(cb as *mut c_void);
        }
    }
}

/// Fire deferred callbacks (friendship onDone + shop buy/use perf callbacks) on a clean
/// main-thread frame (from the ButtonCommon.Update tick), avoiding re-entrancy.
fn pump_pending_tag_cb() {
    unsafe { fire_action(PENDING_TAG_CB.swap(0, Ordering::Relaxed)) };
    let cbs: Vec<usize> = shop_pending().lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
    for c in cbs {
        unsafe { fire_action(c) };
    }
}

// ── PRO SHOP (scenario free shop) buy/use animation skip ────────────────────
// BUY (and buy→use-now): SingleModeScenarioFreeShopViewController.PlayUseItemPerformanceCore
// (items, Action, Action) plays the flourish; the item effect is already applied server-side,
// so skip the visual and defer-fire its callbacks. PlayCharaMessage(Queue<Trigger>) is the
// "Use <item>" chara card (no callback — just don't start it). USE-from-inventory: the card
// is the PartsSingleModeScenarioFreeUseItemPerformance coroutine, which we drive to completion
// in one frame (on_movenext) so the game does its own teardown/continuation but nothing renders.
static SHOP_PENDING: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
fn shop_pending() -> &'static Mutex<Vec<usize>> {
    SHOP_PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

hook_slot!(TR_SHOPPERF, D_SHOPPERF);
type ShopPerfFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void, *mut c_void);
unsafe extern "C" fn on_shop_perf(this: *mut c_void, items: *mut c_void, cb1: *mut c_void, cb2: *mut c_void, m: *mut c_void) {
    if is_shop_enabled() && !in_heaven() {
        if let Ok(mut q) = shop_pending().lock() {
            if !cb1.is_null() {
                q.push(cb1 as usize);
            }
            if !cb2.is_null() {
                q.push(cb2 as usize);
            }
        }
        EVENT_SKIPS.fetch_add(1, Ordering::Relaxed);
        return; // skip the visual; callbacks fire next frame
    }
    let t = TR_SHOPPERF.load(Ordering::Relaxed);
    if t != 0 {
        let f: ShopPerfFn = std::mem::transmute(t);
        f(this, items, cb1, cb2, m);
    }
}

hook_slot!(TR_CHARAMSG, D_CHARAMSG);
type CharaMsgFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);
unsafe extern "C" fn on_chara_msg(this: *mut c_void, q: *mut c_void, m: *mut c_void) {
    if is_shop_enabled() && !in_heaven() {
        EVENT_SKIPS.fetch_add(1, Ordering::Relaxed);
        return; // skip the "Use <item>" chara-message card (arg is data, not a callback)
    }
    let t = TR_CHARAMSG.load(Ordering::Relaxed);
    if t != 0 {
        let f: CharaMsgFn = std::mem::transmute(t);
        f(this, q, m);
    }
}

// Drive the use-item performance coroutine to completion on its first MoveNext: the game runs
// its own cleanup (close dialogs, lift the input block) + SingleMode continuation, we just
// collapse the inter-step visual waits so nothing renders. (External replication is impossible
// — the continuation lives inside the coroutine.) Do NOT skip PlayUseItemPerformance itself.
hook_slot!(TR_MOVENEXT, D_MOVENEXT);
static DRIVING: AtomicBool = AtomicBool::new(false);
type BoolMethodFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
unsafe extern "C" fn on_movenext(this: *mut c_void, m: *mut c_void) -> bool {
    let t = TR_MOVENEXT.load(Ordering::Relaxed);
    if t == 0 {
        return false;
    }
    let f: BoolMethodFn = std::mem::transmute(t);
    if !is_shop_enabled() || in_heaven() || DRIVING.load(Ordering::Relaxed) {
        return f(this, m); // normal single step (or a step during our own drive)
    }
    DRIVING.store(true, Ordering::Relaxed);
    let _g = ReentryGuard::enter();
    let mut n = 0u32;
    while f(this, m) {
        n += 1;
        if n > 2000 {
            break;
        }
    }
    DRIVING.store(false, Ordering::Relaxed);
    let _ = n;
    false
}

// Skip the full-screen rival ENTRY cut-in (the 2D "RIVAL <name>" card shown before a rival
// race). It is played by SingleModeRaceEntryViewController.<PlayRivalEntryCoroutine>d__103.
// On its FIRST MoveNext (state 0) we set the state field to -1 so the body falls through to
// the default case and renders nothing, then call DestroyRivalEntry() to clear any partial
// visuals and invoke the coroutine's endAction so the flow proceeds straight to the race.
// (Driving the coroutine to completion does NOT work here — its first step yields on the
// rival model/asset load, never advancing the on-screen card; this early-skip does.)
hook_slot!(TR_RIVALMN, D_RIVALMN);
static DESTROY_RIVAL_ENTRY: OnceLock<Invokable> = OnceLock::new(); // SingleModeRaceEntryViewController.DestroyRivalEntry
const O_RIVAL_STATE: usize = 0x10; // <>1__state
const O_RIVAL_ENDACTION: usize = 0x20; // endAction (System.Action)
const O_RIVAL_THIS: usize = 0x28; // <>4__this (SingleModeRaceEntryViewController)
unsafe extern "C" fn on_rival_movenext(this: *mut c_void, m: *mut c_void) -> bool {
    let t = TR_RIVALMN.load(Ordering::Relaxed);
    if t == 0 {
        return false;
    }
    let f: BoolMethodFn = std::mem::transmute(t);
    if !is_rival_enabled() || in_heaven() || this.is_null() {
        return f(this, m);
    }
    let state = *((this as usize + O_RIVAL_STATE) as *const i32);
    if state != 0 {
        return f(this, m); // only intercept the very first step
    }
    let ctrl = *((this as usize + O_RIVAL_THIS) as *const *mut c_void);
    let end_action = *((this as usize + O_RIVAL_ENDACTION) as *const usize);
    *((this as usize + O_RIVAL_STATE) as *mut i32) = -1; // body -> default -> returns false, no visuals
    let _ = f(this, m);
    if !ctrl.is_null() {
        if let Some(inv) = DESTROY_RIVAL_ENTRY.get() {
            if inv.ok() {
                let _g = ReentryGuard::enter();
                inv.call_void(ctrl);
            }
        }
    }
    fire_action(end_action); // proceed to the race
    false
}

// ═══════════════════════════════════════════════════════════════════════════
// B3b — RACE-RESULT AUTO-ADVANCE  (EXPERIMENTAL, default OFF)
// Port of native_skip.js part 3. After "View Results" (+ the unavoidable 1st
// tap), the result screens auto-press their own buttons to the next turn.
// Untested in this native form → gated behind RACE_RESULT_ENABLED; enabling it
// never touches the proven training/event core.
// ═══════════════════════════════════════════════════════════════════════════
// Default ON in builds with feature `races_on`, OFF otherwise.
static RACE_RESULT_ENABLED: AtomicBool = AtomicBool::new(cfg!(feature = "races_on"));
pub fn set_race_result_enabled(on: bool) {
    RACE_RESULT_ENABLED.store(on, Ordering::Relaxed);
}
pub fn is_race_result_enabled() -> bool {
    RACE_RESULT_ENABLED.load(Ordering::Relaxed)
}

// TEAM TRIALS guard. The race-result auto-advance is a CAREER (single-mode) feature,
// but its anchor button ("RaceSkipButton") and press targets also exist on the Team
// Trials result screen — so without this it auto-pressed buttons there and got the TT
// result UI stuck (the long-standing v2.2 bug). htt.rs sets this true when a Team
// Trials result is built; the career view-manager (ChangeMainView, single-mode only)
// clears it when we're back in career. While set, race-result never fires.
static IN_TEAM_TRIALS: AtomicBool = AtomicBool::new(false);
pub fn set_in_team_trials(on: bool) {
    IN_TEAM_TRIALS.store(on, Ordering::Relaxed);
}

/// Race-result auto-advance only fires when the player WON (finished 1st).
/// Anything else — lost, or placement not yet known — means NO auto-advance, so
/// the player handles it manually (e.g. to retry). Reset per race in race.rs
/// (ImportDirect), so a retry or the next race is re-evaluated from scratch.
fn rr_should_advance() -> bool {
    if !RACE_RESULT_ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    // Never auto-advance during Team Trials (career-only feature).
    if IN_TEAM_TRIALS.load(Ordering::Relaxed) {
        return false;
    }
    // Auto-advance when the player WON (placement 1), OR when no race retries remain
    // (`available_continue_num` == 0): a retry isn't possible, so don't hold the result
    // screen on a loss. Placement + continues come from the response hook (race_net).
    // continues == -1 means "unknown" → fall back to the win-only gate.
    #[cfg(feature = "raceread")]
    {
        let won = crate::race::player_finish_order() == 1;
        let no_retries_left = crate::race::continues_available() == 0;
        won || no_retries_left
    }
    #[cfg(not(feature = "raceread"))]
    {
        true
    }
}

const PRESS_GAP_MS: u64 = 130;
const MULTI_MAX: u32 = 4;
// EXACT whitelist + exact match (substring matching caused mis-presses).
fn is_press_target(name: &str) -> bool {
    [
        "ButtonCommon",
        "ButtonCenter",
        "NextButton",
        "ScreenTap",
        "SingleModeNextButton",
        "TouchSprite",
        // Debut / first-race completion (and other special result screens) advance via
        // a "Continue" button. Safe to press: auto_press only runs when rr_should_advance
        // (won, or no retries left), so a retry-eligible loss never auto-continues.
        "ContinueButton",
    ]
    .contains(&name)
}
fn is_multi(name: &str) -> bool {
    name == "ScreenTap"
        || name == "TouchSprite"
        || name == "ButtonCommon"
        || name == "ButtonCenter"
}

/// Append a line to the native engine log (race-result diagnostics).
fn rr_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::paths::log_file("heaven-native.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

// Single global busy flag (mirrors native_skip.js `busy`): set while WE invoke a
// button/dialog method so the Update/Push detours skip during our own calls.
static BUSY: AtomicBool = AtomicBool::new(false);
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
static RR_PRESSES: AtomicU64 = AtomicU64::new(0);

// Resolved (methodPointer code, MethodInfo*) pairs — called DIRECTLY like the
// JS NativeFunctions (no runtime_invoke), passing the trailing MethodInfo arg.
static GETNAME_C: AtomicUsize = AtomicUsize::new(0);
static GETNAME_MI: AtomicUsize = AtomicUsize::new(0);
static OPC_C: AtomicUsize = AtomicUsize::new(0);
static OPC_MI: AtomicUsize = AtomicUsize::new(0);
static ISLOCK_C: AtomicUsize = AtomicUsize::new(0);
static ISLOCK_MI: AtomicUsize = AtomicUsize::new(0);
static CLOSE_C: AtomicUsize = AtomicUsize::new(0);
static CLOSE_MI: AtomicUsize = AtomicUsize::new(0);
static CUR_C: AtomicUsize = AtomicUsize::new(0);
static CUR_MI: AtomicUsize = AtomicUsize::new(0);
static CTOR_C: AtomicUsize = AtomicUsize::new(0);
static CTOR_MI: AtomicUsize = AtomicUsize::new(0);
static C_PED: AtomicUsize = AtomicUsize::new(0);

static NAME_CACHE: OnceLock<Mutex<HashMap<usize, String>>> = OnceLock::new();
static PRESS_STATE: OnceLock<Mutex<HashMap<usize, (u32, u64)>>> = OnceLock::new();
static DONE_DLG: OnceLock<Mutex<std::collections::HashSet<usize>>> = OnceLock::new();
static LOGGED_NAMES: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
fn name_cache() -> &'static Mutex<HashMap<usize, String>> {
    NAME_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn press_state() -> &'static Mutex<HashMap<usize, (u32, u64)>> {
    PRESS_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn done_dlg() -> &'static Mutex<std::collections::HashSet<usize>> {
    DONE_DLG.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}
fn logged_names() -> &'static Mutex<std::collections::HashSet<String>> {
    LOGGED_NAMES.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}
fn clear_rr_caches() {
    if let Ok(mut m) = name_cache().lock() { m.clear(); }
    if let Ok(mut m) = press_state().lock() { m.clear(); }
    if let Ok(mut m) = done_dlg().lock() { m.clear(); }
    if let Ok(mut m) = logged_names().lock() { m.clear(); }
}

// Direct-call ABIs (this, …, MethodInfo*).
type RetPtr = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void; // get_name
type RetBool = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool; // IsLock
type Void2 = unsafe extern "C" fn(*mut c_void, *mut c_void); // Close
type Click = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void); // OnPointerClick(this, ped, mi)
type CurStatic = unsafe extern "C" fn(*mut c_void) -> *mut c_void; // EventSystem.get_current(mi)
type Ctor1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void); // ctor(this, es, mi)

/// GameObject/component name of a button (cached), via direct get_name call.
fn button_name(this: *mut c_void) -> String {
    let key = this as usize;
    if let Ok(m) = name_cache().lock() {
        if let Some(n) = m.get(&key) {
            return n.clone();
        }
    }
    let code = GETNAME_C.load(Ordering::Relaxed);
    let name = if code != 0 {
        unsafe {
            let f: RetPtr = std::mem::transmute(code);
            let s = f(this, GETNAME_MI.load(Ordering::Relaxed) as *mut c_void);
            il2cpp::read_string(s)
        }
    } else {
        String::new()
    };
    if let Ok(mut m) = name_cache().lock() {
        m.insert(key, name.clone());
    }
    // Diagnostic: log each distinct button name seen while the result window is
    // open (identifies e.g. the real name of "Next").
    if WINDOW_OPEN.load(Ordering::Relaxed) && !name.is_empty() {
        if let Ok(mut s) = logged_names().lock() {
            if s.insert(name.clone()) {
                rr_log(&format!("[race-result] button seen: \"{name}\" press={}", is_press_target(&name)));
            }
        }
    }
    name
}

/// Synthetic PointerEventData(EventSystem.current), via direct ctor call.
unsafe fn make_pointer_event() -> *mut c_void {
    let ctor = CTOR_C.load(Ordering::Relaxed);
    let ped_cls = C_PED.load(Ordering::Relaxed);
    if ctor == 0 || ped_cls == 0 {
        return std::ptr::null_mut();
    }
    let cur_c = CUR_C.load(Ordering::Relaxed);
    let es = if cur_c != 0 {
        let f: CurStatic = std::mem::transmute(cur_c);
        f(CUR_MI.load(Ordering::Relaxed) as *mut c_void)
    } else {
        std::ptr::null_mut()
    };
    let obj = il2cpp::object_new(ped_cls as il2cpp::Class);
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    let f: Ctor1 = std::mem::transmute(ctor);
    f(obj, es, CTOR_MI.load(Ordering::Relaxed) as *mut c_void);
    obj
}

fn auto_press(this: *mut c_void) {
    if this.is_null() || BUSY.load(Ordering::Relaxed) {
        return;
    }
    let name = button_name(this);
    if !is_press_target(&name) {
        return;
    }
    let key = this as usize;
    let now = clock().elapsed().as_millis() as u64;
    let max = if is_multi(&name) { MULTI_MAX } else { 1 };
    // Read-only check — do NOT consume an attempt yet (mirrors native_skip.js:
    // the count/lastPress only advance AFTER a successful click). Otherwise a
    // transiently-locked button (e.g. "Next" during the result reveal) burns its
    // single allowed press on a locked frame and never retries.
    {
        let st = match press_state().lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let (cnt, last) = *st.get(&key).unwrap_or(&(0, 0));
        if cnt >= max || now.wrapping_sub(last) < PRESS_GAP_MS {
            return;
        }
    }
    unsafe {
        // Respect button lock — return WITHOUT consuming the attempt so we retry
        // next frame once it unlocks.
        let il = ISLOCK_C.load(Ordering::Relaxed);
        if il != 0 {
            let f: RetBool = std::mem::transmute(il);
            if f(this, ISLOCK_MI.load(Ordering::Relaxed) as *mut c_void) {
                return;
            }
        }
        let opc = OPC_C.load(Ordering::Relaxed);
        if opc == 0 {
            return;
        }
        let ped = make_pointer_event();
        if ped.is_null() {
            return;
        }
        BUSY.store(true, Ordering::Relaxed);
        let f: Click = std::mem::transmute(opc);
        f(this, ped, OPC_MI.load(Ordering::Relaxed) as *mut c_void);
        BUSY.store(false, Ordering::Relaxed);
    }
    // Click succeeded → now consume the attempt + stamp the time.
    if let Ok(mut st) = press_state().lock() {
        let (cnt, _) = *st.get(&key).unwrap_or(&(0, 0));
        st.insert(key, (cnt + 1, now));
    }
    RR_PRESSES.fetch_add(1, Ordering::Relaxed);
}

/// Auto-close a pushed dialog (the JS DialogManager.Push*→Close path).
fn auto_close(dlg: *mut c_void) {
    if dlg.is_null() || BUSY.load(Ordering::Relaxed) {
        return;
    }
    if !il2cpp::object_class_name(dlg).contains("DialogCommon") {
        return;
    }
    {
        let mut d = match done_dlg().lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        if !d.insert(dlg as usize) {
            return;
        }
    }
    let code = CLOSE_C.load(Ordering::Relaxed);
    if code == 0 {
        return;
    }
    unsafe {
        BUSY.store(true, Ordering::Relaxed);
        let f: Void2 = std::mem::transmute(code);
        f(dlg, CLOSE_MI.load(Ordering::Relaxed) as *mut c_void);
        BUSY.store(false, Ordering::Relaxed);
    }
}

pub fn race_result_stats() -> (bool, u64) {
    (WINDOW_OPEN.load(Ordering::Relaxed), RR_PRESSES.load(Ordering::Relaxed))
}

// Detours for race-result.
type Void3 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);
type Push1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void;
type Push2 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> *mut c_void;
hook_slot!(TR_UPDATE, D_UPDATE);
hook_slot!(TR_ONPC, D_ONPC);
hook_slot!(TR_CMV, D_CMV);
hook_slot!(TR_PUSH1, D_PUSH1);
hook_slot!(TR_PUSH2, D_PUSH2);

unsafe extern "C" fn on_button_update(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_UPDATE, this, m);
    pump_pending_tag_cb(); // fire deferred friendship-splash onDone on a clean frame
    if rr_should_advance() && WINDOW_OPEN.load(Ordering::Relaxed) && !in_heaven() {
        auto_press(this);
    }
}
unsafe extern "C" fn on_pointer_click(this: *mut c_void, evt: *mut c_void, m: *mut c_void) {
    let t = TR_ONPC.load(Ordering::Relaxed);
    if t != 0 {
        let f: Void3 = std::mem::transmute(t);
        f(this, evt, m);
    }
    if RACE_RESULT_ENABLED.load(Ordering::Relaxed)
        && !in_heaven()
        && !WINDOW_OPEN.load(Ordering::Relaxed)
        && !IN_TEAM_TRIALS.load(Ordering::Relaxed)
        && button_name(this).contains("RaceSkipButton")
    {
        WINDOW_OPEN.store(true, Ordering::Relaxed);
        clear_rr_caches();
        #[cfg(feature = "raceread")]
        let fo = crate::race::player_finish_order();
        #[cfg(not(feature = "raceread"))]
        let fo = -1;
        rr_log(&format!(
            "[race-result] window OPEN (anchor); finish_order={fo} -> {}",
            if fo == 1 { "SKIP" } else { "MANUAL" }
        ));
    }
}
unsafe extern "C" fn on_change_main_view(this: *mut c_void, m: *mut c_void) {
    call_orig(&TR_CMV, this, m);
    // ChangeMainView is single-mode only → we're back in career, clear the TT guard.
    IN_TEAM_TRIALS.store(false, Ordering::Relaxed);
    if WINDOW_OPEN.swap(false, Ordering::Relaxed) {
        clear_rr_caches();
    }
}
unsafe extern "C" fn on_push1(this: *mut c_void, a: *mut c_void, m: *mut c_void) -> *mut c_void {
    let t = TR_PUSH1.load(Ordering::Relaxed);
    let rv = if t != 0 {
        let f: Push1 = std::mem::transmute(t);
        f(this, a, m)
    } else {
        std::ptr::null_mut()
    };
    if rr_should_advance() && WINDOW_OPEN.load(Ordering::Relaxed) && !in_heaven() {
        auto_close(rv);
    }
    rv
}
unsafe extern "C" fn on_push2(
    this: *mut c_void,
    a: *mut c_void,
    b: *mut c_void,
    m: *mut c_void,
) -> *mut c_void {
    let t = TR_PUSH2.load(Ordering::Relaxed);
    let rv = if t != 0 {
        let f: Push2 = std::mem::transmute(t);
        f(this, a, b, m)
    } else {
        std::ptr::null_mut()
    };
    if rr_should_advance() && WINDOW_OPEN.load(Ordering::Relaxed) && !in_heaven() {
        auto_close(rv);
    }
    rv
}

/// Install race-result auto-advance (faithful port of native_skip.js part 3).
/// Independent of training/events. Returns Ok(note) describing what resolved.
pub fn install_race_result() -> Result<String, String> {
    let mut note = String::new();
    let btn = il2cpp::class("Gallop.ButtonCommon");
    if btn.is_null() {
        return Err("anchor miss".into());
    }
    let cvm = il2cpp::class("Gallop.SingleModeChangeViewManager");
    if cvm.is_null() {
        return Err("view-mgr miss".into());
    }
    let dm = il2cpp::class("Gallop.DialogManager");
    let dc = il2cpp::class("Gallop.DialogCommon");
    let obj = il2cpp::class("UnityEngine.Object");
    let es = il2cpp::class("UnityEngine.EventSystems.EventSystem");
    let ped = il2cpp::class("UnityEngine.EventSystems.PointerEventData");

    // Resolve (code, MethodInfo) pairs.
    let resolve = |cs: &AtomicUsize, ms: &AtomicUsize, k: il2cpp::Class, n: &str, argc: i32| -> bool {
        if k.is_null() {
            return false;
        }
        let m = il2cpp::method(k, n, argc);
        if m.is_null() {
            return false;
        }
        cs.store(il2cpp::method_pointer(m) as usize, Ordering::Relaxed);
        ms.store(m as usize, Ordering::Relaxed);
        true
    };
    if !resolve(&GETNAME_C, &GETNAME_MI, obj, "get_name", 0) {
        note.push_str("name miss; ");
    }
    if !resolve(&OPC_C, &OPC_MI, btn, "OnPointerClick", 1) {
        return Err("click miss".into());
    }
    resolve(&ISLOCK_C, &ISLOCK_MI, btn, "IsLock", 0);
    if !dc.is_null() {
        resolve(&CLOSE_C, &CLOSE_MI, dc, "Close", 0);
    }
    resolve(&CUR_C, &CUR_MI, es, "get_current", 0);
    if !ped.is_null() {
        C_PED.store(ped as usize, Ordering::Relaxed);
        resolve(&CTOR_C, &CTOR_MI, ped, ".ctor", 1);
    }
    if CTOR_C.load(Ordering::Relaxed) == 0 {
        note.push_str("evt ctor miss; ");
    }

    unsafe {
        install_one(btn, "Update", 0, on_button_update as *const (), &TR_UPDATE, &D_UPDATE)?;
        install_one(btn, "OnPointerClick", 1, on_pointer_click as *const (), &TR_ONPC, &D_ONPC)?;
        install_one(cvm, "ChangeMainView", 0, on_change_main_view as *const (), &TR_CMV, &D_CMV)?;
        if dm.is_null() {
            note.push_str("dialog-mgr miss (no auto-close); ");
        } else {
            if install_one(dm, "PushDialog", 1, on_push1 as *const (), &TR_PUSH1, &D_PUSH1).is_err() {
                note.push_str("push1 miss; ");
            }
            if install_one(dm, "PushDialogSequence", 2, on_push2 as *const (), &TR_PUSH2, &D_PUSH2).is_err() {
                note.push_str("push2 miss; ");
            }
        }
    }
    Ok(note)
}

// ── install ─────────────────────────────────────────────────────────────────
unsafe fn install_one(
    klass: il2cpp::Class,
    name: &str,
    argc: i32,
    detour_fn: *const (),
    tramp: &AtomicUsize,
    keep: &OnceLock<RawDetour>,
) -> Result<(), String> {
    let m = il2cpp::method(klass, name, argc);
    if m.is_null() {
        return Err(format!("{name} miss"));
    }
    let target = il2cpp::method_pointer(m);
    if target.is_null() {
        return Err(format!("{name} ptr null"));
    }
    if il2cpp::is_detoured(target) {
        return Err(format!("{name}: already detoured (skipped)"));
    }
    let d = RawDetour::new(target as *const (), detour_fn).map_err(|e| format!("{name}: {e}"))?;
    d.enable().map_err(|e| format!("{name} enable: {e}"))?;
    tramp.store(d.trampoline() as *const () as usize, Ordering::Relaxed);
    let _ = keep.set(d);
    Ok(())
}

/// Returns (training_ok, events_ok, notes).
pub fn install() -> (bool, bool, String) {
    let mut notes = String::new();
    let mut training_ok = false;
    let mut events_ok = false;

    // ── TRAINING ──  (all IL2CPP names obfuscated → no `strings` leak)
    let helper = il2cpp::class("Gallop.SingleModeTrainingCutInHelper");
    if helper.is_null() {
        notes.push_str("train helper miss; ");
    } else {
        let _ = SKIP_RUNTIME.set(resolve(helper, "SkipRuntime", 0));
        if !SKIP_RUNTIME.get().map(|i| i.ok()).unwrap_or(false) {
            notes.push_str("train skip miss; ");
        } else {
            let mut any = false;
            unsafe {
                for r in [
                    install_one(helper, "OnStartCutIn", 0, on_start_cutin as *const (), &TR_START, &D_START),
                    install_one(helper, "OnPlayCutIn", 0, on_play_cutin as *const (), &TR_PLAY, &D_PLAY),
                    install_one(helper, "OnPlayMainCutIn", 0, on_play_main_cutin as *const (), &TR_MAIN, &D_MAIN),
                ] {
                    match r {
                        Ok(()) => any = true,
                        Err(e) => notes.push_str(&format!("{e}; ")),
                    }
                }
            }
            training_ok = any;
        }
    }

    // ── TAG (friendship/rainbow) TRAINING splash ── skip the "FRIENDSHIP TRAINING!"
    //    cut-in by firing its onDone early (deferred). Shares the training-skip toggle.
    let _ = ACTION_INVOKE.set(resolve(il2cpp::class("System.Action"), "Invoke", 0));
    let _ = SET_ACTIVE.set(resolve(il2cpp::class("UnityEngine.GameObject"), "SetActive", 1));
    let tag = il2cpp::class("Gallop.SingleModeMainViewTagTrainingCutInPlayer");
    if tag.is_null() {
        notes.push_str("tag cutin miss; ");
    } else if !ACTION_INVOKE.get().map(|i| i.ok()).unwrap_or(false) {
        notes.push_str("action.invoke miss; ");
    } else {
        unsafe {
            if let Err(e) = install_one(tag, "PlayCutIn", 2, on_tag_play_cutin as *const (), &TR_TAGIN, &D_TAGIN) {
                notes.push_str(&format!("{e}; "));
            }
            let _ = install_one(tag, "PlayCutInOut", 1, on_tag_play_cutin_out as *const (), &TR_TAGOUT, &D_TAGOUT);
        }
    }

    // ── PRO SHOP (scenario free shop) buy/use animation skip ── (own "Shop" toggle)
    let shop = il2cpp::class("Gallop.SingleModeScenarioFreeShopViewController");
    if shop.is_null() {
        notes.push_str("shop ctrl miss; ");
    } else {
        // Chara-message popup skip (the "Use <item>" card) needs no callback plumbing.
        unsafe {
            if let Err(e) = install_one(shop, "PlayCharaMessage", 1, on_chara_msg as *const (), &TR_CHARAMSG, &D_CHARAMSG) {
                notes.push_str(&format!("{e}; "));
            }
        }
        // The BUY performance skip defers the original callbacks, so it needs Action.Invoke.
        if !ACTION_INVOKE.get().map(|i| i.ok()).unwrap_or(false) {
            notes.push_str("shop: action.invoke miss; ");
        } else {
            unsafe {
                if let Err(e) = install_one(shop, "PlayUseItemPerformanceCore", 3, on_shop_perf as *const (), &TR_SHOPPERF, &D_SHOPPERF) {
                    notes.push_str(&format!("{e}; "));
                }
            }
        }
    }
    // Inventory use-item animation: drive its performance coroutine to completion on the first
    // MoveNext (game does its own teardown/continuation; only the visual waits collapse).
    {
        let coro = il2cpp::nested_class(
            "Gallop.PartsSingleModeScenarioFreeUseItemPerformance",
            "<PlayUseItemPerformanceCoroutine>d__14",
        );
        if coro.is_null() {
            notes.push_str("useperf coro miss; ");
        } else {
            unsafe {
                if let Err(e) = install_one(coro, "MoveNext", 0, on_movenext as *const (), &TR_MOVENEXT, &D_MOVENEXT) {
                    notes.push_str(&format!("coro movenext: {e}; "));
                }
            }
        }
    }

    // ── RIVAL-RACE entry intro ("RIVAL <name>" card) — skip its coroutine on the first step ──
    {
        let entry = il2cpp::class("Gallop.SingleModeRaceEntryViewController");
        if entry.is_null() {
            notes.push_str("rival entry cls miss; ");
        } else {
            let _ = DESTROY_RIVAL_ENTRY.set(resolve(entry, "DestroyRivalEntry", 0));
            if !DESTROY_RIVAL_ENTRY.get().map(|i| i.ok()).unwrap_or(false) {
                notes.push_str("rival destroy miss; ");
            }
        }
        let rcoro = il2cpp::nested_class(
            "Gallop.SingleModeRaceEntryViewController",
            "<PlayRivalEntryCoroutine>d__103",
        );
        if rcoro.is_null() {
            notes.push_str("rival coro miss; ");
        } else {
            unsafe {
                if let Err(e) = install_one(rcoro, "MoveNext", 0, on_rival_movenext as *const (), &TR_RIVALMN, &D_RIVALMN) {
                    notes.push_str(&format!("rival movenext: {e}; "));
                }
            }
        }
    }

    // ── EVENTS ──
    let view = il2cpp::class("Gallop.StoryViewController");
    let story = il2cpp::class("Gallop.StoryTimelineController");
    if view.is_null() {
        notes.push_str("story view miss; ");
    } else {
        let _ = SKIP_STORY.set(resolve(view, "SkipStory", 0));
        let _ = GET_TL.set(resolve(view, "get_TimelineController", 0));
        let _ = TRAIN_CUTT.set(resolve(view, "get_IsPlayingOrWillPlayTrainingCutt", 0));
        if !story.is_null() {
            let _ = IS_PLAYING.set(resolve(story, "get_IsPlaying", 0));
        }
        if !SKIP_STORY.get().map(|i| i.ok()).unwrap_or(false) {
            notes.push_str("story skip miss; ");
        } else {
            unsafe {
                match install_one(view, "OnStartPlayingTimeline", 0,
                                  on_start_timeline as *const (), &TR_TIMELINE, &D_TIMELINE) {
                    Ok(()) => events_ok = true,
                    Err(e) => notes.push_str(&format!("{e}; ")),
                }
            }
        }
    }

    (training_ok, events_ok, notes)
}
