# REUSE-02C.4A — Write lifecycle and termination safety

Status: **draft PR #20, corrected after the owner decisions of 2026-11-25**.

- Branch `feat/reuse-02c4a-write-lifecycle` starts from verified main
  `9dc343b052f456ad507dfc69e2a82fb28345f108`.
- PR #19 merged as `d1571fc` during the work. It was reconciled by an ordinary merge.
- Only `crates/hmi-engine/` and documentation are changed. The `crates/tag-engine/`
  EngineDriver is untouched.
- Only simulated and synthetic drivers are used: a scripted driver, loopback, and the existing
  OPC UA and S7 test servers. There is no physical PLC connectivity and no deployment, and
  nothing is production-accepted.
- This is not REUSE-02C.4B: no write-capable EngineDriver integration.

## 1. Compatibility

- **Protocol 3 is unchanged.** No request or message type, field, refusal code or version
  changed. `contract.rs`, `messages.json` and `write-guard-cases.json` pass unchanged.
- **Kotlin runtime.** Only messages that Kotlin already receives are emitted. Two behaviours are
  new, both through existing messages:
  - `writesArmed {armed:false}` when a source loses its connection (D-02C4A-1). Kotlin already
    receives this message on TTL expiry.
  - `writeResult {accepted:false, refusal: BUSY, message: "COMMAND_UNCONFIRMED: …"}` for a
    blocked momentary command (D-02C4A-2).
  
  The Kotlin runtime was not exercised against these behaviours (L-5).
- **Development and simulation.** Writes in development and isolated simulation remain
  available without production approval. Arming is still the only communication gate.
- **Dependencies.** None added or changed. Both lockfiles are unchanged.

## 2. Owner decisions (2026-11-25), recorded in `docs/ENGINEERING_DECISIONS.md`

**D-02C4A-1: accepted.** Global disarm on connection loss under protocol 3:

- when a source leaves the connected state, its pulses are released and arming is withdrawn;
- monitoring of healthy sources continues;
- automatic re-arming and stale command replay are prohibited.

*Protocol 4 requirement:* per-connection arming.

**D-02C4A-2: accepted with modifications.**

- A release attempt is bounded; an uncertain outcome is reported explicitly; only the affected
  momentary command is blocked.
- Ordinary re-arming does not clear an unconfirmed command, because communication
  authorization and command recovery are separate operations.
- Clearing requires explicit, command-specific recovery with verification of the device state.
- Protocol 3 has no such operation, so the block is retained conservatively. Nothing clears it:
  not `armWrites`, `disarmWrites`, `stop`, `start`, `configure` nor reconnection.

*Protocol 4 requirements:* an explicit outcome-unknown status, and an operator-initiated,
command-specific recovery operation.

PLC-side watchdogs remain required for physical momentary commands.

The first PR #20 candidate (`26910c1`) cleared the block on the next `armWrites`. That rule was
**SUPERSEDED** by the owner decision and is now covered by a regression test (§5).

## 3. Inspection of REUSE-02C.1 findings

| Finding | Disposition |
| --- | --- |
| F-1 arming survives reconnection | **Resolved**: D-02C4A-1 |
| F-2 release after an unknown start can fail | **Mitigated**: bounded, reported, and the affected command blocked for the engine session (D-02C4A-2). Command recovery is a protocol-4 requirement. A PLC-side watchdog is still required before physical use |
| F-3 release and stop bounded only by the driver | **Resolved for the engine**: every release and stop is bounded by the engine, whichever driver it is |
| F-4 a detached S7 thread may finish one transmission | Unchanged (inherent; the stop flag prevents anything further) |
| F-5 no explicit protocol-3 "outcome unknown" code | Unchanged in protocol 3 (`DRIVER_ERROR` + `OUTCOME_UNKNOWN:`); now a recorded protocol-4 requirement |
| F-6 redundant S7 stop check | Unchanged (defence in depth) |
| F-7 flood behaviour | Unchanged |
| F-8 bridge not wired | Out of scope (REUSE-02C.4B) |
| F-9 evidence scope | Hosted Windows and Linux CI comes from the PR. Kotlin not exercised (L-5) |

## 4. Implementation

**Time bounds (`engine::Limits`).** Every wait on a driver is bounded.

| Bound | Default | Effect when exceeded |
| --- | --- | --- |
| `write_timeout` | 10 s | A `write` or `pulse` start is withdrawn and reported as `DRIVER_ERROR` / `OUTCOME_UNKNOWN:`. Dropping the future withdraws an untransmitted S7 command. A timed-out pulse start is released at once. A `browse` answers `BROWSE_FAILED`. |
| `release_timeout` | 5 s, per release | `pulseReleased {released:false}` with `OUTCOME_UNKNOWN:`; the command is blocked |
| `stop_timeout` | 5 s, per driver | The driver is dropped and termination continues |

