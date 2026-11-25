# REUSE-02C.2 — Protocol dispatch foundation

Status: **first implementation candidate; not milestone acceptance**.
Base: `dbdeeb2ea74555ba879380168e17c923a1c41da2` (PR #17 merged).
No new dependencies, lockfile changes, engine protocol changes, Kotlin changes, packaging, or real-device access.

## Implemented

- Protocol-3 typed inbound message mirror, checked against the already-authorized shared messages fixture. Preserves signed 64-bit values, rejects unknown message kinds/fields and malformed frames. No in-process driver linkage.
- One outstanding request per supervisor; monotonic session-local IDs, reply kind and ID matching, error reply matching, one absolute timeout across intervening events. Unmatched, stale and duplicate replies invalidate the session. No replay or automatic restart.
- Bounded event mailbox: 32 messages and 4 MiB of serialized payload accounting; raw transport retains its separate 8-frame queue and 1 MiB frame limit. This is a logical bound, not an exact heap-byte measurement. Overflow clears the entire partial backlog, invalidates the session and requires a new handshake; the transport has a sticky fault flag even when an error cannot be queued.
- Typed event delivery by polling. Handshake and a read-only browse operation are exposed for fixture tests; configure/start, arming, write and pulse are not exposed. Clean output closure is detected separately from transport failure.
- The old single-session kill/reap behavior remains restricted to this scope. No live write-capable integration or graceful write termination is claimed.

## Local validation

Linux isolated checkout, locked/offline: 70 tag-engine tests passed; strict Clippy, rustdoc and formatting passed. Added 5 portable dispatcher tests and 4 Unix fixture subprocess tests. The portable tests cover shared wire fixture round trips, interleaved events, monotonic IDs, exact long values, malformed/duplicate/wrong replies and event-count/byte exhaustion. Subprocess tests cover correlated browse, stale reply, timeout and output closure. Exact-head hosted CI and independent review pending. Existing Unix-only subprocess tests do not constitute Windows process acceptance.

## Remaining work in REUSE-02C.2

- Coalesced bounded tag subscriptions with explicit lag semantics; existing in-process TagEngine subscriptions are unchanged.
- Generation-scoped cache invalidation and a complete, authoritative resynchronization baseline; a new handshake is necessary but not sufficient. Protocol 3 delta events do not themselves define a complete snapshot boundary. Establish the baseline with isolated simulator configuration/start and documented completeness rules before serving UI data.
- Cross-platform child fixture, timeout under event-only traffic, transport saturation and repeated-session isolation tests.
- Exact numeric mapping and safe refusal at the EngineDriver boundary. Preserve unknown outcome semantics, no automatic writes/rearming, and F-1/F-2 decisions before physical writes. No production approval for simulation.
- Separate subsequent review and exact-head CI before declaring this milestone complete.

Next: review this scoped foundation, complete bounded/coalesced subscriptions and resync on an explicitly identified isolated simulator; do not attach to Studio or physical machinery.
