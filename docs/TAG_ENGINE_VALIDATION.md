# Tag engine validation (RUST-06A)

Acceptance for this milestone is automated and simulation-only (D-008: RUST-06 uses
simulation first; hardware acceptance comes at explicitly defined hardware milestones).
No manual desktop installation applies. No physical PLC, network or production
capability is claimed.

## Local evidence

Environment: Linux x86_64, rustc/cargo 1.97.0. Branch `feat/rust-06a-tag-engine`
on `main` `d17f299` (developed from `101c8f3`; the RUST-02A merge in between touched
no crate, and tests were re-run on the new base). Commands run from `crates/tag-engine/`:

```text
cargo fmt --check                                   → clean
cargo clippy --all-targets --locked -- -D warnings  → clean
cargo test --locked                                 → 44 passed, 0 failed
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked → clean
cargo audit --deny warnings (cargo-audit 0.23.0, RustSec DB 1566 advisories)
                                                    → 22 crates scanned, 0 findings
```

Dependency review (from `cargo metadata --locked`): 20 external crates, all from
crates.io, with declared licenses: 15 MIT OR Apache-2.0, 3 MIT, 1 Unlicense OR MIT and
1 (MIT OR Apache-2.0) AND Unicode-3.0. No new dependency beyond the set already used by
`hmi-project-model` / `hmi-engineering-core`. `Cargo.lock` is committed. The same
`cargo audit --deny warnings` run on the existing `project-model` and
`engineering-core` lockfiles also found nothing (informational; those crates are
unchanged).

Windows evidence comes only from CI (`.github/workflows/tag-engine.yml`, ubuntu-24.04
and windows-latest, plus an audit job). Exact-head results are recorded in the pull
request, because a commit cannot contain its own CI result.

## Required categories

| Category | Tests (`crates/tag-engine/tests/`) |
| --- | --- |
| Valid and invalid tag operations | `tags.rs`: `internal_read_write_and_types`, `initialization_is_deterministic_and_reports_config_issues`, `invalid_initial_values_and_config_errors_do_not_abort_startup`, `timestamps_follow_the_clock`; `simulation.rs`: `plc_changes_appear_after_poll_and_writes_reach_the_plc`, `grouped_writes_preserve_order_and_report_per_item`, `unavailable_driver_is_reported_and_inert` |
| Data-type compatibility | `tags.rs`: `type_context_rules` (integer bounds, u64 max, float32 overflow, no cross-kind coercion, NaN), `internal_read_write_and_types`; `simulation.rs`: `injected_and_item_errors` (PLC-side type mismatch) |
| Structured member access | `tags.rs`: `structured_member_access`; `bindings.rs`: `read_bindings_with_every_transform` (struct/array members); `concurrency.rs`: `concurrent_member_writes_are_atomic_read_modify_write` |
| Access permissions | `tags.rs`: `access_permissions_are_enforced`, `engineering_range_rejects_operator_writes`, `device_values_outside_range_are_uncertain_not_rejected`; `bindings.rs`: compile-time access checks |
| Concurrent reads and writes | `concurrency.rs`: `concurrent_reads_and_writes_never_observe_torn_values` (8 threads, no torn structs, notifications strictly ordered), `concurrent_member_writes_are_atomic_read_modify_write`, `threaded_scanning_tracks_the_plc_and_writes_interleave_with_polls`, `engine_is_send_and_sync` |
| Subscription lifecycle | `subscriptions.rs`: all six tests (initial state, filtering, no-change suppression, quality-only events, connection events, global order, unsubscribe by drop, closed at shutdown, IPC serialization) |
| Simulated PLC connection lifecycle | `simulation.rs`: `connection_lifecycle_and_initial_seed`, `reconnect_respects_the_reconnect_delay`, `strict_addresses_reject_unknown_memory_at_driver_level` |
| Communication failure and recovery | `simulation.rs`: `connection_loss_and_recovery`, `latency_within_timeout_succeeds_beyond_times_out`, `injected_and_item_errors`; `concurrency.rs`: threaded loss and recovery |
| Binding evaluation and inverse transforms | `bindings.rs`: `read_bindings_with_every_transform`, `write_and_bidirectional_bindings_with_inverse_transforms`, `incompatible_and_non_invertible_bindings_are_rejected_at_compile_time`, `map_without_match_or_default_is_an_evaluation_error`, `bad_quality_propagates_through_bindings`, `industrial_fixture_bindings_compile_and_evaluate_on_the_simulator` (every tag binding of the RUST-03A bottling-line fixture, including faceplate tag parameters) |
| Clean shutdown | `subscriptions.rs`: `shutdown_closes_subscriptions_cleanly`; `concurrency.rs`: threaded shutdown with a 400 ms in-flight latency (bounded, joined), `drop_shuts_down_and_ends_subscriptions` |
| Scan groups / determinism | `simulation.rs`: `scan_groups_poll_at_their_intervals`, `large_scan_groups_are_split_by_driver_request_size`, `deterministic_scenarios_are_repeatable`, `generator_addresses_are_deterministic_and_read_only` |
| Regression tests | See below. |

