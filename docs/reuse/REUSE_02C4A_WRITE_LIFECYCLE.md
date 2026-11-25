# REUSE-02C.4A — Write lifecycle and termination safety

Status: **candidate, draft PR**. Branch `feat/reuse-02c4a-write-lifecycle` from verified main
`9dc343b052f456ad507dfc69e2a82fb28345f108` (merge of PR #18). Only `crates/hmi-engine/` and
documentation are changed. The `crates/tag-engine/` EngineDriver adapter (PR #19, under
independent review) is untouched.

The work uses only a scripted simulated driver, the existing loopback driver and the existing
synthetic OPC UA and S7 test servers. Nothing connects to a physical PLC, no deployment is
involved, and nothing here is production-accepted.

## 1. Compatibility

- **Protocol 3 is unchanged.** No request or message type, field, refusal code or version
  changed. `contract.rs`, `messages.json` and `write-guard-cases.json` pass unchanged.
- **Kotlin runtime compatibility.** Every message emitted is one Kotlin already receives. Two
  behaviours are new, both through existing messages:
  - `writesArmed {armed:false}` is sent when a source loses its connection (§3, D-02C4A-1).
    Kotlin already receives this message on TTL expiry.
  - `writeResult {refusal: BUSY}` can carry an explanatory message for a command whose last
    release was not confirmed (§3, D-02C4A-2).
  
  These were not exercised against the Kotlin runtime in this milestone. That remains an
  explicit gap (F-9).
- **Development and simulation.** Writes in development and simulation remain available
  without any production approval. Arming is still the only gate.
- **Dependencies.** No dependency was added or changed. `Cargo.lock` is unchanged.

## 2. Inspection of REUSE-02C.1 findings

| Finding | 02C.4A disposition |
| --- | --- |
| F-1 arming survives a reconnection | **Resolved**: losing a connection disarms; reconnection never re-arms (D-02C4A-1) |
| F-2 release after an unknown start can fail, TRUE may stay asserted | **Mitigated, owner decision remains**: the release is attempted once, bounded and reported. The command is then blocked (`BUSY`) until an explicit re-arming, and the engine never writes FALSE again by itself (D-02C4A-2). A PLC-side command watchdog is still required before physical use |
| F-3 release and stop bounded only by the driver | **Resolved for the engine**: every release write and every driver `stop` is bounded by the engine, whichever driver it is |
| F-4 a detached S7 thread may finish one transmission | Unchanged; inherent to a synchronous client. The stop flag still prevents anything further |
| F-5 no explicit protocol-3 "outcome unknown" code | Unchanged by design (no protocol change). Carried as `DRIVER_ERROR` plus `OUTCOME_UNKNOWN:`; certain non-transmission is `NOT_TRANSMITTED:`. An explicit code is still recommended for protocol 4 (O-9) |
| F-6 redundant S7 transmit-time stop check | Unchanged (defence in depth) |
| F-7 flood behaviour (32 deferred) | Unchanged |
| F-8 bridge not wired | Out of scope here (`tag-engine`, PR #19) |
| F-9 evidence scope | Local Linux only. Hosted Windows and Linux CI comes from this PR. Kotlin interoperability is not exercised |

## 3. Implementation

**Time bounds (`engine::Limits`).** Every wait on a driver is bounded.

| Bound | Default | Effect when exceeded |
| --- | --- | --- |
| `write_timeout` | 10 s | A `write` or `pulse` start is withdrawn (its future dropped, so an untransmitted S7 command is withdrawn) and reported as `DRIVER_ERROR` / `OUTCOME_UNKNOWN:`. A timed-out pulse start is released at once. A `browse` answers `BROWSE_FAILED`. |
| `release_timeout` | 5 s, per release | `pulseReleased {released:false}` with `OUTCOME_UNKNOWN:`; the command is marked unconfirmed |
| `stop_timeout` | 5 s, per driver | The driver is dropped and termination continues |

A session with *n* running pulses and *m* sources therefore terminates within about
*n* × `release_timeout` + *m* × `stop_timeout` after the stop or shutdown begins.

`Limits` is engine-side only. Protocol 3 has no field for it, so clients cannot change it.
`run_with_limits` (`#[doc(hidden)]`) exists for tests. `run` and `run_with_drivers` keep their
signatures and use the defaults.

**Write outcomes.** `write_outcome` maps the supervised result to protocol-3 refusals:

- the driver answered → its result;
- session ended → `OUTCOME_UNKNOWN:` (the REUSE-02C.1 abandonment text);
- deadline passed → `OUTCOME_UNKNOWN:`, citing the bound.

Nothing is ever retried.

**D-02C4A-1: connection loss ends the control period (proposed).** When a source goes from
connected to any other state while the engine is running, two things happen:

1. Pulses of that source are released at once. The release is reported, and marked unconfirmed
   if it fails.
2. Arming is withdrawn: `writesArmed {armed:false, actor}`.

After reconnection, writes are refused with `WRITES_DISARMED` until the operator arms again.
Arming is engine-wide, so the loss of any one source disarms all sources. This is the
conservative choice; per-source arming would need a protocol change.

**D-02C4A-2: unconfirmed release (proposed).** A release that fails, times out or has no
started driver is reported as `pulseReleased {released:false}` with
`NOT_TRANSMITTED:` or `OUTCOME_UNKNOWN:`, and the command's variable is recorded as
unconfirmed. The engine never writes FALSE again by itself, not on reconnection and not on
re-arming. A new `pulse` for that command is refused with `BUSY` and an explanation. The next
explicit `armWrites` clears every unconfirmed mark; that is the operator's acknowledgement that
the device was checked. A later pulse then runs normally and ends with FALSE.

Whether to add an operator-initiated "release now" request is an owner and protocol-4 decision.

**Shutdown and termination.**

- `stop_all` releases every running pulse (bounded), disarms, then stops each driver (bounded),
  dropping any driver that overruns.
- `shutdown` during an active write still ends the wait at once (REUSE-02C.1). The write is
  reported as `OUTCOME_UNKNOWN` and deferred requests are dropped.

## 4. Regression tests

`crates/hmi-engine/tests/write_lifecycle.rs` has 13 tests with a scripted simulated driver and
short limits (300 ms each).

| Area | Test |
| --- | --- |
| Normal write and acknowledgement | `a_normal_write_is_acknowledged_once_and_audited` |
| Momentary command | `a_momentary_command_is_true_then_false_after_its_pulse_time` |
| Timeout and cancellation | `a_write_without_an_answer_times_out_as_outcome_unknown_and_is_withdrawn`, `a_pulse_start_that_times_out_is_released_at_once` |
| Uncertain outcomes | `certain_refusal_and_unknown_outcome_are_distinguished` |
| Connection loss, no re-arming | `connection_loss_disarms_and_reconnection_never_rearms`, `connection_loss_during_a_pulse_releases_it_and_disarms` |
| Failed or unconfirmed release | `an_unconfirmed_release_blocks_the_command_until_writes_are_armed_again`, `a_release_without_an_answer_is_bounded_and_reported` |
| Shutdown and termination | `shutdown_with_a_running_pulse_releases_it_before_stopping`, `shutdown_is_bounded_when_the_release_and_the_driver_hang`, `a_client_disconnect_terminates_within_bounds_with_a_hung_driver`, `shutdown_during_a_pending_write_is_immediate_and_nothing_follows` |

The REUSE-02C.1 tests (`session_safety.rs`, the S7 `supervision_tests`) and every earlier suite
still pass unchanged.

### Defect re-introduction

Each defect was re-introduced one at a time, the targeted tests were run, and the source was
then restored and compared byte for byte. **All 8 were caught.**

| Re-introduced defect | Failing test(s) |
| --- | --- |
| no write deadline | both timeout tests |
| timed-out pulse start not released at once | `a_pulse_start_that_times_out_is_released_at_once` |
| arming survives connection loss (F-1) | `connection_loss_disarms_and_reconnection_never_rearms` |
| loss does not release that source's pulses | `connection_loss_during_a_pulse_releases_it_and_disarms` |
| unconfirmed release not tracked (F-2) | `an_unconfirmed_release_blocks_…` |
| arming does not clear the unconfirmed mark | `an_unconfirmed_release_blocks_…` |
| unbounded release write | `a_release_without_an_answer_is_bounded_and_reported` |
| unbounded driver stop | `shutdown_is_bounded_when_the_release_and_the_driver_hang` |

## 5. Local validation

Linux x86_64, rustc and cargo 1.97.0, all commands in `crates/hmi-engine`, `--locked`.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features test-server -- -D warnings` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --features test-server` | clean |
| `cargo test --locked --features test-server` | **67 passed, 0 failed** |
| `cargo test --locked` | **49 passed, 0 failed** |
| `write_lifecycle` + `session_safety` repeated 10× | 10/10 green (13 + 7 each run) |
| `Cargo.lock` | unchanged (`git diff --exit-code`) |
| `cargo audit` 0.23.0, database fetched at run time, 221 crates | 0 vulnerabilities, 1 warning (RUSTSEC-2026-0543, `chrono` unmaintained, already documented) |

Test counts by target, with `test-server` (default features include the targets marked †):

| Target | Tests |
| --- | --- |
| lib † | 16 |
| contract † | 4 |
| opcua | 9 |
| process † | 5 |
| pulse † | 4 |
| s7 | 9 |
| session_safety † | 7 |
| write_lifecycle † | 13 |

Hosted Windows and Linux CI, routed by CI-02, comes from the PR. It is not claimed here.

### After reconciling with PR #19

PR #19 (REUSE-02C.3 EngineDriver) merged into main as `d1571fc` during this work. It was
reconciled by an ordinary merge: the only conflicts were in shared records, and both entries
were kept. Re-run on the merged tree:

| Check | Result |
| --- | --- |
| hmi-engine `cargo test --locked --features test-server` | 67 passed, 0 failed |
| tag-engine fmt and strict Clippy | clean |
| tag-engine rustdoc `-D warnings` | clean |
| tag-engine `cargo test --locked` | 86 passed, 0 failed, 2 ignored by design |
| Real-child tests (`--ignored`) against an `hmi-engine` freshly built from this branch | 2 passed (`real_child_integrates_with_tag_engine`, `real_loopback_baseline_and_new_generation`) |
| Both lockfiles | unchanged |

The EngineDriver remains read-only, so it does not yet exercise the new write semantics.

## 6. Limitations and open findings

- **L-1 Driver timeouts above the engine bound.** `write_timeout` (10 s) is shorter than the
  largest S7 `timeoutMs` (30 s). A slower S7 write is reported as outcome unknown at 10 s. If the
  S7 command was still queued, it is withdrawn. Accepted as conservative; review if real
  devices need longer.
- **L-2 Scope of disarm on loss.** The loss of any source disarms all, because arming is
  engine-wide. Per-source arming would need a protocol change and an owner decision.
- **L-3 Releasing a hung driver.** The timeout drops the write future. Whether the device saw
  FALSE is unknown, which is why the command is marked unconfirmed. The OPC UA driver's own
  behaviour when a future is dropped was not analysed.
- **L-4 Owner decisions open.** Confirm D-02C4A-1 and D-02C4A-2. Decide whether an explicit
  operator "release now" request (protocol 4) is wanted. A PLC-side watchdog for momentary
  commands remains required before physical machinery (decision 14 in the legacy register,
  O-9).
- **L-5 Kotlin not exercised.** The Kotlin runtime was not run against these behaviours (F-9).
- **L-6 Unchanged.** F-4, F-6 and F-7 are unchanged. No EngineDriver write integration exists
  yet.

## 7. Next exact action

1. Verify exact-head Required CI (Windows and Linux engine tests, notice and advisory gate) and
   handoff on this PR.
2. Independently review it, reconciling with PR #19 if that merges first. Shared records may
   need an ordinary merge.
3. Obtain owner confirmation of D-02C4A-1 and D-02C4A-2.
4. Then REUSE-02C.4B: write-capable EngineDriver integration in `tag-engine` on these semantics,
   still in isolated simulation.