With *n* running pulses and *m* sources, termination takes at most about
*n* × `release_timeout` + *m* × `stop_timeout`.

`Limits` is engine-side only, because protocol 3 has no field for it. `run_with_limits`
(`#[doc(hidden)]`) exists for tests. `run` and `run_with_drivers` are unchanged and use the
defaults.

**Write outcomes.** The driver's own result is passed on. A session end or an elapsed deadline
becomes `OUTCOME_UNKNOWN:`. Certain non-transmission stays `NOT_TRANSMITTED:`. Nothing is
retried.

**D-02C4A-1.** `on_driver_event`: on any transition from connected to another state, the engine
releases that source's pulses (bounded and reported) and disarms. Other sources keep
connecting, polling and reporting values.

**D-02C4A-2.** `Engine.unconfirmed` holds each momentary command whose release failed, timed out
or had no driver.

- A `pulse` on such a command is refused with `BUSY` and
  `COMMAND_UNCONFIRMED: … re-arming does not clear it … command-specific recovery is not
  available in protocol 3`. Nothing reaches the driver.
- Other commands and ordinary writes are unaffected.
- No request clears the set. It lives for the engine session.

**Shutdown.** `stop_all` releases running pulses (bounded), disarms, then stops each driver
(bounded). A `shutdown` during an active write still ends the wait at once (REUSE-02C.1).

## 5. Regression tests

`crates/hmi-engine/tests/write_lifecycle.rs` has **17 tests**. It uses a scripted simulated
driver with two sources (`sim` with two momentary commands and a setpoint, and the healthy
read-only source `aux`), and 300 ms limits.

| Area | Tests |
| --- | --- |
| Normal write and acknowledgement | `a_normal_write_is_acknowledged_once_and_audited` |
| Momentary command | `a_momentary_command_is_true_then_false_after_its_pulse_time` |
| Timeout and cancellation | `a_write_without_an_answer_times_out_as_outcome_unknown_and_is_withdrawn`, `a_pulse_start_that_times_out_is_released_at_once` |
| Uncertain outcomes | `certain_refusal_and_unknown_outcome_are_distinguished` |
| D-02C4A-1 | `connection_loss_disarms_and_reconnection_never_rearms`, `connection_loss_during_a_pulse_releases_it_and_disarms`, `a_healthy_source_keeps_being_monitored_after_another_source_is_lost` |
| D-02C4A-2 | `an_unconfirmed_release_blocks_the_command_and_is_never_rewritten`, `re_arming_does_not_clear_an_unconfirmed_command`, `stop_start_configure_and_reconnection_do_not_clear_an_unconfirmed_command`, `only_the_affected_command_is_blocked`, `a_release_without_an_answer_is_bounded_and_reported` |
| Shutdown and termination | `shutdown_with_a_running_pulse_releases_it_before_stopping`, `shutdown_is_bounded_when_the_release_and_the_driver_hang`, `a_client_disconnect_terminates_within_bounds_with_a_hung_driver`, `shutdown_during_a_pending_write_is_immediate_and_nothing_follows` |

The REUSE-02C.1 suites (`session_safety.rs`, the S7 `supervision_tests`) and every earlier suite
pass unchanged.

### Defect re-introduction

Each defect was re-introduced one at a time against the corrected head, the targeted tests were
run, and the source was restored and compared byte for byte. **All 11 were caught.**

| Re-introduced defect | Failing test(s) |
| --- | --- |
| re-arming clears an unconfirmed command (the superseded first rule) | `re_arming_does_not_clear_an_unconfirmed_command` |
| `configure` clears an unconfirmed command | `stop_start_configure_and_reconnection_do_not_clear_…` |
| every command blocked instead of the affected one | `only_the_affected_command_is_blocked` |
| connection loss stops all sources (no healthy monitoring) | `a_healthy_source_keeps_being_monitored_…` |
| unconfirmed release not tracked | 3 D-02C4A-2 tests |
| arming survives connection loss | `connection_loss_disarms_and_reconnection_never_rearms` |
| no write deadline | both timeout tests |
| timed-out pulse start not released at once | `a_pulse_start_that_times_out_is_released_at_once` |
| loss does not release that source's pulses | `connection_loss_during_a_pulse_releases_it_and_disarms` |
| unbounded release write | `a_release_without_an_answer_is_bounded_and_reported` |
| unbounded driver stop | `shutdown_is_bounded_when_the_release_and_the_driver_hang` |

## 6. Local validation (corrected head)

Linux x86_64, rustc and cargo 1.97.0, `--locked`, on the tree merged with PR #19.

