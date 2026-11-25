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

## Reconciliation and independent engineering review — 2026-11-24

PR #7 original head `de9e207a43b9c8ab322b1d3db33fd0832febe596` reconciled with verified main `78f3a64c06d00afcc392edccad9da9fd757d0392` (PR #16 merged); clean ordinary merge without conflicts or force push. Prior head Desktop CI 45414031733, handoff 45414031764 and Tag engine CI 45414031908 passed but are not acceptance of the reconciled head.

Review of typed values, access/range checks, binding transformations, simulator/driver contract, subscriptions and shutdown, with fixes and regression tests:
- CONFIRMED_FAILURE: adjacent 64-bit integers above 2^53 were rounded together by range comparison and map matching. Compare integral range values with ceil/floor bounds in i128, match typed map keys exactly; reject unsafe wide-integer linear transforms/inverses rather than silently rounding an operator command. Regressions cover 2^53 and 2^53+1. The range error display still uses f64 for diagnostics.
- CONFIRMED_FAILURE: property numeric min/max were ignored for operator input and map output checks. Enforce inclusive bounds using exact integer comparisons where applicable; bounded input regression.
- CONFIRMED_FAILURE: a writer paused before acquiring the store lock could update after shutdown. Recheck running state under the lock; check subscription admission under subscriber lock, device read/write admission under driver lock, and stop issuing new batches after cancellation. Pending unattempted items report errors rather than stale successes. Deterministic pausing-clock shutdown regression. Already acknowledged device writes are not misreported as unexecuted.
- CONFIRMED_FAILURE: unchecked expanded default array/struct allocation could exhaust memory, including nested arrays. Review-selected conservative per-default limit of 100,000 expanded nodes and existing depth limit; fail explicitly before allocation. Nested/huge array regression. This is not an aggregate project memory budget or full untrusted project parser hardening.
- CONFIRMED_FAILURE: extreme duration could overflow Instant addition even if already cancelled. Use elapsed/saturating remaining time and bounded platform wait intervals; huge-duration cancellation regression. Simulator checks cancellation even with zero latency.
- CI-02 reconciliation: preserve dedicated Windows/Linux checks and strict audit rather than remove possibly required contexts; exact-head checkout, pinned cache, separate manual concurrency and lockfile drift guard. General Desktop CI discovers the tag-engine crate; dedicated workflow adds redundant tests for now. Workflow change triggers existing full fallback including unrelated desktop installers; no new packaged engine or UI.

Local final: cargo fmt, locked offline tests **50/50**, Clippy (-D warnings), rustdoc (-D warnings) all passed. 15 Python CI routing/engine notice regressions passed. Tag-engine Cargo.lock unchanged. Cargo-audit 0.23.0 with `--deny warnings` using pinned advisory DB `1d328b6bb9f2f350bf65c1c81ab68781cb9e018a`: zero vulnerabilities and warnings. Independent archive check: all 20 crates.io archives match locked SHA-256 and declared license inventory (15 MIT OR Apache-2.0, 3 MIT, 1 Unlicense OR MIT, 1 MIT/Apache-2.0 plus Unicode-3.0); license files present, choose MIT where offered and retain Unicode terms. No new dependency, vendoring or release notice clearance. Fresh hosted audit remains mandatory.

Simulation foundation only. Unbounded subscribers remain a known integration blocker: bounded/coalesced delivery with explicit resync, process supervision and framing, lossless or safely refused wide integer bridge transport, no stale write replay, and validated failure/quality propagation before host/UI or physical driver integration. Aggregate project allocation limits and broader fuzzing remain follow-ups. No physical PLCs or manual desktop installation required for this PR.
