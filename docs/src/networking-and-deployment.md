# Networking & deployment

Sahou's transport is built on [Zenoh](https://zenoh.io). You do not configure
addresses or ports in the contract — nodes discover each other by name on a
shared LAN. Transport and encoding are **adapter-swappable**: the contract and
the IR are transport-independent, and encoding is a per-connection attribute
(JSON by default).

## Zero-IP discovery

On a shared LAN, discovery is automatic. The [endpoints file](concepts/contract-vs-endpoints.md)
is empty or nearly so; you only name a router or an explicit endpoint when the
network requires it. Moving a node from one machine to another changes the
endpoints, never the contract.

## `sahou link`

`sahou link` is **one relay per machine plus a WebSocket entrypoint** for the
Node and browser runtimes (built on Zenoh's remote-api). Native runtimes (Python,
the core) speak Zenoh directly; Node and browser reach the LAN through the link's
WebSocket.

- The **Node runtime auto-spawns** a link if one is not already running, so you
  rarely start it by hand.
- The **browser cannot spawn** one — it connects to a link that is already
  reachable, and returns a NO with startup steps when it is not.
- By default the link exits on its own after a grace period with no clients, so
  it does not linger.

Run `sahou link --help` for the WebSocket port (default `10000`), the native peer
port (default `7448`), NIC pinning, and grace/startup timers.

## Exposing the browser path safely

The link's WebSocket is a LAN entrypoint, not an authenticated public endpoint.
Two patterns, depending on trust:

- **Trusted LAN (direct).** Browser clients on a network you control connect to a
  link on that network.
- **Untrusted or public clients (recommended: a BFF).** Put a backend-for-frontend
  in front: a server-side app uses the Node runtime and speaks Sahou, while the
  browser talks to that app over your own authenticated API. The link's WebSocket
  stays private.

The guiding rule: **do not expose the link's WebSocket to an untrusted network.**
Put authentication and network isolation outside Sahou (a gateway in front, or a
segmented network).

## `sahou doctor`

When discovery or connectivity misbehaves, `sahou doctor` runs an environment
preflight: it probes loopback, ping, this binary's real Zenoh scout (surfacing
permission/NIC problems), and the link's WebSocket reachability.

```bash
sahou doctor
```
