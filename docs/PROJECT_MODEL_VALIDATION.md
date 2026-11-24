# Project model validation (RUST-03A)

`hmi_project_model::validate(&Project) -> ValidationReport` checks a deserialized
project. Loading is a two-stage process (see `serialization.rs`):

1. **Structural** (deserialization) — wrong JSON shape, unknown fields, unknown enum
   tags and unsupported versions fail the load with a `LoadError` carrying a JSON
   Pointer (or line/column for syntax errors).
2. **Semantic** (validation) — everything that deserializes is checked and *all*
   findings are reported together.

## Guarantees

- **Deterministic.** Checks run in a fixed order over arrays in document order and
  ordered maps; no hash-map iteration reaches the output. The same project always
  yields an identical report (tested by repeated runs).
- **Non-destructive.** Validation never mutates, normalizes, repairs or drops data.
  Unknown fields are rejected at load time rather than discarded.
- **Located.** Every diagnostic has an RFC 6901 JSON Pointer into the canonical document.
- **Machine-readable.** `Diagnostic { severity, code, path, message }` serializes to JSON
  (`code` in kebab-case) for the editor/IPC and the `validate` example CLI. Codes are a
  stable contract; messages are English and may change.
- **Bounded.** Layout trees deeper than 32 levels are reported (`nesting-too-deep`) and
  not descended further; JSON deeper than `serde_json`'s limit (128) fails as a syntax
  error instead of overflowing the stack.

`load_project` and `save_project` refuse projects with any **error**; **warnings**
are returned to the caller.

## Diagnostic codes

