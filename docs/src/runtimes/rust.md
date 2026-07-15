# Rust

The app-facing Rust runtime — the counterpart of `sahou` on PyPI and npm — is
**not released yet**. The [`sahou`](https://crates.io/crates/sahou) crate on
crates.io is a **placeholder** that reserves the name; it has no functionality.

For now, build on the core:

```bash
cargo add sahou-core
```

- **`sahou-core`** — the schema core: the IR plus parse / serialize / validate,
  and the same pure functions the CLI and the other runtimes use. This is the
  crate to depend on today.
- **`sahou-cli`** — installs the `sahou` command-line tool.

When the Rust runtime lands, `cargo add sahou` will give you the same
`connect` / `publish` / `subscribe` / `query` surface as the other languages.
Follow the [project README](https://github.com/craf-te/sahou#readme) for status.
