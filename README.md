# Heaven Internal — Public Version

In-game **QoL overlay** for **Umamusume Pretty Derby (Steam / Global)** — a single native
DLL that loads with the game and renders inside it (D3D11 + imgui). No external window, no
Python, no extra process. Open the game and press **Insert** for the menu.

**Made by Night DC : nighty3333**

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/nighty33)

---

## Install

1. Close the game.
2. Copy these files into the game folder, next to `UmamusumePrettyDerby.exe`
   (usually `…\steamapps\common\UmamusumePrettyDerby\`):
   - `version.dll`
   - `heaven_overlay.dll`
3. Add **`heaven_version.dll`** — pick one of the two ways:
   - **Default (our way):** just copy the included `heaven_version.dll` into the
     same folder. Done — works out of the box.
   - **Make it yourself (optional):** copy your own
     `C:\Windows\System32\version.dll` into the game folder and **rename it to
     `heaven_version.dll`**.
4. Launch the game. Heaven loads itself — press **Insert** to show/hide the menu.
   Use **Windowed / Borderless** so the overlay is visible (not exclusive fullscreen).

> **Antivirus note:** `version.dll` is a *proxy loader* (a normal technique for in-game
> overlays). Windows Defender or some antivirus may flag it as a **false positive**
> because it loads a DLL into the game. It is not malware — if your AV quarantines it,
> allow-list the game folder. (This build deliberately does **not** use a commercial
> packer like Themida/VMProtect — those trip both antivirus and anti-cheat.)

To uninstall: delete the 3 files.

---

## Features

### Skip
- **SuperSkip** — *Events / Training / Races*, each toggleable. Calls the game's own
  skip routines and auto-advances the post-race result screens. Training skip also
  skips the friendship training cut-in (the "FRIENDSHIP TRAINING!" splash).
  - **Races only auto-advances when you WON** (finished 1st). If you lost — or the
    placement isn't known yet — it stops so you can handle it manually (e.g. a retry).
  - **Races never runs during Team Trials** — it's a career (story-mode) feature only.
  - Defaults: Events **ON**, Training **ON**, **Races OFF**.
- **Game speed** — speeds up the game's UI / story animations (menu opens, transitions,
  event text). Slider **1x–10x**.

### Performance
- **Low Resources mode** — "potato" mode for very weak PCs: lowest 3D quality, no
  shadows / AA, low textures & LOD, and lighter cloth physics. One toggle.
- **Frame rate** — master **Cap FPS** toggle, a **1–300** slider, and **Unlimited**
  (renders as fast as possible, vSync forced off). Shows the **real measured FPS** (a true
  frames-per-second counter, not an estimate).
- **Cloth physics** — uncap the character's hair / cloth physics so they stay smooth at
  high frame rates instead of the default frame-skipping.
- **Graphics** — force the **max 3D model quality** beyond the in-game cap, plus enhanced
  textures (anisotropic filtering), LOD and shadow detail.
- **Display & Window** — **always-on-top**, **block-minimize**, and **screen mode**
  (borderless / exclusive / windowed).

> **⚠ Frame rate — note:** this **unlocks / caps the frames the game already produces**
> (removes the 30/60 lock + vSync override) and measures them exactly. It does **not**
> synthesise extra "real" frames; true high-refresh rendering is a separate WIP.

### Team Trials capture  (`Capture` → ON)
Captures your **Team Trials** results as you view them — it reads each trial's result and
writes it to Heaven's data folder. This works together with the main **Heaven** app, which
reads and analyzes the captured data.

1. Enable **Team Trials** under `Capture` (it shows `N saved`).
2. Open your Team Trials results in-game — each one you view is saved automatically.
3. Browse/analyze them in the main Heaven app:
   **https://github.com/Nighty3333/Heaven**

This public build only does the *capture*; the analysis lives in Heaven.

---

## Custom intro  *(optional)*

Play your own video as the game's startup intro. It draws over the splash screens, plays
your audio track, and shows a **START GAME** button (bottom-right) to skip into the game.

Two files in the game folder drive it, read at runtime (no reinstall to change them):

| File | What it is |
|------|------------|
| `intro_full.bin` | the video (a stream of frames + a small header) |
| `intro_song.ogg` | the audio track |

Both go next to `heaven_overlay.dll`. If either is missing, that part is simply skipped.

**Build them from any video** with the included `pack_intro.py` (needs Python 3.8+ and
ffmpeg, on PATH or `pip install imageio-ffmpeg`):

```
python pack_intro.py my_video.mp4
```

Copy the two output files next to `heaven_overlay.dll` and launch. Resolution and fps are
configurable:

```
python pack_intro.py my_video.mp4 --res 1920x1080 --fps 30
```

Full guide: **[custom-intro.md](custom-intro.md)**. Delete the two files to restore the
normal startup.

> You supply your own video; nothing copyrighted is included with Heaven.

---

## The menu (press **Insert**)

A sidebar with sections: **Gameplay**, **Camera**, **Visuals**, **Performance**,
**Interface**, **About**. Every setting is remembered across sessions. The open/close key
(default **Insert**) and the window layout are configurable in **Interface → Layout**.

Prefer something simpler? Toggle **Classic menu** in **Interface → Layout** to switch to the
original compact menu in-game — it carries the full feature set grouped into collapsible
categories, just a plainer style.

---

## Compatibility

Heaven can run alongside Hachimi. When both are installed, Heaven yields to Hachimi
where they would otherwise overlap, so they don't collide. Full side-by-side support is
planned but not finalized yet, so occasional conflicts may still appear after updates.

---

## Updating

**Heaven updates itself.** When a newer version is available, it downloads quietly in
the background while you play. **Restart the game once** and the new version is applied
automatically — that's it. You can see the status anytime in the menu under **Updates**
(e.g. *"Update vX.Y.Z ready — restart to apply"*).

How it works: a loaded DLL can't replace itself while the game is running, so the new
version is staged and swapped in cleanly the next time you launch.

**First time only:** if you're on an older build from *before* auto-update existed, do
**one** manual update to get a version that has it (steps below). After that, updates are
automatic.

Manual update (only needed that first time, or if the auto-updater can't reach the
internet):

1. Open the **Releases** page:
   **https://github.com/Nighty3333/Heaven-Internal-Public-Version-/releases**
2. Download the newest release zip.
3. Close the game, replace the **3 DLLs** with the new ones, relaunch.

---

## Build from source

The full source for the overlay DLL lives in [`native/`](native/). Build it with Rust
(stable, MSVC toolchain) on Windows:

```
cd native
cargo build --release
```

The DLL is produced at `native/target/release/heaven_overlay.dll`. The custom-intro media
(`intro_full.bin` / `intro_song.ogg`) is not part of the build — supply your own (see the
Custom intro section above).

---

## Credits & support

Made by **Night DC : nighty3333**.

If Heaven saves you time, a coffee is appreciated:
[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/nighty33)

Licensed under the **MIT License** — see [LICENSE](LICENSE). The full source is in this
repository: you're free to read, build, and modify it.