## Defects found during development

Classified per `docs/ARCHITECTURE_MEMORY.md`. All were found before the first push and
have regression tests.

1. **CONFIRMED_FAILURE — lock-order deadlock.** The scan thread's wake-time
   computation took the connection status lock and then the driver lock, the reverse
   of the documented order. A writer holding the driver lock while publishing a status
   change could deadlock with it. Fixed (driver lock first).
   Regression: `regression_no_deadlock_between_scanner_and_writers_during_reconnects`.
   **Evidence:** with the faulty order temporarily restored, the test hung and hit its
   30 s watchdog in 3 of 3 runs; with the fix it passes in about 0.3 s.
2. **CONFIRMED_FAILURE — generator precision.** `sin(2π·t/period)` with epoch-scale `t`
   (≈1.7×10⁹ s) produced an error of about 2×10⁻⁷. The phase is now computed in integer
   microseconds modulo the period.
   Regression: `regression_generator_phase_is_exact_at_epoch_scale` (error < 1e-12).
3. **CONFIRMED_FAILURE — integer sources rendered as floats.** Integer tags bound to
   number properties produced `3.0`.
   Regression: `regression_integer_sources_stay_integers` (including `u32::MAX`).
4. **CONFIRMED_FAILURE — out-of-range simulator seed.** Tags with a range and no
   `initialValue` were seeded with 0 even when 0 is outside the range. They are now
   seeded at `range.min`.
   Regression: `regression_simulator_seeds_inside_the_engineering_range`.

No previously rejected approach (F-001, F-002) is revived.

## Known gaps

- Physical drivers and hardware acceptance: not in scope.
- Subscriber back-pressure: channels are unbounded; there is no test with a stalled
  consumer under load.
- `ramp` and `square` generators are implemented but only `sine` is covered by tests.
- Timing-based tests (threaded mode) use generous bounds; they detect hangs, not
  performance regressions. No benchmarks.
- Threaded mode is not tested with many connections (only one simulator connection
  is active).
- Kotlin runtime parity of transform/rounding semantics is not verified (RUST-05).
- No fuzzing of value conversion or member paths yet.

## Handoff entry (for the integrator)

`CHAT_HANDOFF.md`, `ROADMAP.md`, `docs/MILESTONE_PLAN.md`, `handoff/state.json` and
`docs/ENGINEERING_DECISIONS.md` are being updated concurrently by the RUST-02 workstream,
so this PR does not edit them, to avoid conflicting edits. Proposed entry to add on
merge:

> RUST-06A tag engine — PR (this) on main d17f299: new crate `crates/tag-engine`
> (typed values, quality, subscriptions, driver abstraction, scan groups, simulated PLC,
> binding evaluation), 44 local tests, fmt/Clippy/rustdoc clean, cargo audit clean.
> Decisions 06A-1…8 in docs/TAG_ENGINE.md; four CONFIRMED_FAILURE defects fixed with
> regressions. Simulation only; physical drivers and hardware acceptance deferred.
> Next: integrate a host-owned TagEngine with the ProjectSession for desktop simulation
> (bounded event delivery), then alarms/recipes on top of subscriptions.
