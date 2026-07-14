# Sahou monorepo tasks — run `just <task>` (install just: `brew install just` or `cargo install just`).
# Each recipe line runs in its own shell, so multi-step `cd` chains are joined with &&.

# List available tasks (default).
default:
    @just --list

# --- Rust core / cli ---

# Build the whole Cargo workspace (core + cli).
build:
    cargo build --workspace

# Run all Rust tests (core + cli).
test:
    cargo test --workspace

# Install the `sahou` CLI (run `just gui-build` first, or use `just install-full`, to embed the latest GUI).
install:
    cargo install --path cli --force

# --- GUI (compiled into the cli binary via rust-embed) ---

# Build the in-browser core wasm + the Vue app into gui/dist (what the cli embeds).
gui-build:
    cd gui && npm ci && npm run build:core && npm run build

# Copy the built GUI (gui/dist) into the cli crate so rust-embed can bundle it (release/publish pipeline).
gui-embed:
    rm -rf cli/gui-dist && mkdir -p cli/gui-dist && cp -R gui/dist/. cli/gui-dist/

# Run the GUI unit tests (vitest).
gui-test:
    cd gui && npm test

# Build the GUI, then install so the binary carries the freshest GUI assets.
install-full: gui-build gui-embed install

# --- Generated artifacts (committed) ---

# Regenerate the committed demo IR + type stubs (keep them fresh; stub_freshness test guards this).
gen-demo:
    cargo run -p sahou-cli -- gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang python --node sensor
    cargo run -p sahou-cli -- gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang ts --node visuals

# Regenerate the third-party license notice embedded in the cli (needs `cargo install cargo-about --features cli`).
licenses:
    cd cli && cargo about generate about.hbs -o licenses/THIRD-PARTY-LICENSES.md

# --- Runtimes: publish (placeholders — wire up once npm org / PyPI name are secured) ---

# publish-npm:
#     cd runtimes/ts && npm run build:core && npm run build && npm publish --access public
# publish-py:
#     cd runtimes/py && maturin publish

# --- FFI (C ABI for C++/Go/TouchDesigner) ---

# Build the static lib + regenerate the C header (feature = capi).
# Output: target/release/libsahou_core.a + core/sahou.h (needs `cargo install cbindgen`).
build-ffi:
    cargo build -p sahou-core --release --features capi
    cd core && cbindgen --config cbindgen.toml -o sahou.h

# --- TouchDesigner op (macOS .plugin) ---
# Needs the TD C++ SDK vendored into td/vendor/ (Derivative Shared Use License — see td/README.md).

# Regenerate the TD demo descriptor (source of truth = td/examples/schema.sahou.yaml).
gen-td-demo:
    cargo run -p sahou-cli -- gen td/examples/schema.sahou.yaml --out-dir td/examples/gen

# Run the op's TD-independent tests (pure payload/envelope) + the FFI smoke test.
test-td: build-ffi gen-td-demo
    td/test/run.sh

# Build the Sahou Out CHOP .plugin (arm64) into td/build/Release/SahouOut.plugin.
# Bundles the zenoh transport dylib into the plugin (Contents/Frameworks) and ad-hoc re-signs it.
build-td-macos: build-ffi
    cargo build -p sahou-transport --release
    install_name_tool -id @rpath/libsahou_transport.dylib target/release/libsahou_transport.dylib
    xcodebuild -project td/macos/SahouOut.xcodeproj -target SahouOut -configuration Release SYMROOT="$PWD/td/build" build
    mkdir -p "td/build/Release/SahouOut.plugin/Contents/Frameworks"
    cp target/release/libsahou_transport.dylib "td/build/Release/SahouOut.plugin/Contents/Frameworks/"
    codesign -f -s - "td/build/Release/SahouOut.plugin/Contents/Frameworks/libsahou_transport.dylib"
    codesign -f -s - "td/build/Release/SahouOut.plugin"
