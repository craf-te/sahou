# Sahou GUI (`sahou gui`)

The production node editor.
The backend only bridges raw bytes; all contract interpretation happens in the core wasm inside the
browser (core-bridge, single entry point).

## Dev loop

```powershell
# 1) Backend (file API + watch. cwd = the directory being edited)
cargo run -p sahou -- gui examples/demo          # http://127.0.0.1:4649

# 2) Frontend dev server (proxies /api to 4649 · HMR)
cd gui
npm install
npm run build:core                                # core wasm (wasm-pack --target web)
npm run dev                                       # http://localhost:5179
```

## Production build (bundled into the binary)

```powershell
cd gui
npm run build:core
npm run build            # vue-tsc --noEmit (app + tests) + vite build → gui/dist/
cd ..
cargo build -p sahou --release   # rust-embed bundles gui/dist/
```

Note: in a debug build `rust-embed` reads `gui/dist/` as real files every time, but a release build
**embeds them into the binary at compile time**. So after rebuilding dist you must re-run
`cargo build --release` (otherwise the binary keeps the old assets).

## Regenerating the core wasm

`gui/src/core-wasm/` is produced by `npm run build:core` (which internally runs
`wasm-pack build ../core --release --target web --features wasm`). Whenever you change the core's
(`core/`) types / validate / codegen logic, you must re-run this command before building the GUI to
rebuild the core wasm (running `npm run build` / `npm run dev` with a stale wasm keeps interpreting
the contract with the old logic).

## Tests

```powershell
cd gui && npm test                                # Vitest (core-bridge/edits/store/graph/panes/App)
cargo test -p sahou                               # backend (files/serve)
cargo test -p sahou-core --features wasm          # core wasm ABI
```

## Manual E2E checklist (run all items before a release)

Target: `sahou gui examples/demo` (requires schema.sahou.yaml. layout.sahou.json / endpoints.dev.yaml may be absent)

- [ ] load: 4 nodes appear in the graph. touch = gray solid / points = same / debug_tap = red dashed / get_state = purple ⇄
- [ ] edit→autosave: change a field name → after about 0.4s the status bar shows "autosaved ✓" → reflected in `git diff examples/demo/schema.sahou.yaml`
- [ ] layout: drag a node → layout.sahou.json is created and holds only coordinates (no coordinates in the contract)
- [ ] reflecting external edits: change and save the version in schema.sahou.yaml in an editor → the GUI shows "reflected the external edit ✓"
- [ ] conflict (watch detects ahead of time): edit a field name in the GUI and, within 0.4s, save the file externally → the conflict dialog appears.
      "Show diff" shows the line diff · "Keep local edits" makes the GUI side win · redo it and "Discard and reload" makes the external side win
- [ ] 409 (last line of defense): since we can't kill the watch, save externally repeatedly while editing in the GUI → no overwrite loss occurs (you always land on one of the conflict paths)
- [ ] comment warning: add a `# comment` to the schema and reload → the persistent banner appears. Edit in the GUI → after save the comment is gone and the banner disappears
- [ ] broken YAML: create a duplicate key in the schema → the "cannot be read" screen + parse_error diagnostic + raw text display. Fixing it externally recovers automatically
- [ ] default UI: click "e.g." on touch.payload's x → a default is filled in → `default:` appears in schema.yaml. Entering `"abc"` produces invalid_default in the diagnostics tab
- [ ] selector: selecting get_state (query) shows the selector field. It does not appear on the pub_sub touch. The value entered lands in schema.yaml
- [ ] delivery: touch has "Reliable" lit (reliable+block). Switch to "Stream" → the reliability/congestion keys disappear from schema.yaml (default values)
- [ ] diagnostic jump: create an undefined node in `to` → click unknown_node in the diagnostics tab → the matching connection is selected. The keyexpr display becomes stale
- [ ] unwired advisory: create an unwired node with ＋Node → the status bar shows an "unwired node: …" hint (not counted as a diagnostic = NO)
- [ ] deploy tab: change namespace → endpoints.dev.yaml is created/updated. Reflected in the effective keyexpr display
- [ ] bus display: "Topic bus" → touch becomes 1+2 edges via a topic node. auto-layout arranges them and layout.sahou.json is updated
- [ ] port in use: launch another `sahou gui` → it exits immediately with the `[gui_port_in_use]` NO (no automatic fallback to another port)
- [ ] --open: `sahou gui examples/demo --open` opens the browser. Without it, only the URL is shown

The above involves real-browser G6 canvas interaction, so it is not automated (the judgment being that
for a single-user localhost GUI, adding a playwright dependency isn't worth the cost · design §8).
Before a release, a human should run through every item top to bottom.
