# Sahou Out CHOP — Windows build

**Placeholder / planned.** This directory will hold the Windows build of the TouchDesigner
Sahou Out CHOP: a Visual Studio project that compiles the shared C++ op source against the
TouchDesigner C++ SDK and links the Rust core, producing `SahouOut.dll`.

Shared, platform-independent parts live one level up:

- `../src` — the CHOP C++ glue (`SahouOutCHOP`, `payload`, `envelope`)
- `../transport` — the `sahou-transport` Rust cdylib (builds `sahou_transport.dll` on Windows)
- `../test` — TD-independent unit tests + FFI smoke
- `../examples` — the demo contract

The macOS build lives in [`../macos`](../macos) (Xcode → `SahouOut.plugin`).

**Status: not implemented yet.** The current supported build is macOS / arm64. Bringing up
Windows means: a VS project mirroring the Xcode build (compile `../src` + `../vendor` TD SDK,
statically link `libsahou_core`, bundle `sahou_transport.dll`), plus its own signing story.