| Code | Severity | Detects |
| --- | --- | --- |
| `unsupported-format` | error | `format` is not `hmi-studio.project` (model built in code). |
| `unsupported-schema-version` | error | `schemaVersion` ≠ the version this validator supports (model built in code; documents are already rejected at load). |
| `invalid-identifier` | error | Identifier syntax (empty, too long, bad first char, bad character). |
| `duplicate-identifier` | error | Same id used twice anywhere in the project (reported on the later occurrence, naming the first). |
| `unknown-reference` | error | Missing screen, tag, connection, unit, data type, style, theme, resource, definition, breakpoint, device profile, alarm, alarm group, recipe/set, locale, localization key or color token. |
| `invalid-reference` | error | Reference to the wrong kind: navigating to a popup, start screen is a popup, resource of the wrong kind (font as image, …). |
| `invalid-hierarchy` | error | Children on leaf components or instances, > 1 child in a container, ≠ 1 child in a scroll. |
| `circular-component-reference` | error | Component definitions instantiating themselves directly or indirectly (path rotated to the first definition in document order, e.g. `cmp-a -> cmp-b -> cmp-a`). |
| `circular-data-type` | error | Struct types containing themselves (directly, via other structs or via arrays). |
| `nesting-too-deep` | error | Layout depth > `MAX_NODE_DEPTH` (32). |
| `invalid-layout-constraint` | error | Negative/non-finite sizes, fill fraction ∉ (0, 1], min > max, fixed size outside min/max, aspect ratio ≤ 0, negative padding/spacing/gaps, empty or non-positive grid tracks, `weight`/`crossAlign` outside row/column, `align` outside box, `position` outside absolute (or missing inside), `gridCell` outside grid (or missing inside, or out of bounds), fill along a scroll axis. |
| `invalid-responsive-rule` | error | Empty condition or override, min > max, `direction`/`spacing` on non-flow nodes, `columns` on non-grids, `weight`/`position` overrides in the wrong parent. |
| `unknown-property` | error | Property not in the catalog for the node's kind (instances: common properties only; instance arguments go in `element.parameters`); binding to an unknown property. |
| `invalid-property-type` | error | Wrong JSON type for the declared property type. |
| `invalid-property-value` | error | Right type, but out of range, not an allowed enum option, or a malformed color. |
| `invalid-parameter` | error | Definition parameters: bad/duplicate names, shadowing common properties, `tag` parameters with defaults, invalid enum/range declarations. Instances: unknown parameter, required parameter missing, tag argument with wrong data type or not writable when required. |
| `invalid-binding` | error | Static (non-bindable) property; write/readWrite on a non-input property; writing a non-writable tag; reading a write-only tag; type mismatch without transform; invalid transforms (non-invertible linear in write mode, bit index out of range, bit on non-integer, map/bit in write mode, map `when` values not matching the source type); `tagParameter`/`parameter` outside a definition, missing, or of the wrong kind; parameter bindings with modes, members or transforms. |
| `duplicate-binding` | error | Same property bound twice on one node. |
| `invalid-event` | error | Event not raised by the element kind (catalog), duplicate handlers for one event, handlers without actions, non-`show`/`hide` screen events. |
| `invalid-action` | error | Navigating to a popup / opening a page as popup; writing non-writable tags; value type mismatch; `toggleTag`/`pulseTag` on non-bool, `stepTag` on non-numeric; zero step/duration; min > max; both `alarm` and `group` set; `tagParameter` targets outside definitions. |
| `invalid-tag` | error | Empty name; connection without address or address without connection; unit/range on non-numeric tags; min > max; initial value not matching the data type (incl. integer ranges, array lengths, struct members). |
| `invalid-data-type` | error | Empty structs, bad/duplicate member names, zero-length arrays, member paths that do not resolve (unknown member, index on non-array, index out of bounds, malformed path). |
| `invalid-alarm` | error | Condition incompatible with the value type, non-finite limit, bit index out of range, deadband on non-high/low or negative, priority ∉ 1..=1000, empty group, unreadable tag. |
| `invalid-recipe` | error / warning | Error: no fields, non-writable field tags, set values for unknown fields or of the wrong type. Warning: a set lacks a value for a field. |
| `invalid-localization` | error | Malformed or duplicate locale tags, default locale not declared, malformed keys, translations for undeclared locales, missing default-locale text. |
| `missing-translation` | warning | A key has no text for a declared non-default locale. |
| `invalid-resource` | error | Absolute paths, `..`, `\`, `:` (drive letters / URL schemes), empty segments; malformed SHA-256. |
| `invalid-style` | error / warning | Error: malformed theme colors, typography size/weight/line height, non-finite metrics, style target ≠ node kind. Warning: a color token is missing from some (not all) themes. |
| `invalid-value` | error / warning | Error: empty project/definition names, zero resolutions, non-positive density, bad breakpoints, empty endpoint, no screens. Warning: orientation contradicts resolution; non-simulator connection without endpoint. |
| `possible-secret` | warning | Connection parameter names containing password/secret/token/apikey/privatekey; use `credentialRef`. |

Structural `LoadError`s: `Syntax` (line/column), `NotAProject` (root/format),
`MissingVersion`, `UnsupportedVersion` (newer, or older without migration path),
`Migration`, `Structure` (JSON Pointer + message, includes unknown fields), `Invalid`
(validation report).

## Rules in more detail

- **Identifiers** share one project-wide namespace: project, navigation items,
  device profiles, breakpoints, themes, styles, resources, units, data types,
  connections, tags, alarms, recipes, recipe fields, recipe sets, component
  definitions, screens and every node (including nodes inside definitions).
- **Component instances** are not expanded; definition trees are validated once
  in the definition's own scope. `tagParameter`/`parameter` sources are resolved
  against the enclosing definition. Cycles make expansion impossible and are errors.
- **Tag type compatibility (no transform)**: bool → bool; any numeric → number;
  integer → integer; string → string/text/enum. Colors, resources and other
  combinations require a `map` transform whose values are validated against the
  target property type.
- **Write paths** (bindings with `write`/`readWrite`, write actions, recipe fields)
  require `access: write|readWrite`, or a `tag` parameter declared `writable: true`.

## Test evidence

Environment used for this PR: Linux x86_64, `rustc`/`cargo` 1.97.0 (stable).
Commands, all run from `crates/project-model/`:

```text
cargo fmt --check                                   # clean
cargo clippy --all-targets --locked -- -D warnings  # clean
cargo test --locked                                 # 71 passed, 0 failed
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps      # clean
cargo run --example validate -- fixtures/valid/*.hmi.json   # both "ok"
```

Not run: Windows/macOS hosts, MSRV verification (no `rust-version` declared),
`cargo audit`/`cargo deny` (tools unavailable in the environment; all dependencies
are MIT, Apache-2.0, Unlicense or Unicode-3.0 licensed per `cargo metadata`).
The new `project-model` CI workflow runs fmt/clippy/test on ubuntu and windows.

| Required test category | Tests (`crates/project-model/tests/`) |
| --- | --- |
| JSON round-trip | `roundtrip.rs`: `fixtures_round_trip_byte_for_byte`, `model_round_trip_through_json_is_identity`, `programmatic_project_round_trips`, `extensions_and_free_form_values_are_preserved_exactly`, `floats_round_trip_exactly`, `unknown_fields_are_rejected_not_dropped`, `unknown_fields_on_empty_variants_are_rejected`, `structure_errors_report_json_pointer`, `syntax_errors_report_position` |
| Stable identifiers | `identifiers.rs`: `identifier_syntax`, `allocator_is_deterministic_and_avoids_existing`, `ids_are_unaffected_by_renames_and_edits`, `invalid_identifiers_are_reported_with_path`, `duplicates_are_detected_across_kinds`, `duplicate_report_points_at_later_occurrence` |
| Deeply nested layouts | `layout.rs`: `nesting_up_to_the_limit_is_valid_and_round_trips`, `nesting_beyond_the_limit_is_reported_once`, `pathological_depth_is_a_clean_error_not_a_crash`, `every_container_kind_nests_inside_every_other` (36 combinations) |
| Responsive layout representation | `layout.rs`: `responsive_rules_are_represented_and_round_trip`, `invalid_responsive_rules`, `layout_constraints_are_checked_in_context`, `z_order_visibility_and_clipping_are_preserved` |
| Invalid references | `references_and_hierarchy.rs`: `unknown_references_are_reported`, `references_to_the_wrong_kind_of_object`, `styles_must_match_the_node_kind` |
| Circular hierarchies | `references_and_hierarchy.rs`: `self_recursive_component_is_detected`, `indirect_component_cycles_are_reported_once_each`, `circular_data_types_are_detected`, `invalid_component_hierarchies`, `container_accepts_one_child_and_scroll_exactly_one` |
| Property types | `references_and_hierarchy.rs`: `properties_are_typed_per_component_kind`, `instance_parameters_are_checked`, `tag_parameter_type_and_access_are_enforced_per_instance` |
| Binding validation | `bindings.rs`: `valid_bindings_pass`, `write_bindings_need_writable_tags_and_input_properties`, `type_mismatches_need_transforms`, `transforms_are_checked`, `member_paths_are_resolved`, `duplicate_and_unknown_binding_targets`, `parameter_sources_only_inside_definitions`, `parameter_sources_inside_definitions_are_typed`, `instance_value_parameters_can_be_bound_to_tags`, `events_and_actions_are_checked`, `tags_alarms_and_recipes_are_consistent`, `localization_rules`, `secrets_in_connections_are_flagged_and_paths_are_sandboxed` |
| Schema versions | `versions.rs`: `detects_current_version`, `rejects_documents_that_are_not_projects`, `newer_versions_are_rejected_explicitly`, `older_versions_without_migrations_are_rejected`, `migrations_upgrade_older_documents`, `migration_registry_guards`, `validator_reports_unsupported_schema_version_and_format`, `load_project_fails_with_full_report` |
| Deterministic serialization | `determinism.rs`: `serialization_is_byte_identical_across_runs`, `map_insertion_order_does_not_affect_output`, `json_key_order_in_input_does_not_affect_output`, `canonical_output_omits_defaults_and_ends_with_newline`, `validation_is_deterministic`, `non_finite_numbers_are_refused_not_written_as_null`, `save_refuses_invalid_projects` |
| Example fixtures | `fixtures.rs`: `valid_fixtures_load_and_validate_cleanly`, `invalid_fixtures_fail_with_the_expected_code`, `industrial_example_covers_required_features` |
| JSON Schema | `schema.rs`: `committed_schema_is_up_to_date`, `schema_identity_and_root_requirements`, `every_ref_resolves`, `fixtures_conform_to_the_committed_schema`, `schema_rejects_structural_mistakes` |

`fixtures/valid/bottling-line.hmi.json` is the realistic industrial example: five
screens (four pages and a settings popup), three reusable definitions (motor
faceplate with a writable struct tag parameter, tank, KPI tile), OPC UA and simulator
connections, two UDTs, twelve tags (structs, arrays, internal tags), five alarms,
one recipe with two sets, two themes, two locales, four device profiles, three
breakpoints, nested row/column/box/grid/scroll/absolute layouts and responsive
rules. The OPC UA endpoint uses a documentation-only address (RFC 5737); it is not
a real plant.

`fixtures/invalid/` holds one minimal document per major failure, named after the
expected diagnostic code (or `load-error.<kind>`).

## Known validation gaps

- No validation of RFC 3339 timestamps, `dateTime` values, media types, OPC UA /
  S7 address syntax or endpoint URLs (driver-specific; belongs to the drivers).
- Theme `textStyle` names in properties are not checked against typography tokens.
- Responsive overrides are checked individually, not for the combined effect of
  several matching rules; grid `gridCell` overrides are checked against base tracks.
- Binding/action checks treat `tag` parameters without a declared `dataType` as
  untyped (type checks are skipped, access checks still apply).
- Cycle reporting guarantees at least one cycle per strongly connected group.

## Independent review corrections (2026-11-24)

Reviewed initial PR #2 head 491d4aa8909064dee47f3ec82201923229894c3f.
Three findings corrected in the same PR:

- JSON decoding through serde_json::Value previously discarded duplicate object
  keys (last key wins), including conflicting schema versions and nested extension
  data. Text entry points now reject duplicate keys recursively before migrations
  or typed loading. The predecoded Value API cannot detect duplicates already lost
  by callers. Regression: duplicate_json_keys_are_rejected_before_version_and_model_loading.
- Float32 tag, action and recipe values previously accepted numbers beyond the
  finite f32 range. Shared value validation now rejects overflow. Normal floating
  point rounding/underflow policy is not changed. Regression:
  float32_overflow_is_rejected_in_tag_and_recipe_values.
- Recursive reference-graph DFS could exhaust the call stack on long chains of
  otherwise shallow reusable definitions or UDTs. Replaced with heap-backed DFS,
  preserving deterministic cycle reporting. Regression:
  long_reference_chain_uses_heap_and_reports_cycle (20,000 vertices).

Independent local preflight on Linux x86_64, Cargo 1.100.0: 74 tests passed,
fmt and Clippy -D warnings passed, rustdoc -D warnings passed, both valid
fixtures accepted. No new dependencies or JSON Schema structural changes.
Exact corrected-head Windows/Linux CI remains required. Full advisory/license
audit and Kotlin golden round-trip/render parity are not established.
No production execution or private source migration is claimed. No previously
rejected architecture is revived by these corrections.

Next exact action: verify corrected-head CI, review the final patch, resolve
remaining acceptance evidence, then integrate after PR #1 without overwriting its
continuity records. Update the shared handoff/decision registers when integrating
the independently scoped crate (PR #1 owns those files until merged).
