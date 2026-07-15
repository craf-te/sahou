# Contract vs Endpoints

Sahou keeps three concerns in three separate files. Do not conflate them.

| File | What it holds | Who edits it |
|---|---|---|
| `schema.sahou.yaml` | The **contract** — meaning, types, wiring. The single source to edit. | Humans / AI / the GUI |
| `endpoints.<env>.yaml` | **Deployment** — per-environment connection settings. Empty by default. | Per environment |
| `layout.sahou.json` | GUI **coordinates** only. No bearing on the contract's meaning. | The GUI |

## The contract

`schema.sahou.yaml` is the one file that defines *what* the system communicates:
the nodes, the messages, and the connections between them. It is transport- and
environment-independent. This is the file you version, review, and share.

## The endpoints

`endpoints.<env>.yaml` says *which machine plays which node* and carries any
environment-specific settings, so the **same contract runs unchanged across
environments** (dev, staging, an installation on site). It is deliberately small
— often just a namespace — because **LAN auto-discovery is the default**:

```yaml
env: dev
namespace: sahou/demo
# LAN auto-discovery is the default. Only specify a router / explicit endpoint when needed.
```

When you omit endpoints entirely, Sahou uses the LAN-auto default (namespace
`sahou`).

## Why the split matters

Because deployment lives outside the contract, moving from your laptop to a
gallery network, or from one machine to three, does not touch the contract. You
change *where* nodes run without changing *what* they say. See
[Networking & deployment](../networking-and-deployment.md) for how discovery and
transport use these files.
