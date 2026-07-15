# Contract evolution

Contracts change. A node gains a field; a new receiver is added; a message grows.
Sahou decides, per connection, whether a change is safe to roll out while peers
are still running.

## Additive passes, breaking is a NO

Compatibility is judged **per connection** and **structurally**:

- **Additive changes pass.** Adding an optional field, or otherwise widening in a
  structurally-compatible way, is allowed — old and new peers can coexist.
- **Breaking changes are a NO.** Removing or retyping a field in a way that would
  make existing messages invalid is rejected.

Because the judgement is structural and per-connection, one connection evolving
does not force every peer to upgrade in lockstep.

## The delivery handshake

The compatibility decision is made by the **delivery handshake**. When peers
meet, they compare their views of a connection's contract and decide whether they
are compatible before traffic flows. A compatible (additive) difference is
accepted; an incompatible (breaking) one is refused — the same "say NO early"
principle, applied to two peers that were built against different versions of the
contract rather than to a single bad message.

This is why you can evolve a running system additively with confidence: the
handshake, not luck, decides that an old subscriber and a newer publisher still
agree.
