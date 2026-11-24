# REUSE-01 — Legacy project migration

Status: **proposed**. This document covers importing HMI Forge projects (schema v1–v10, HMI Forge `92a612d`) into
HMIStudio canonical projects (`project-model`, HMIStudio `264ace7`). Nothing in it is implemented.

## 1. Inputs

| Input | Form | Notes |
| --- | --- | --- |
| Portable project file | `*.hmiforge.json` (`storage-jvm/PortableProjectFiles`) | strict JSON, the schema version is declared, and assets are inline |
| Revision store | `.hmistudio` bundle (D-006: immutable checksummed revisions, atomic HEAD) | the importer reads HEAD (or a chosen revision) after verifying its checksum |
| Runtime package | `.hmipkg` format 1 or 2 | contains `project.json`. Allowed as an input of last resort; the signature is checked, but trust is not required for import |

## 2. Principles (binding for the importer)

1. **Never discard silently.** Every source field ends up in one of three places, and the report lists each one
   with its source JSON path:
   - **mapped**: a canonical equivalent;
   - **preserved**: kept verbatim in `extensions["hmiforge.<json path>"]`;
   - **refused**: the import stops.
2. Severity per finding:
   - **info**: lossless mapping;
   - **warning**: approximation with defined semantics, for example `navigate.startup` becoming `Navigate` to the
     start screen;
   - **blocking**: a feature that is preserved but not functional. The project opens, but validation forbids
     publishing until it is resolved.
3. **Safety-relevant data is never approximated.** This covers write commands, `writeConfirmed`,
   `commissioningAllowed`, write limits, permissions and `requiredPermission`. Until the canonical model supports
   these, they are preserved and marked blocking.
4. **The import is deterministic.** The same input gives byte-identical canonical JSON and the same report. Ids that
   are not valid in the HMIStudio global namespace, or that collide across categories, are renamed by a fixed rule,
   and the old → new map is stored in the report and in `extensions["hmiforge.idMap"]`.
5. **Read-only towards the source.** The source file is never written. The output is a new HMIStudio project.
6. **Minimal-version files.** HMI Forge writes the lowest schema version that a project needs. The importer accepts
   v1–v10 and refuses anything else, including newer versions, with a clear message. This mirrors
   `SUPPORTED_SCHEMA_VERSIONS`.

## 3. Pipeline

1. Decode with a Rust mirror of the HMI Forge types (strict, refusing unknown keys). It is version-aware: v1 is
   valid v3, and so on, because each version only adds optional fields.
2. Validate structurally with the rules ported from `core/validation`. A source project that HMI Forge itself would
   flag invalid is reported as such, not repaired silently.
3. Map it according to `PROJECT_MODEL_COMPATIBILITY.md` §2–5.
4. Validate the result with RUST-03A validation (`rejectNewErrors` by default, per 03B-2).
5. Write the canonical project through engineering-core persistence, and the migration report as a sidecar
   (`<project>.import-report.json` and a Markdown summary).

The pipeline is an engineering-core command, `ImportLegacyProject`, so that the UI and the optional AI use the same
validated operation (AGENTS.md).

## 4. Migration report format (proposal)

`{ sourceFormat: "hmiforge", sourceSchemaVersion, sourceSha256, importerVersion, findings: [ { severity, path,
code, message, disposition: mapped|preserved|refused, target } ], idMap }`

## 5. Fixtures

| Fixture | Source | Licence state |
| --- | --- | --- |
| `examples/conveyor-demo.hmiforge.json` (v1, `RectDp`) | committed in HMI Forge | owner-authored; transfer to public repository needs licence decision O-1 |
| Demo projects pump station, food line, live conveyor (`core/demo/DemoProjects.kt`) | built in code | export to JSON with a small Gradle task in the runtime repository; same licence condition |
| Synthetic per-version fixtures v1…v10 | to be written | written for HMIStudio from the published schema description, not copied |
| Negative fixtures: unknown key, version 11, broken checksum, duplicate ids, expression that does not parse | to be written | same |

**No customer projects** are used. Any real project needs explicit owner authorization and must be kept outside the
public repository.

## 6. Acceptance for REUSE-03

- Every fixture imports deterministically: imported twice, the bytes are identical.
- The report accounts for every source JSON leaf. This is checked by a test that walks the source tree and asserts
  that each path has a disposition.
- No blocking finding is reported as mapped.
- Write-command and security fields appear as blocking until model version 2 lands. After that, they are mapped and
  covered by round-trip tests.
- Engine fixtures from the legacy project re-validate against the engine's `validate_config` rules once compiled
  (REUSE-04).
