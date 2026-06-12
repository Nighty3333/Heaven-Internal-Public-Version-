# Heaven Internal — Public Version

In-game overlay for **Umamusume Pretty Derby (Global)**. Public release: **SuperSkip**,
**Frame Rate** control, **Team Trials capture**, and an optional **custom video intro**.
Renders inside the game (D3D11 + imgui) — no external window.

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
     `heaven_version.dll`**. Same result, using your own Windows system file instead
     of the bundled copy.

   If you don't care, do it the default way — the package already ships a working
   `heaven_version.dll`.
4. Launch the game. Heaven loads itself — press **Insert** to show/hide the panel.

> **Antivirus note:** `version.dll` is a *proxy loader* (a normal technique for in-game
> overlays). Windows Defender or some antivirus may flag it as a **false positive**
> because it loads a DLL into the game. It is not malware — but if your AV quarantines
> it, allow-list the game folder. (This is also why this build does **not** use a
> commercial packer like Themida/VMProtect — those trip both antivirus and anti-cheat.)

To uninstall: delete the 3 files.

---

## Features

### SuperSkip
Skips the slow cut-ins automatically. Three independent toggles in the panel:

| Toggle | What it skips |
|--------|---------------|
| **Events** | Story / event / recreation / rest timelines |
| **Training** | Training cut-in animations |
| **Races** | Auto-advances the post-race result screens |

- **Races only auto-advances when you WON** (finished 1st). If you lost — or the
  placement isn't known yet — it stops so you can handle it manually (e.g. to retry).
- **Races never runs during Team Trials** — it's a career (story-mode) feature only.
- Defaults: Events **ON**, Training **ON**, **Races OFF** (it's experimental — turn it
  on in the panel if you want it).

### Frame Rate
Lifts the game's frame-rate lock. Master **Cap FPS** toggle, a **1–300** slider, and
**Unlimited** (renders as fast as possible, vSync forced off). Shows the real measured
FPS live.

> **⚠ Work in progress:** this currently **unlocks / caps the frames the game already
> produces** (removes the 30/60 lock + vSync override). It does **not yet generate a
> truly higher-refresh "real frame"** — that proper high-refresh rendering is still WIP
> and planned for a later build.

### Team Trials capture  (`Team Trials` = ON)
Captures your **Team Trials** results as you view them — it reads each trial's result
and writes it to Heaven's data folder. This feature **works together with the main
Heaven dashboard**, which is what reads and analyzes the captured data.

**How to use it:**
1. Enable **Team Trials** under `capture` in the panel (it shows `ON (N saved)`).
2. Open your Team Trials results in-game — each one you view is saved automatically.
3. Open them in the main Heaven app to browse/analyze:
   **https://github.com/Nighty3333/Heaven**

This public build only does the *capture*; the analysis lives in Heaven (normal).

---

## Custom intro  *(optional)*

Play your own video as the game's startup intro. It draws over the splash screens shortly
after launch, plays your audio track, and shows a **START GAME** button (bottom-right) to
skip into the game.

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

This writes `intro_full.bin` and `intro_song.ogg` — copy both next to `heaven_overlay.dll`
and launch. Resolution and fps are configurable:

```
python pack_intro.py my_video.mp4 --res 1920x1080 --fps 30
```

`--res` (default `2560x1440`) and `--fps` (default `60`) are free. The video is scaled to
fill the screen, so use a 16:9 size to avoid stretching. Full guide:
**[custom-intro.md](custom-intro.md)**. Delete the two files to restore the normal startup.

> You supply your own video; nothing copyrighted is included with Heaven.

---

## The panel (press **Insert**)

- **superskip** — Events / Training / Races toggles (ON/OFF each).
- **frame rate** — Cap FPS, Unlimited, the 1–300 slider, and live FPS.
- **capture** — Team Trials capture ON/OFF + how many are saved.
- **Dock left / Dock right** — move the panel to the other side of the screen.
- **update** — opens the Releases page in your browser (see below).
- **Support on Ko-fi** — opens the donation page.

---

## Updating

This build does **not** auto-update. To update:

1. Click **Releases** in the panel (or go to the Releases page):
   **https://github.com/Nighty3333/Heaven-Internal-Public-Version-/releases**
2. Download the newest release zip.
3. Close the game, replace the **3 DLLs** with the new ones, relaunch.

(A loaded DLL can't replace itself while the game is running, so an update is always:
download → replace files → restart.)

---

## Credits & support

Made by **Night DC : nighty3333**.

If Heaven saves you time, a coffee is appreciated:
[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/nighty33)

See [LICENSE](LICENSE) for terms — this is proprietary software: **no redistribution,
no reverse-engineering, no decompilation.**
