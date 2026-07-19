# Roadmap — alpha to 1.0

Sahou is currently **alpha** (0.0.x): APIs and the wire contract may still change
without notice. This document describes what has to be true — not when — for it
to leave alpha. Releases are gated on criteria, not dates.

## What 1.0 means

1.0 is a **compatibility promise, not a feature level**: the wire protocol and
the IR schema are frozen as v1, a conformance suite exists so third parties can
verify their own integrations, and breaking changes thereafter require a major
version with a migration path. Everything below works backwards from that
promise.

## Phases and gates

### Alpha (now, 0.0.x)

Goal: build the verification machinery while the contract can still move.
Breaking changes are allowed.

Exit criteria (→ beta):

- [ ] CI runs the full suites on an OS matrix — Linux, macOS, Windows — for the
      Rust workspace and the TypeScript runtime (unit + cross-language +
      real-browser e2e).
- [ ] A release-rehearsal checklist exists (see Layer 2 below) and has passed
      once on a real mixed-OS mesh.
- [ ] No breaking wire-protocol changes are pending.

### Beta (0.x)

Goal: the contract is believed stable; verification is routine, not an event.
Wire changes are additive only.

Exit criteria (→ RC):

- [ ] Version-skew tests in CI: an n−1 runtime talking to an n runtime, since
      mixed-version meshes are the normal case on a LAN, and per-connection
      compatibility is a core feature.
- [ ] Docs cover the full journey: install → first mesh → triage with
      `sahou doctor`.
- [ ] Known issues are documented per platform (documented beats zero).
- [ ] Verified in real production use (an actual installation running unattended,
      not a desk test).

### RC

- Cut `vX.Y.Z-rc.N` on every channel (CLI/crates.io, PyPI, npm).
- Install from the **real registries onto clean machines** — the install path
  breaks more often than the code does.
- Acceptance on each platform = the smoke run (below) passing.

### 1.0

- [ ] Wire-protocol SPEC v1 written and frozen, with a published conformance
      suite (the "embed Sahou anywhere" kit: C ABI / WASM binding guides and
      the protocol convention for building directly on Zenoh).
- [ ] Compatibility policy documented: what counts as additive, and how a
      breaking change would ship.

## Verification strategy — two layers

**Layer 1 — CI, on every PR (automated).** The existing suites are
loopback-based and multicast-free by design, so they run on hosted runners;
the work is widening them to an OS matrix. This layer catches per-OS runtime
differences — a class of bug this project has already met twice (a zenoh-ts
close race visible only on Linux; a Windows IPv4/IPv6 listen mismatch in the
link).

**Layer 2 — release rehearsal (manual, checklist-driven).** What CI can't
reach: real multicast discovery, Wi-Fi AP client isolation, multi-NIC hosts,
OS firewall prompts, and mixed-OS / mixed-runtime meshes (Python ↔ TypeScript
↔ browser ↔ CLI). The acceptance test is Sahou's own tooling: a
`sahou doctor --lan` roll call over the mesh, plus the browser fixture demo.
Planned as a single smoke command so that "verify on a new machine" is one
command and one checklist page.

## Platform envelope

Verification targets what Sahou is positioned for, and no more:

- **Platforms (1.0):** macOS (arm64), Windows (x64), Linux (x64).
- **Runtimes (1.0):** CLI, Python, TypeScript (Node + browser).
- **Scale:** small-to-mid installations on one LAN segment (up to a few dozen
  nodes). Larger topologies are explicitly out of scope for 1.0.
- **TouchDesigner plugin:** remains experimental; not part of the 1.0 gate.

## Known risks being tracked

- zenoh-ts `Session.close()` does not await its inner close (upstream);
  test-side mitigation is in place, a proper fix is tracked separately.
- Multicast behavior varies widely by network environment — mitigated by
  `doctor --lan` diagnostics and the explicit `--connect` mode rather than by
  pretending discovery always works.
- Windows CI toolchain friction (browser installs, wasm builds, process
  handling) is expected cost, not a blocker.

Release mechanics (tags, channels, secrets) live in [RELEASING.md](RELEASING.md).
