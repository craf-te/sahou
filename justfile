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
    cargo run -p sahou-cli -- gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang ts
    cargo run -p sahou-cli -- gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang python
    # Browser demo needs the browser-target connect (re-exports "sahou/browser"). It only needs the two stub
    # files; the descriptor it fetches at runtime is served from runtime/gen, so drop the redundant copy.
    cargo run -p sahou-cli -- gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/browser --lang ts --target browser
    rm -f examples/demo/runtime/browser/descriptor.json

# Regenerate the third-party license notice embedded in the cli (needs `cargo install cargo-about --features cli`).
licenses:
    cd cli && cargo about generate about.hbs -o licenses/THIRD-PARTY-LICENSES.md

# --- Runtimes: publish (placeholders — wire up once npm org / PyPI name are secured) ---

# publish-npm:
#     cd runtimes/typescript && npm run build:core && npm run build && npm publish --access public
# publish-py:
#     cd runtimes/python && maturin publish

# --- FFI (C ABI for C++/Go/TouchDesigner) ---

# Build the static lib + regenerate the C header (feature = capi).
# Output: target/release/libsahou_core.a + core/sahou.h (needs `cargo install cbindgen`).
build-ffi:
    cargo build -p sahou-core --release --features capi
    cd core && cbindgen --config cbindgen.toml -o sahou.h

# --- TouchDesigner op (macOS .plugin) ---
# Needs the TD C++ SDK vendored into runtimes/touchdesigner/vendor/ (Derivative Shared Use License — see runtimes/touchdesigner/README.md).

# Regenerate the TD demo descriptor (source of truth = runtimes/touchdesigner/examples/schema.sahou.yaml).
gen-td-demo:
    cargo run -p sahou-cli -- gen runtimes/touchdesigner/examples/schema.sahou.yaml --out-dir runtimes/touchdesigner/examples/gen

# Run the op's TD-independent tests (pure payload/envelope) + the FFI smoke test.
test-td: build-ffi gen-td-demo
    runtimes/touchdesigner/test/run.sh

# Build the Sahou Out + In CHOP .plugins (arm64) into runtimes/touchdesigner/build/Release/.
# Each .plugin bundles the zenoh transport dylib (Contents/Frameworks) and is ad-hoc re-signed.
build-td-macos: build-ffi
    cargo build -p sahou-transport --release
    install_name_tool -id @rpath/libsahou_transport.dylib target/release/libsahou_transport.dylib
    xcodebuild -project runtimes/touchdesigner/macos/SahouOut.xcodeproj -target SahouOut -configuration Release SYMROOT="$PWD/runtimes/touchdesigner/build" build
    mkdir -p "runtimes/touchdesigner/build/Release/SahouOut.plugin/Contents/Frameworks"
    cp target/release/libsahou_transport.dylib "runtimes/touchdesigner/build/Release/SahouOut.plugin/Contents/Frameworks/"
    codesign -f -s - "runtimes/touchdesigner/build/Release/SahouOut.plugin/Contents/Frameworks/libsahou_transport.dylib"
    codesign -f -s - "runtimes/touchdesigner/build/Release/SahouOut.plugin"
    xcodebuild -project runtimes/touchdesigner/macos/SahouOut.xcodeproj -target SahouIn -configuration Release SYMROOT="$PWD/runtimes/touchdesigner/build" build
    mkdir -p "runtimes/touchdesigner/build/Release/SahouIn.plugin/Contents/Frameworks"
    cp target/release/libsahou_transport.dylib "runtimes/touchdesigner/build/Release/SahouIn.plugin/Contents/Frameworks/"
    codesign -f -s - "runtimes/touchdesigner/build/Release/SahouIn.plugin/Contents/Frameworks/libsahou_transport.dylib"
    codesign -f -s - "runtimes/touchdesigner/build/Release/SahouIn.plugin"

# Package the macOS TD plugins into a distributable zip (LOCAL build only — the TD SDK is
# Derivative "Shared Use License", usable only on a licensed TD machine, so this is intentionally
# NOT a GitHub-hosted CI job). Bundles the license notices Apache-2.0 requires (incl. Eclipse
# Zenoh). Version is the arg (defaults to 0.0.1). Output: dist/sahou-td-macos-arm64-<version>.zip.
package-td-macos version="0.0.1": build-td-macos
    rm -rf dist/sahou-td-macos "dist/sahou-td-macos-arm64-{{version}}.zip"
    mkdir -p dist/sahou-td-macos
    cp -R runtimes/touchdesigner/build/Release/SahouOut.plugin dist/sahou-td-macos/
    cp -R runtimes/touchdesigner/build/Release/SahouIn.plugin dist/sahou-td-macos/
    cp LICENSE NOTICE runtimes/touchdesigner/INSTALL.txt dist/sahou-td-macos/
    cp cli/licenses/THIRD-PARTY-LICENSES.md dist/sahou-td-macos/
    ditto -c -k --keepParent dist/sahou-td-macos "dist/sahou-td-macos-arm64-{{version}}.zip"
    @echo "packaged dist/sahou-td-macos-arm64-{{version}}.zip"

# --- Docs (mdBook + i18n) ---

# Live-preview the English book at http://localhost:3000.
docs-serve:
    cd docs && mdbook serve

# Build both languages into docs/book/html (English at /, Japanese at /ja/).
docs-build:
    cd docs && mdbook build -d book/html && MDBOOK_BOOK__LANGUAGE=ja mdbook build -d book/html/ja

# Re-extract the translation template and merge into po/ja.po after editing English.
# (Needs GNU gettext: msgmerge. CI/Linux have it; on native Windows it may be absent.)
docs-i18n-update:
    cd docs && MDBOOK_OUTPUT='{"xgettext": {}}' mdbook build -d po && msgmerge --update po/ja.po po/messages.pot
