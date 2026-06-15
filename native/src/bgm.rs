//! Heaven — silence the game's original title BGM while the custom intro plays.
//!
//! `Gallop.AudioManager` exposes `SetBgmVolume(float, float)` and `GetBgmVolume()`. We grab
//! the singleton (get_Instance), save the current BGM volume, set it to 0 on entering the
//! title scene, and restore it on leaving. Pure native IL2CPP calls via the compiled method
//! pointers (floats ride XMM per the Win64 ABI, which `extern "C"` handles).

#![allow(dead_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use obfstr::obfstr;

use crate::il2cpp;

static GET_INST: AtomicUsize = AtomicUsize::new(0);
static SET_VOL: AtomicUsize = AtomicUsize::new(0);
static GET_VOL: AtomicUsize = AtomicUsize::new(0);
static SET_BUS: AtomicUsize = AtomicUsize::new(0); // SetBusVolume(String, float)
static GET_CUR: AtomicUsize = AtomicUsize::new(0); // GetCurBusVolumeParam(String) -> float
static STOP_VOICE_ALL: AtomicUsize = AtomicUsize::new(0); // StopVoiceAll(float fade)
static SET_VOICE_UNAVAIL: AtomicUsize = AtomicUsize::new(0); // SetVoiceUnavailable(bool)
static SAVED: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0 default
static RESOLVED: AtomicBool = AtomicBool::new(false);

fn log(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true)
        .open(crate::paths::log_file("heaven-native.log")) { let _ = writeln!(f, "{msg}"); }
}

/// Resolve the AudioManager methods once (call after the runtime is ready).
pub fn init() {
    let k = il2cpp::class(obfstr!("Gallop.AudioManager"));
    if k.is_null() {
        log("[bgm] AudioManager class not found");
        return;
    }
    let gi = il2cpp::method(k, obfstr!("get_Instance"), 0);
    let sv = il2cpp::method(k, obfstr!("SetBgmVolume"), 2);
    let gv = il2cpp::method(k, obfstr!("GetBgmVolume"), 0);
    let sb = il2cpp::method(k, obfstr!("SetBusVolume"), 2);
    let gc = il2cpp::method(k, obfstr!("GetCurBusVolumeParam"), 1);
    let sva = il2cpp::method(k, obfstr!("StopVoiceAll"), 1);
    let svu = il2cpp::method(k, obfstr!("SetVoiceUnavailable"), 1);
    STOP_VOICE_ALL.store(sva as usize, Ordering::Relaxed);
    SET_VOICE_UNAVAIL.store(svu as usize, Ordering::Relaxed);
    GET_INST.store(gi as usize, Ordering::Relaxed);
    SET_VOL.store(sv as usize, Ordering::Relaxed);
    GET_VOL.store(gv as usize, Ordering::Relaxed);
    SET_BUS.store(sb as usize, Ordering::Relaxed);
    GET_CUR.store(gc as usize, Ordering::Relaxed);
    RESOLVED.store(!gi.is_null() && !sv.is_null(), Ordering::Relaxed);
    log(&format!(
        "[bgm] resolve: SetBgmVolume={} SetBusVolume={} StopVoiceAll={} SetVoiceUnavailable={}",
        !sv.is_null(), !sb.is_null(), !sva.is_null(), !svu.is_null()
    ));
}

// Buses muted during the intro so only our song is heard. Master/MasterOut kill ALL game audio
// (incl. the title USM movie + the "Cygames / Pretty Derby" voice); Voice/SE are belt-and-braces.
// Our intro song plays through a separate WASAPI stream (rodio), so it is unaffected.
const INTRO_MUTE_BUSES: &[&str] = &["Master", "MasterOut", "Voice", "SE"];
static SAVED_VOICE: [AtomicU32; 4] = [
    AtomicU32::new(0x3f80_0000), AtomicU32::new(0x3f80_0000),
    AtomicU32::new(0x3f80_0000), AtomicU32::new(0x3f80_0000),
];
static VOICE_SAVED: AtomicBool = AtomicBool::new(false);

/// Low-level: SetBusVolume(this, busName, vol). The 2-arg overload is instance on the singleton
/// (same calling shape as SetBgmVolume). Safe string+float call — no object deref.
fn set_bus(inst: *mut c_void, name: &str, vol: f32) {
    let sbm = SET_BUS.load(Ordering::Relaxed) as *const c_void;
    if sbm.is_null() {
        return;
    }
    let p = il2cpp::method_pointer(sbm as il2cpp::Method);
    if p.is_null() {
        return;
    }
    let s = il2cpp::new_string(name);
    if s.is_null() {
        return;
    }
    let f: extern "C" fn(*mut c_void, *mut c_void, f32, *const c_void) =
        unsafe { std::mem::transmute(p) };
    f(inst, s, vol, sbm);
}

fn get_bus(inst: *mut c_void, name: &str) -> f32 {
    let gcm = GET_CUR.load(Ordering::Relaxed) as *const c_void;
    if gcm.is_null() {
        return 1.0;
    }
    let p = il2cpp::method_pointer(gcm as il2cpp::Method);
    if p.is_null() {
        return 1.0;
    }
    let s = il2cpp::new_string(name);
    if s.is_null() {
        return 1.0;
    }
    let f: extern "C" fn(*mut c_void, *mut c_void, *const c_void) -> f32 =
        unsafe { std::mem::transmute(p) };
    f(inst, s, gcm)
}

