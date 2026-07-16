# Sahou Out / In CHOP — Windows build

The Windows build of the TouchDesigner Sahou **Out** and **In** CHOPs: a CMake project that
compiles the shared C++ op source against the TouchDesigner C++ SDK, statically links the Rust
core, and links the Zenoh transport cdylib — producing `SahouOut.dll` and `SahouIn.dll`.

Shared, platform-independent parts live one level up:

- `../src` — the CHOP C++ glue (`SahouOutCHOP` / `SahouInCHOP`, `payload`, `envelope`, `outcome`)
- `../transport` — the `sahou-transport` Rust cdylib (`sahou_transport.dll` on Windows)
- `../test` — TD-independent unit tests + FFI smoke
- `../examples` — the demo contract

The macOS build lives in [`../macos`](../macos) (Xcode → `SahouOut.plugin`).

## Requirements

- MSVC 2022 Build Tools ("Desktop development with C++").
- CMake ≥ 3.20 (uses the Visual Studio 17 2022 generator — no dev-shell needed).
- Rust with the `x86_64-pc-windows-msvc` target (`build-td-windows` adds it).
- TouchDesigner 2025 (build 2025.32820) or newer — see "Supported TD versions" below.

## Prerequisite — vendor the TD C++ SDK (once)

The TD C++ SDK is Derivative "Shared Use License" and is **not committed** (`../.gitignore`).
Copy it from your local TD install into `../vendor/`:

```sh
cp -R "/c/Program Files/Derivative/TouchDesigner/Samples/CPlusPlus/CHOP/." ../vendor/
```

## Build

```sh
just build-td-windows   # -> runtimes/touchdesigner/build/win/{SahouOut,SahouIn,sahou_transport}.dll
```

This builds `sahou_core` (static, capi) and `sahou-transport` (cdylib) for
`x86_64-pc-windows-msvc`, then runs CMake to link them into the two DLLs and co-locates
`sahou_transport.dll` next to them.

## Package

```sh
just package-td-windows          # -> dist/sahou-td-windows-x64-0.0.1.zip (default version arg)
just package-td-windows 0.0.2    # the current TD release version
```

## Load in TouchDesigner

Put all three DLLs together in a folder TD scans (per-`.toe` `Plugins/`, the user-global
`%USERPROFILE%\Documents\Derivative\Plugins\`, or `TOUCHDESIGNER_PLUGIN_PATH`),
then restart TD. `SahouOut.dll` / `SahouIn.dll` load `sahou_transport.dll` from alongside them,
so it must be in the same folder.

During development, **symlink** the built DLLs into the user-global Plugins folder instead of
copying — a rebuild then only needs a TD restart. Needs Windows Developer Mode (or an elevated
shell) for symlink creation:

```powershell
$plugins = "$env:USERPROFILE\Documents\Derivative\Plugins"
$src = "runtimes\touchdesigner\build\win"
foreach ($dll in 'SahouOut.dll','SahouIn.dll','sahou_transport.dll') {
    New-Item -ItemType SymbolicLink -Force -Path "$plugins\$dll" -Target "$PWD\$src\$dll" | Out-Null
}
```

(If your Documents folder is redirected to OneDrive, `$env:USERPROFILE\Documents` still resolves
to it.) Then restart TD (a running TD locks loaded DLLs).

## Supported TD versions

A Custom OP bakes in a fixed API version from the SDK it is compiled against
(`CHOPCPlusPlusAPIVersion`); TD loads it only if that version is within TD's supported range.
A **newer** TD than the build SDK loads fine (backward compatible); an **older** TD is rejected
(it does not know the newer API) and the op shows as a red error node. So the build here targets
the SDK of the **oldest TD we support** — currently TouchDesigner 2025 (build 2025.32820). To
lower the floor, vendor an older TD's SDK and rebuild.

## Signing

None required. Windows loads unsigned Custom-OP DLLs; SmartScreen / Mark-of-the-Web gates
downloaded `.exe`/`.msi` installers, not plugin DLLs dropped into a Plugins folder. Authenticode
signing is an optional future nice-to-have (AV false-positives / trust).
