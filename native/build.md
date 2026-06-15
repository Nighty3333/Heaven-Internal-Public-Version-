# Building the Heaven overlay (`heaven_overlay.dll`)

This DLL is the in-game overlay — it draws the HUD inside the game's D3D11
frame, rendered directly in the swapchain (no external window).

## 1. Install the toolchain (one time)

1. **Rust (MSVC toolchain):** https://rustup.rs → run `rustup-init.exe`, accept
   defaults (`stable-x86_64-pc-windows-msvc`).
2. **MSVC Build Tools** (the C++ linker hudhook needs):
   - Download "Build Tools for Visual Studio" →
     https://visualstudio.microsoft.com/visual-cpp-build-tools/
   - In the installer tick **"Desktop development with C++"** (gives `link.exe`
     + Windows SDK). ~2–4 GB.
3. Restart the shell so `cargo` and `link.exe` are on PATH. Verify:
   ```
   cargo --version
   rustc --version
   ```

## 2. Build

```
cd native
cargo build --release
```

Output: `native/target/release/heaven_overlay.dll`.

> If `cargo` complains the `hudhook` API doesn't match, pin it:
> in `Cargo.toml` set `hudhook = "=0.6.0"` (or the latest 0.6.x) and re-run.
> The `ImguiRenderLoop` trait and `hudhook!` macro signatures occasionally move
> between minor versions — `overlay.rs` / `lib.rs` target the 0.6 line.

## Graphics API note

Umamusume (Unity) runs on **D3D11** by default — `lib.rs` uses `ImguiDx11Hooks`.
If a future build uses D3D12 or Vulkan, swap the hook type:

```rust
use hudhook::hooks::dx12::ImguiDx12Hooks;   // then: hudhook!(ImguiDx12Hooks, ...)
```