/// Mute every game-audio bus (Master/Voice/SE) during the intro so ONLY our song is heard —
/// this kills the title USM movie audio and the "Cygames / Pretty Derby" voice that the BGM
/// mute alone leaves audible. Called every frame at the title (cheap) to win any re-trigger.
/// Saves each bus's prior volume once so we can restore it on leave.
pub fn mute_voice() {
    let inst = instance();
    if inst.is_null() {
        return;
    }
    if !VOICE_SAVED.swap(true, Ordering::Relaxed) {
        for (i, name) in INTRO_MUTE_BUSES.iter().enumerate() {
            let cur = get_bus(inst, name);
            if cur > 0.001 {
                SAVED_VOICE[i].store(cur.to_bits(), Ordering::Relaxed);
            }
        }
        log("[bgm] intro buses muted");
    }
    for name in INTRO_MUTE_BUSES {
        set_bus(inst, name, 0.0);
    }
    // The "Cygames / Pretty Derby" title voice is a CRI voice cue, not a bus — stop it outright
    // and disable voice playback so it can't re-trigger while our intro plays.
    call_f32(inst, &STOP_VOICE_ALL, 0.0);
    call_bool(inst, &SET_VOICE_UNAVAIL, true);
}

/// Call an instance method with a single f32 arg: f(this, value, MethodInfo*).
fn call_f32(inst: *mut c_void, slot: &AtomicUsize, v: f32) {
    let m = slot.load(Ordering::Relaxed) as *const c_void;
    if m.is_null() {
        return;
    }
    let p = il2cpp::method_pointer(m as il2cpp::Method);
    if p.is_null() {
        return;
    }
    let f: extern "C" fn(*mut c_void, f32, *const c_void) = unsafe { std::mem::transmute(p) };
    f(inst, v, m);
}

/// Call an instance method with a single bool arg: f(this, value, MethodInfo*).
fn call_bool(inst: *mut c_void, slot: &AtomicUsize, v: bool) {
    let m = slot.load(Ordering::Relaxed) as *const c_void;
    if m.is_null() {
        return;
    }
    let p = il2cpp::method_pointer(m as il2cpp::Method);
    if p.is_null() {
        return;
    }
    let f: extern "C" fn(*mut c_void, bool, *const c_void) = unsafe { std::mem::transmute(p) };
    f(inst, v, m);
}

/// Restore every intro-muted bus to its pre-mute volume when leaving the title.
pub fn restore_voice() {
    let inst = instance();
    if inst.is_null() {
        return;
    }
    for (i, name) in INTRO_MUTE_BUSES.iter().enumerate() {
        let v = f32::from_bits(SAVED_VOICE[i].load(Ordering::Relaxed));
        set_bus(inst, name, v);
    }
    call_bool(inst, &SET_VOICE_UNAVAIL, false); // re-enable voice for the rest of the game
    VOICE_SAVED.store(false, Ordering::Relaxed);
    log("[bgm] intro buses restored");
}

fn instance() -> *mut c_void {
    let m = GET_INST.load(Ordering::Relaxed) as *const c_void;
    if m.is_null() {
        return std::ptr::null_mut();
    }
    let p = il2cpp::method_pointer(m as il2cpp::Method);
    if p.is_null() {
        return std::ptr::null_mut();
    }
    // static method: only the trailing MethodInfo* arg, returns the singleton Object.
    let f: extern "C" fn(*const c_void) -> *mut c_void = unsafe { std::mem::transmute(p) };
    f(m)
}

pub fn mute() {
    if !RESOLVED.load(Ordering::Relaxed) {
        return;
    }
    let inst = instance();
    if inst.is_null() {
        return;
    }
    // Save the current BGM volume (don't overwrite the saved value with an already-muted 0).
    let gvm = GET_VOL.load(Ordering::Relaxed) as *const c_void;
    if !gvm.is_null() {
        let gp = il2cpp::method_pointer(gvm as il2cpp::Method);
        if !gp.is_null() {
            let gf: extern "C" fn(*mut c_void, *const c_void) -> f32 = unsafe { std::mem::transmute(gp) };
            let cur = gf(inst, gvm);
            if cur > 0.001 {
                SAVED.store(cur.to_bits(), Ordering::Relaxed);
            }
        }
    }
    set_volume(inst, 0.0);
    log("[bgm] muted");
}

/// Force the BGM volume to 0 with no logging and no save — called every frame while at the
/// title so the game's PlayBgm volume reset can't bring the original track back.
pub fn force_mute() {
    if !RESOLVED.load(Ordering::Relaxed) {
        return;
    }
    let inst = instance();
    if !inst.is_null() {
        set_volume(inst, 0.0);
    }
}

pub fn restore() {
    if !RESOLVED.load(Ordering::Relaxed) {
        return;
    }
    let inst = instance();
    if inst.is_null() {
        return;
    }
    let v = f32::from_bits(SAVED.load(Ordering::Relaxed));
    set_volume(inst, v);
    log(&format!("[bgm] restored to {v}"));
}

fn set_volume(inst: *mut c_void, vol: f32) {
    let svm = SET_VOL.load(Ordering::Relaxed) as *const c_void;
    if svm.is_null() {
        return;
    }
    let p = il2cpp::method_pointer(svm as il2cpp::Method);
    if p.is_null() {
        return;
    }
    // SetBgmVolume(this, float volume, float fade, MethodInfo*).
    let f: extern "C" fn(*mut c_void, f32, f32, *const c_void) = unsafe { std::mem::transmute(p) };
    f(inst, vol, 0.0, svm);
}
