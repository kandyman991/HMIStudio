# REUSE-02C.3 — supervised EngineDriver, isolated read integration

Status: candidate; independent review and exact-head hosted checks pending.
Base: PR #18 merge 9dc343b052f456ad507dfc69e2a82fb28345f108.
No engine or wire protocol changes, new dependencies, project schema changes, or physical endpoints.

## Scope and decisions

The native caller explicitly registers EngineDriverFactory for Simulator, supplying an absolute executable path outside the project. The existing default in-memory simulator remains unchanged. This first adapter accepts 1–4 unique scalar read-only synthetic tags; it refuses endpoint/credential configuration, duplicate addresses, unsupported types, writable tags and explicit initial values. Synthetic cells use tag IDs, with request address/type verification; zero initialization is the imported loopback behavior. It does not implement arbitrary addresses, aliases, physical drivers, writes, pulse termination, UI wiring or automatic arming. This is an implementation limitation, not a production approval gate for simulation; the existing in-memory simulator's writes remain available.

Protocol-3 mapping: bool→bool; signed 8/16/32 and unsigned 8/16→int; signed 64 and unsigned 32/64→long; float32→float, float64→double, string→string. UInt64 above i64::MAX is refused on encoding. Checked integer narrowing/unsigned conversion on decoding; exact variant matching, non-finite float refusal; no integer-to-float conversion. DateTime/array/struct are unsupported. Encode is a tested conversion primitive, not an exposed write command.

Connection startup requires complete good baseline and canonical decoding before admission. Reads check generation and live transport; invalid source/transport invalidates the generation and returns connection loss so TagEngine marks the connection bad. TagEngine's normal reconnect policy creates a fresh child and baseline. Last known values retained as bad diagnostics are not fresh values. Polling observes failure on next read; no continuous monitoring or real-time guarantee. Cancellation is propagated through startup/handshake/configure/start using short bounded receive waits and absolute deadlines, and baseline waiting is interruptible. No replay. Read-only direct child termination remains as before.

## Validation

Linux isolated locked/offline: 85 ordinary tag-engine tests passed; strict Clippy, rustdoc, fmt and lockfile/diff checks passed. Four new portable/Unix tests cover exact JSON round trips (including >2^53 and integer extrema), refusal, configuration isolation, child loss -> bad quality -> fresh changed baseline, and cancellation of stalled handshake. Existing real-child test plus new end-to-end TagEngine test both passed locally using an existing previously built engine binary; this is smoke evidence, not a freshly built exact-source executable acceptance. Initial smoke assertion incorrectly expected readable cached state after shutdown; corrected to the documented ShutDown error and reran. Existing dedicated hosted workflow explicitly builds unchanged engine source and runs both ignored real-child tests on Windows and Linux; no workflow change needed.

Remaining: independent review of exact candidate, hosted Required CI, dedicated tag tests/real child and audits, handoff. Broader driver/project mapping, simulation writes with explicit termination semantics, source quality propagation beyond this good-only loopback, and physical/hardware and UI acceptance remain separate.

## Independent corrective review

On predecessor `8a78179c5f7b3a702ecf68c979168e2c3bf47e69`, static inspection found that post-baseline scalar range failures could leave a generation connected while returning a per-item error. Validate all decoded values in each refreshed snapshot, not just the requested read group; invalidate and report connection loss for invalid snapshots before publishing any result. Added a synthetic child regression emitting a negative Long for UInt32 after a valid baseline, expecting bad quality, faulted connection and a fresh changed baseline. Hosted verification pending; reviewer has not executed this new regression locally.
