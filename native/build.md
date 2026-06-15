# Building Heaven (`heaven_overlay.dll`)

The overlay is a single Rust `cdylib` that loads into the game and renders inside its
D3D11 frame (hudhook + imgui). No external process — everything runs in-process.

## 1. Toolchain (one time)

1. **Rust (MSVC toolchain):** https://rustup.rs → run `rustup-init.exe`, accept the
   defaults (`stable-x86_64-pc-windows-msvc`).
2. **MSVC Build Tools** (the C++ linker hudhook needs): "Build Tools for Visual Studio"
   → https://visualstudio.microsoft.com/visual-cpp-build-tools/ → tick **"Desktop
   development with C++"** (gives `link.exe` + the Windows SDK).
3. Restart the shell so `cargo` and `link.exe` are on PATH:
   ```
   cargo --version
   ```

## 2. Build

```
cd native
cargo build --release
```

Output: `native/target/release/heaven_overlay.dll`.

> If `cargo` complains the `hudhook` API doesn't match, pin it: in `Cargo.toml` set
> `hudhook = "=0.6.0"` (or the latest 0.6.x) and re-run. The `ImguiRenderLoop` trait
> and `hudhook!` macro signatures occasionally move between minor versions.

## 3. Install

Copy `heaven_overlay.dll` and `version.dll` into the game folder next to
`UmamusumePrettyDerby.exe`, launch the game, and press **Insert** to open the menu.
See the repository README for the full install steps.

## Graphics API note

Umamusume (Unity) runs on **D3D11** — `lib.rs` uses `ImguiDx11Hooks`. If a future game
build switches to D3D12 or Vulkan, swap the hook type:

```rust
use hudhook::hooks::dx12::ImguiDx12Hooks;   // then: hudhook!(ImguiDx12Hooks, ...)
```
