# sahou-core / sahou CLI

The Rust core of Sahou (the SoT). Parse / serialize / validate the contract (schema.sahou.yaml), derive the IR, and judge compatibility.

## Usage

    sahou validate examples/demo/schema.sahou.yaml
    sahou fmt examples/demo/schema.sahou.yaml [--write]   # note: comments are not preserved
    sahou gen examples/demo/schema.sahou.yaml --endpoints examples/demo/endpoints.dev.yaml

## Module map (core/src/)

- contract / typespec … the contract model (inline shapes, recursive types, deny_unknown_fields)
- parse / fmt … YAML⇄IR round-trip (NO on duplicate/unknown keys; deterministic canonical output)
- schema_check … schema self-validation (positional diagnostics)
- payload / sample … runtime payload validation (lazy path building) and valid-sample generation
- endpoints / ir … deployment layer (empty = LAN auto-discovery); keyexpr/hash/Descriptor derivation
- compat … structural compat classification + delivery handshake judgement
- wasm … bindings for the GUI (feature "wasm")

## Tests

    cargo test --workspace          # unit + proptest (512×2) + CLI integration
    cargo clippy --all-targets -- -D warnings
