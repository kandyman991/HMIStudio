# REUSE-02C.1 — Bridge supervision: bounded commands, session invalidation, uncertain writes

Status: **local candidate, uncommitted**, on branch `feat/reuse-02c1-bridge-supervision`.
Base: `5fa32afa14d2b5d765e6563bef9a840c4adf2b2a` (main after PR #7). Nothing is committed,
pushed or reviewed, and hosted CI has not run on any of it. Simulated drivers only. No PLC,
network device or production acceptance is claimed. The protocol stays at version 3 and the
Kotlin client is unchanged.

## 1. Scope

The working tree already had uncommitted changes when this session started:

- `main.rs`: bounded transport and frame limits;
- bounded engine channels and `EventSink`;
- `tag-engine` `bridge_frame` and `bridge_supervisor`;
- the O-5 decision entry.

This session kept all of them and added the following.

| Requirement | Implementation |
| --- | --- |
| Bound the S7 driver command queue | `std::sync::mpsc::sync_channel(COMMAND_QUEUE)` with `COMMAND_QUEUE = 8`. `write` uses `try_send`; a write that does not fit is refused at once as **not transmitted**. A demand change sends at most one coalesced `Wake` (`wake_pending` flag), so demand churn can never fill the queue. |
| Shutdown not blocked by queue saturation | `stop()` no longer uses the queue. It raises `Control.stopping` and drops the only sender. The join is bounded by `3 × timeoutMs + 1 s`; after that the thread is detached and the flag keeps it from transmitting anything further. The read-span loop and the back-off also check the flag. In the engine, a `shutdown` received while a driver operation is in progress ends the wait at once instead of queuing behind it. |
| No stale queued commands after session invalidation | Before each transmission (`transmit_refusal`), a queued write is refused as not transmitted in three cases: the driver is stopping, the requester has stopped waiting (`oneshot` reply closed), or it was queued before the current connection was established. After a stop, the remaining queue is drained and refused. In the engine, once the request channel or output is closed, or event delivery has failed, buffered and deferred requests are dropped and never handled. |
| Client disconnection during driver operations | `engine::supervise` awaits `write` and `browse` while watching: output closed, event-sink failure, request channel closed (polled every 10 ms), and `shutdown`. Other requests that arrive meanwhile are deferred in order, at most `DEFERRED_MAX = 32`. When the session ends, the operation future is dropped. A driver that has not transmitted yet withdraws the write. The session then ends through `stop_all`. |
| Pulse release preserved; uncertain outcomes reported | A failed transmission is `DriverError::outcome_unknown` (stable prefix `OUTCOME_UNKNOWN: `). Certain non-transmission is `DriverError::not_transmitted` (`NOT_TRANSMITTED: `). An abandoned write is reported, best effort, as `DRIVER_ERROR` with an `OUTCOME_UNKNOWN` message. A pulse start with unknown outcome, including one abandoned on disconnect, registers the release immediately. The FALSE is written by the normal pulse path or by `stop_all`, and `pulseReleased` says it followed an unknown start. |
| No automatic write replay or re-arming | No code path retries a write. Reconnection never resends: queued-before-connection writes are refused. `stop`, `configure` and a new session all start disarmed (existing behaviour, now under regression test). |

Support for testing:

- `driver::DriverFactory` and `#[doc(hidden)] engine::run_with_drivers` let tests substitute a
  scripted simulated driver. `engine::run` is unchanged for callers.
- A crate-private `Plc` trait over `rust7::S7Client` lets the S7 thread run against a simulated
  PLC in unit tests. The production path still uses `rust7`.

Corrections to the earlier uncommitted work:

- `main.rs`: the reader loop was rewritten as `while let` to satisfy strict Clippy
  (`while_let_loop`). Behaviour is unchanged.
- `tag-engine/tests/bridge_supervisor.rs`: intermittent failure fixed (§4).

## 2. Regression tests added

**Simulated PLC: `crates/hmi-engine/src/driver/s7.rs`, module `supervision_tests` (7 tests).**
Each blocking point is a gate opened by the test.

1. `the_command_queue_is_bounded_and_overflow_is_not_transmitted`
2. `stop_is_not_blocked_by_a_saturated_queue_and_drops_queued_writes`: the thread is stuck in a
   transmission and the queue is full. `stop` returns within its bound, nothing queued is
   transmitted afterwards, and a later write is refused.
3. `stop_with_a_full_idle_queue_returns_promptly`
4. `a_withdrawn_request_is_not_transmitted`
5. `a_write_queued_before_the_connection_is_not_transmitted`
6. `a_failed_transmission_is_outcome_unknown_and_never_replayed`: the connection is rebuilt and
   nothing is resent.
7. `demand_changes_never_fill_the_queue`

**Scripted engine driver: `crates/hmi-engine/tests/session_safety.rs` (6 tests).**

1. `client_disconnect_during_a_write_ends_the_session_and_drops_queued_requests`
2. `shutdown_is_not_queued_behind_an_active_write`
3. `an_unknown_outcome_is_reported_and_never_retried`
4. `a_pulse_start_with_unknown_outcome_is_still_released`
5. `a_client_disconnect_during_a_pulse_start_still_releases_it`
6. `arming_is_never_restored_automatically`

### Fault-injection check

Each defect was re-introduced one at a time and the targeted tests run; the sources were then
restored and compared byte for byte.

| Re-introduced defect | Result |
| --- | --- |
| withdrawn write transmitted | caught |
| pre-connection write transmitted | caught |
| stop join unbounded | caught (test exceeded its bound) |
| wake not coalesced | caught |
| uncertain pulse start not released | caught (2 tests) |
| shutdown queued behind active write | caught |
| deferred requests executed after disconnect | caught |
| transmit-time stop check removed | **not caught**: the loop-head stop check refuses the same writes in every tested interleaving. It is kept as a second line of defence; see finding F-6 |

## 3. Results

Local run on 2026-11-24, Linux x86_64, rustc 1.100.0 and cargo 1.100.0, `--locked`.

| Check | Result |
| --- | --- |
| `cargo fmt --check` (hmi-engine, tag-engine) | clean |
| `cargo clippy --locked --all-targets -- -D warnings` (hmi-engine, default features) | clean |
| `cargo clippy --locked --all-targets --features test-server -- -D warnings` (hmi-engine) | clean |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` (tag-engine) | clean |
| hmi-engine `cargo test --locked --features test-server` | **53 passed, 0 failed** |
| hmi-engine `cargo test --locked` (default features) | **35 passed, 0 failed** |
| tag-engine `cargo test --locked` | **61 passed, 0 failed** (doc-tests: 0) |
| New tests repeated 5× | 5/5 green (7 + 6 each run) |
| tag-engine `bridge_supervisor` parallel repeat after fix | 0 failures in 200 runs (before: 41 in 100) |

hmi-engine with `test-server`, by target:

| Target | Passed |
| --- | --- |
| lib | 16 (9 address/OPC UA unit tests and 7 new) |
| contract | 4 |
| opcua | 9 |
| process | 5 |
| pulse | 4 |
| s7 (snap7 simulated PLC) | 9 |
| session_safety | 6 |

Baseline before this session, on the same tree: hmi-engine 40 and tag-engine 61, all passing.

## 4. Test-fixture race fixed (tag-engine)

`bridge_supervisor` tests failed intermittently, in 41 of 100 whole-suite runs, but never when
run one at a time (0 of 300) or single-threaded (0 of 60).

The cause is Linux `ETXTBSY` (`ExecutableFileBusy`). A fixture script is written while another
test thread forks a child. The fork briefly inherits the fixture's write descriptor until that
child execs, and exec of a file open for writing fails.

The test helper `start_supervisor` now retries only `ExecutableFileBusy`, up to 50 times at
10 ms intervals. No child exists when this error occurs, so the retry cannot mask a supervisor
fault. Production launches installed binaries and does not write executables at run time.

## 5. Limitations and remaining findings

- **F-1 Arming across reconnection.** Arming survives a source reconnection, bounded by its TTL
  as before. Whether loss of a source connection should disarm needs an owner decision.
- **F-2 Release after an unknown start can itself fail.** If the transport failed, the S7 source
  is in back-off and the immediate FALSE is refused as not transmitted. The engine reports
  `released: false` and does not retry, because no automatic writes are allowed. The PLC may
  keep TRUE asserted. A PLC-side command watchdog or an owner decision on "release on
  reconnect" is required before physical machinery is used.
- **F-3 Release writes during `stop_all` are bounded only by the driver.** For S7 this is
  `timeoutMs` per call. The OPC UA driver's write and cancellation bounds were not reviewed in
  this milestone. A hung non-S7 driver could delay shutdown.
- **F-4 Detached thread.** After the stop bound, a detached S7 thread may complete the one
  transmission already in progress, which cannot be recalled. It transmits nothing further.
- **F-5 Protocol 3 has no explicit "outcome unknown" code.** It is carried as `DRIVER_ERROR`
  plus the `OUTCOME_UNKNOWN: ` message prefix, and existing Kotlin clients treat it as a
  refusal. Recommend an explicit code in protocol 4 (O-9). Protocol 3 is unchanged.
- **F-6 Redundant stop check.** The transmit-time stop check is defence in depth and was not
  separately provable with deterministic interleavings.
- **F-7 Flood behaviour.** A client that keeps sending during a long driver operation is not
  read beyond 32 deferred requests. Its transport queue then saturates and the session ends
  (earlier bounded-transport design). The client must reconnect and re-arm explicitly.
- **F-8 Bridge not wired.** The `tag-engine` supervisor still has no request correlation, typed
  event dispatch, bounded subscription delivery or resync, and is not used by Studio. Those
  remain REUSE-02C scope.
- **F-9 Evidence scope.** Local Linux evidence only. Windows, hosted CI and independent review
  have not run. No physical PLC, network device or packaged sidecar was used.

## 6. Next exact action

1. Owner or reviewer: inspect the working tree.
2. Commit it on `feat/reuse-02c1-bridge-supervision` when authorized.
3. Run exact-head hosted Windows and Linux CI with the dedicated audit, then independent review.
4. Decide F-1 and F-2 before any physical-machinery write.
5. Continue REUSE-02C with bounded and coalesced subscription delivery with resync, request
   correlation, and typed events in the supervisor.
