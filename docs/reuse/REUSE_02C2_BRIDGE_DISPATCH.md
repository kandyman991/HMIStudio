# REUSE-02C.2 — Protocol dispatch foundation

Status: **implementation candidate on PR #18; independent review and exact-head CI pending**.
Base: `dbdeeb2ea74555ba879380168e17c923a1c41da2` (PR #17 merged).
No new dependencies, lockfile changes, engine protocol changes, Kotlin changes, packaging, or real-device access.

## Implemented

- Protocol-3 typed inbound message mirror, checked against the already-authorized shared messages fixture. Preserves signed 64-bit values, rejects unknown message kinds/fields and malformed frames. No in-process driver linkage.
- One outstanding request per supervisor; monotonic session-local IDs, reply kind and ID matching, error reply matching, one absolute timeout across intervening events. Unmatched, stale and duplicate replies invalidate the session. No replay or automatic restart.
- Bounded event mailbox: 32 messages and 4 MiB of serialized payload accounting; raw transport retains its separate 8-frame queue and 1 MiB frame limit. This is a logical bound, not an exact heap-byte measurement. Overflow clears the entire partial backlog, invalidates the session and requires a new handshake; the transport has a sticky fault flag even when an error cannot be queued.
- Typed event delivery by polling. Handshake and a read-only browse operation are exposed for fixture tests. The follow-up below permits configure/start only for a generated isolated read-only loopback configuration; arming, write and pulse remain unexposed. Clean output closure is detected separately from transport failure.
- The old single-session kill/reap behavior remains restricted to this scope. No live write-capable integration or graceful write termination is claimed.

## Initial dispatch foundation validation

Linux isolated checkout, locked/offline: 70 tag-engine tests passed; strict Clippy, rustdoc and formatting passed. Added 5 portable dispatcher tests and 4 Unix fixture subprocess tests. The portable tests cover shared wire fixture round trips, interleaved events, monotonic IDs, exact long values, malformed/duplicate/wrong replies and event-count/byte exhaustion. Subprocess tests cover correlated browse, stale reply, timeout and output closure. Exact-head hosted CI and independent review pending. Existing Unix-only subprocess tests do not constitute Windows process acceptance.

## Bounded tag subscriptions and isolated resynchronization follow-up

- Replaced unbounded TagEngine channels with a bounded latest-state mailbox: maximum 64 live subscribers, 256 queued events per subscriber and 4 MiB of serialized payload per subscriber. These are conservative initial engineering limits, not an exact heap or latency guarantee.
- Pending tag changes coalesce by tag ID only within the latest uninterrupted sequence of change events; replacement moves to the tail so retained sequence numbers are ordered. Connection and lifecycle events are barriers. This is not a lossless alarm, audit or trend history.
- Admission takes a value snapshot and, for unfiltered subscriptions, current connection statuses under the documented lock order. A BaselineComplete event identifies the subscription generation and separates initial snapshots from subsequent changes. Oversized initial snapshots are rejected rather than partially published.
- Overflow clears queued data and emits ResyncRequired before disconnecting. Explicit unsubscribe discards its backlog. A fresh subscription gives a new generation and complete baseline. Consumers must discard cached data from invalid generations.
- SimulationBridge owns a fresh supervised process, monotonic generation and private baseline. It constructs only read-only loopback configuration, bounded to 1–4 uniquely identified scalar points; no endpoint, credentials, write or arming controls. A single absolute deadline spans handshake, configuration, start and baseline assembly. No publication until the named source is connected and every expected point has a GOOD type-correct value. Malformed, unexpected, bad-quality or missing data, source loss and transport failure fail closed.
- Refresh checks process/transport health and drains pending events with bounded work. Old generation tokens are refused without invalidating a newer healthy session. Reconnect is explicit fresh process + new complete baseline, with no command replay. Consumers may hold stale snapshot copies and must use the generation-aware refresh contract; this is not a hard real-time liveness guarantee.
- The dedicated Windows/Linux workflow now builds the actual engine binary with the unchanged lockfile and explicitly runs the real-child loopback test. No hardware is accessed and no new dependency or wire format is introduced.

## Follow-up local evidence (2026-11-25)

Prior head d2b9b96 passed Desktop CI 45569339342, tag CI 45569339255, handoff 45569339262. This is predecessor evidence only.
Final local locked/offline Linux tag-engine suite: 80 ordinary tests passed, plus 1 explicitly invoked real loopback subprocess test (81 total). Strict Clippy, rustdoc and fmt passed. Real hmi-engine built from the unchanged source in this checkout. Regression coverage includes a 2,000-update slow-consumer burst, ordering barriers, count/byte overflow, admission refusal, complete initial baseline, resubscribe and explicit unsubscribe, stale generations, incomplete baseline timeout, wrong type/quality/source failure and actual child restart. An intermediate concurrency assertion expecting every historical event was replaced with a latest-state invariant; the underlying concurrent read consistency check is retained. Intermediate deprecated-atomic warning corrected and strict preflight rerun.

## Remaining gates and scope

- Exact-head Windows/Linux CI and dependency audits, including the new real-child step, then independent review. The workflow change may trigger CI-02 full routing including installer jobs; this is not packaging acceptance.
- Full production EngineDriver integration, arbitrary project configuration, general PLC source reconnect/quality semantics, exact numeric conversion to canonical tag types, large-scale load validation, graceful write termination and Studio integration remain separate follow-ups. No physical PLC testing, automatic replay, re-arming or production approval requirement for simulation.
- The isolated loopback baseline is authoritative only for its explicitly configured small synthetic point set, not a new protocol-3 snapshot mechanism for arbitrary drivers. Existing transport limits remain unchanged. Existing Unix fixture tests are not Windows evidence until hosted CI runs the cross-platform real-child test.