| Check | Result |
| --- | --- |
| hmi-engine `cargo fmt --check` | clean |
| hmi-engine strict Clippy (`-D warnings`), default and `test-server` | clean |
| hmi-engine rustdoc (`RUSTDOCFLAGS="-D warnings"`, `test-server`) | clean |
| hmi-engine `cargo test --locked --features test-server` | **71 passed, 0 failed** |
| hmi-engine `cargo test --locked` | **53 passed, 0 failed** |
| `write_lifecycle` + `session_safety` repeated 10× | 10/10 green (17 + 7 each run) |
| `cargo audit` 0.23.0, database fetched at run time, 221 crates | 0 vulnerabilities; 1 warning (RUSTSEC-2026-0543, `chrono` unmaintained, already documented) |
| tag-engine fmt and strict Clippy | clean |
| tag-engine `cargo test --locked` | 86 passed, 0 failed, 2 ignored by design |
| tag-engine real-child tests (`--ignored`) against an `hmi-engine` freshly built from this branch | 2 passed |
| Both lockfiles; `crates/tag-engine` vs main | unchanged; no diff |

hmi-engine tests by target (`test-server`):

| Target | Tests |
| --- | --- |
| lib | 16 |
| contract | 4 |
| opcua | 9 |
| process | 5 |
| pulse | 4 |
| s7 | 9 |
| session_safety | 7 |
| write_lifecycle | 17 |

The first candidate `26910c1` passed exact-head Required CI 45577232401, tag workflow 45577232651
and handoff 45577232344. That evidence does not carry over to the corrected head, which needs
its own exact-head hosted CI.

## 7. Limitations and open items

- **L-1 Engine bound shorter than some driver bounds.** `write_timeout` (10 s) is shorter than
  the largest S7 `timeoutMs` (30 s). A slower write is reported as outcome unknown, and is
  withdrawn if still queued.
- **L-2 Disarm scope.** The disarm is global by design under protocol 3 (D-02C4A-1).
  Per-connection arming is a protocol-4 requirement.
- **L-3 Hung release.** When a release times out, its future is dropped and whether FALSE
  reached the device is unknown, hence the block. The OPC UA driver's behaviour when a future
  is dropped was not analysed.
- **L-4 The block is per engine session.** It is in memory and ends with the engine process; a
  restarted engine does not know about it. Before writes are exposed through the EngineDriver
  (REUSE-02C.4B), the host must carry such blocks across engine restarts, or protocol 4 must
  provide recovery. Until then, no write-capable integration should rely on an engine restart.
- **L-5 Kotlin not exercised.** The Kotlin runtime was not run against the disarm and
  `COMMAND_UNCONFIRMED` behaviour.
- **L-6 Unchanged and still required.** F-4, F-6 and F-7 are unchanged. PLC-side watchdogs are
  still required for physical momentary commands.

**Protocol 4 requirements recorded:**
- explicit outcome-unknown status;
- operator-initiated, command-specific recovery with verification of the device state;
- per-connection arming.

## 8. Next exact action

1. Exact-head hosted Required CI, tag workflow and handoff on the corrected PR #20 head.
2. Independent review of the corrected head, then merge only after both gates pass.
3. REUSE-02C.4B only on separate authorization. It must honour D-02C4A-1/2 and L-4.

## 9. Independent review — 2026-11-25 (integration blocked)

Reviewed predecessor 7b0166cccd0657104a440fec0fb7c628170c364e against
main a4babf7d7f8cfdbca4adfea3584ee0b7188bd6e6. GitHub reported all 17
predecessor checks successful (Desktop CI 45594448967, Tag engine CI
45594448963, handoff 45594448982). This does not verify the reconciled head.

- **Blocking — uncertain OPC UA pulse start:** the OPC UA driver's five-second
  write timeout returns an unprefixed DriverError; other transport errors are
  also unclassified. Engine pulse cleanup depends on is_outcome_unknown(),
  so a potentially transmitted TRUE can skip the compensating FALSE. Classify
  pre-transmission, definite rejection and ambiguous post-transmission outcomes
  conservatively and test the driver-to-engine behavior.
- **Blocking — queued work vs. connection loss:** the engine's unbiased select
  may choose a deferred/new request before an already queued source-state event.
  The supervision loop does not consume driver events. A write can consequently
  be dispatched using stale arming after a loss event is queued. Prioritize
  loss processing at dispatch boundaries, address stale queued control requests
  and test a loss during an active operation with queued work.
- **Bounded lifecycle gap:** start_source awaits start and failure cleanup stop
  directly; demand updates also await driver methods directly. Existing drivers
  generally return promptly or have internal timeouts, but the stated
  engine-enforced bound on every driver wait is not implemented for these
  paths. Define and test the termination bound and correct the implementation
  or explicitly scope the guarantee.

No physical PLC testing or fresh engine test execution claimed by this review.
Do not integrate until these findings are resolved, scope coordinated for any
additional driver paths, and fresh exact-head checks and review are complete.
