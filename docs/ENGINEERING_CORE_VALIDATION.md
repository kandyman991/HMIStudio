# Engineering core validation (RUST-03B)

Acceptance for RUST-03 is automated engineering tests (D-008). No manual desktop
installation or visual acceptance applies to this milestone.

## Local evidence

Environment: Linux x86_64, rustc/cargo 1.97.0 (stable). Branch
`feat/rust-03b-engineering-core`, stacked on PR #2 head `ca60316`. Commands run from
`crates/engineering-core/`:

```text
cargo fmt --check                                   → clean
cargo clippy --all-targets --locked -- -D warnings  → clean
cargo test --locked                                 → 55 passed, 0 failed
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps      → clean
```

The RUST-03A crate at the base head was re-run unchanged:
`cargo test --locked` in `crates/project-model/` → 74 passed, 0 failed.

The engine crate adds no new third-party dependencies beyond RUST-03A's set
(serde, serde_json, thiserror; licenses MIT / Apache-2.0 / Unlicense /
Unicode-3.0 per `cargo metadata`). `Cargo.lock` is committed.

**Not run locally:** Windows (the read-only, rename and CRLF tests branch on
platform; Windows evidence comes only from CI), macOS, MSRV, `cargo audit` /
`cargo deny` (not available in this environment). CI results for the exact head are
reported in the pull request, not here, because a commit cannot contain its own
CI result.

## Required regression categories

| Category | Tests (`crates/engineering-core/tests/`) |
| --- | --- |
| Atomic transaction rollback | `transactions.rs`: `multi_command_transaction_is_atomic_on_command_failure`, `transaction_rejected_by_validation_rolls_back_everything`, `empty_and_no_op_transactions` |
| Invalid references | `transactions.rs`: `invalid_references_are_rejected` (binding to unknown tag, unknown style, deleting referenced popup / definition, unknown start screen); `commands.rs`: `tag_lifecycle`, `component_definitions_lifecycle_and_duplication` |
| Circular component hierarchies | `transactions.rs`: `circular_hierarchies_are_rejected` (move into descendant, into itself, self-instancing definition, indirect definition cycle, moving roots) |
| Duplication and reference remapping | `commands.rs`: `duplicate_node_gets_new_ids_and_keeps_original`, `duplicate_in_place_lands_after_original`, `duplicate_screen_remaps_self_references`, `component_definitions_lifecycle_and_duplication`, `insert_with_assign_new_ids_never_collides` |
| Stale revision rejection | `transactions.rs`: `stale_revisions_are_rejected`; `large_and_protocol.rs`: `protocol_dispatch_flow` |
| Undo/redo consistency | `undo_redo.rs`: `undo_and_redo_restore_exact_states`, `undo_reports_inverted_changes`, `divergent_edit_invalidates_redo`, `history_limit_drops_oldest_steps`, `multi_command_transactions_and_merge_keys_group_undo_steps`, `undo_after_merge_does_not_reopen_the_group`, `dirty_state_follows_undo_redo_and_save`, `identifiers_are_never_reused_after_undo`, `undo_restores_references_exactly` |
| Save/load equivalence | `persistence.rs`: `save_then_open_is_equivalent_and_canonical` (incl. byte-for-byte round trip of the RUST-03A industrial fixture), `save_requires_path_and_save_as_respects_existing_files`, `backups_rotate` |
| Failed writes and interrupted saves | `persistence.rs`: `failed_write_leaves_original_intact`, `interrupted_write_leaves_original_intact_and_next_save_succeeds`, `silently_truncated_write_is_caught_by_read_back`, `failed_replace_leaves_original_intact`, `failed_backup_aborts_the_save`, `stale_temp_file_from_a_crash_does_not_block_saving` |
| External file modification | `persistence.rs`: `external_modification_is_detected_and_never_overwritten_silently`, `reload_takes_the_disk_version` |
| Autosave and recovery | `persistence.rs`: `autosave_and_crash_recovery`, `inconsistent_or_stale_autosave_is_reported_not_trusted`, `interrupted_autosave_never_produces_a_bad_recovery`, `untitled_projects_autosave_to_the_recovery_dir`, `clean_close_removes_autosave_forced_close_keeps_it` |
| Invalid projects / migration | `persistence.rs`: `invalid_project_opens_with_diagnostics_and_is_not_repaired`, `unloadable_documents_fail_to_open_without_touching_the_file`, `opening_older_schema_migrates_in_memory_and_writes_only_on_save`; `transactions.rs`: `reject_new_errors_allows_incremental_repair_of_invalid_projects` |
| Deterministic execution | `transactions.rs`: `execution_is_deterministic` (same script, three sessions, identical canonical JSON and serialized outcomes), `commands_round_trip_as_json` |
| Large nested projects | `large_and_protocol.rs`: `large_nested_projects_edit_undo_and_persist` (40 screens × depth 32, > 5,000 nodes; 30 multi-command transactions, full undo/redo, depth-limit refusal, save/open equivalence) |
| Windows/Linux filesystem behavior | `platform.rs`: `paths_with_spaces_and_unicode`, `replace_over_existing_file_is_atomic_rename`, `crlf_and_non_canonical_files_open_and_are_detected_as_unchanged`, `read_only_target_behaves_per_platform_without_data_loss` (`cfg(windows)` vs POSIX expectations), `sidecar_and_temp_names_stay_in_the_project_directory` |
| Frontend contract | `large_and_protocol.rs`: `protocol_requests_parse_from_json` (unknown / shell / file requests rejected), `protocol_dispatch_flow` |

Fault injection uses `fs::FaultyFileSystem`: write fails before writing, write
interrupted after half the bytes, write reports success but truncates, rename fails,
each targeted at one path pattern and firing once.

## Known gaps

- Real power-loss / kernel-crash durability cannot be proven by unit tests; the
  tests prove ordering (temp → verify → backup → rename) and that every injected
  failure leaves the original valid.
- Windows-specific expectations run only in CI; locked files held open by another
  process (sharing violations) are not tested.
- Recovering a project whose file was deleted after autosave is implemented
  (`baseUnchanged: false`) but not covered by a dedicated test.
- Performance is checked with a generous bound only; no benchmarks.
- No fuzzing of command sequences yet (recommended: property-based random
  transaction scripts checking undo-to-start equivalence).
