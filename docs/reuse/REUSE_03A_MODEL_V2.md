# REUSE-03A — Canonical model v2 contract and compatibility baseline

Status: first implementation slice / design candidate. Base main f86f4338e33cd6889f42e7d8511a04b938fb1f42. Existing schema remains v1 until all v2 additions, commands and consumer checks land together; do not label this as a frozen or implemented v2.

## Owner decisions, 2026-11-25

- O-3: adopt the canonical v2 additions for connection kinds, separate write semantics, security, faceplate/startup actions, enumerations and industrial components.
- O-4: adopt the HMI Forge expression language contract 1 via a validated Rust implementation, not a Kotlin-source transfer.
- O-1 fixture scope: newly authored synthetic fixtures only initially. No private demo project, customer project, private source, credentials or signing material transferred. Any future actual legacy fixture needs separate exact rights and sensitive-content review.

## Contract candidate

| Area | Proposed representation and required invariant |
| --- | --- |
| Version | Keep the committed v1 schema and fixtures immutable as historical contract; publish v2 schema under a new versioned filename when complete. Register explicit v1-to-v2 migration; accept v1, reject unsupported versions and duplicates. New v2-only keys in v1 must not accidentally be interpreted as authorized policy. |
| Drivers | Reuse existing SiemensS7 identifier (no duplicate S7 enum). Add a separate loopback declaration with no hardware access; do not auto-register physical drivers or connect on load. Driver-specific parameters are non-secret; secure-store refs only. |
| Write policy | Optional typed tag declaration for command kind, confirmation, critical, pulse duration, exact numeric limits and step, and explicit confirmation evidence flag; separate connection commissioningAllowed default false. Preserve display range separately. Never round integer limits through f64; fail closed on invalid or unrepresentable data. Model declarations do not authorize writes. Preserve existing simulation behavior. |
| Security | Typed permissions and roles with stable identifiers, anonymous role and auto-logout; permission refs on screens/nodes, global uniqueness and reference validation. No passwords/tokens or imported active trust. Unresolved/unsupported policy blocks publish, never grants access. |
| Actions | Typed startup navigation resolves current startup target; faceplate opens a reusable definition with typed parameters and validated references. No execution during import; retain existing transaction and event validation. |
| Enumerations | Typed discrete values, localized labels and optional color, exact equality and unique values; distinguish display maps from write limits. |
| Components | Industrial symbols use reusable definition library where semantically sufficient; extend catalog only for missing primitives/inputs with explicit property and binding contracts. No mock rendering or silent approximation. |
| Expressions | Contract-versioned read-only binding source; validate syntax, type and referenced tags/parameters, bounded evaluation; no I/O, writes, arbitrary code or unsafe coercions. Pin contract-1 behavior and compare against independently authored golden vectors before enabling; unsupported expressions produce blocking diagnostics. |

## Implementation order

1. Compatibility regression baseline (this slice). Existing model already includes SiemensS7, typed transforms, tag parameters, exact JSON integer handling and a migration registry. Current schema export and several fixture assertions are hard-coded v1; update consumers deliberately rather than changing the version constant alone.
2. Specify concrete serialized fields/types and contract-1 vectors against the authorized audit and read-only behavior reference. Implement industrial declarations, policy and security validation plus typed engineering commands. Preserve undo/redo, reference deletion/rewrite behavior and transaction rejection.
3. Add typed action, enumeration, component and expression contracts, validation and command coverage. Review downstream tag-engine, Tauri and frontend consumers; extend the reservation before touching paths outside the current scope. No runtime behavior or publishing claim.
4. Activate v2 only when the complete additive schema, v1 migration and generated schema tests agree; verify every existing fixture and all affected downstream builds, migration determinism, exact values, unknown/duplicate key rejection, policy defaults, references, undo/redo and save/reopen. Freeze v2 in a separately reviewed completion candidate before the legacy decoder maps to it.

## First-slice validation

New original synthetic compatibility tests assert exact u64 preservation, extension preservation, binding/action references, deterministic load/save, and duplicate/unknown-field rejection through the existing v1 loader. They remain applicable when v2 is activated (adjust expected additive defaults deliberately, not by discarding assertions). No version bump, decoder, private fixtures or runtime implementation in this slice.

## Second slice — Industrial and security declarations (2026-11-25)

Implemented typed staging declarations in `model/v2.rs` and deterministic semantic validation via `validation::validate_v2_declarations`. These types are deliberately not attached to the active v1 Project root or runtime; they are preparation for the single canonical v2 schema, not a new editable project format.

- Reuse SiemensS7 naming. Loopback is a distinct candidate driver kind, permitted only against an endpoint-free, credential-free simulator with no driver parameters; simulation drivers cannot declare physical commissioning. Commissioning intent and writeConfirmed default false and never constitute authorization.
- Momentary/maintained commands require boolean writable tags; momentary requires positive duration. Setpoints require writable numeric scalar tags. Critical commands require confirmation. Numeric limits are separate from display range; validate exact signed/unsigned widths without f64 coercion, min <= max and positive step. Conservative candidate rejects f32 limit values that would narrow on conversion and integer literals that would round when interpreted as floats. This is an engineering default subject to contract review; no runtime limit enforcement is implied.
- Typed permission and role identifiers share the existing global namespace. Validate nonempty names, unique declaration/reference entries, existing role and permission references, positive configured auto-logout, and screen/node references (including reusable definition nodes). No user/password store, imported trust or permission evaluator.
- Validate the base project before staged declarations. No project mutation, PLC connection or write execution. Unknown fields remain rejected. Seven new synthetic tests cover round-trip staging, v1 rejection of v2-only fields, exact adjacent u64 limits, invalid commands, narrowing, identity collisions, missing and wrong-kind references, duplicate declarations, endpoint/commissioning refusal and invalid base handling. The initial u64 test had an incompatible fractional recipe value in its synthetic base; corrected the fixture, not validation.

Next: integrate these declarations into typed engineering operations and complete action, enumeration, component and expression contracts. Before activating v2, finalize inline serialized fields, v1-specific admission, schema migration, persistence/undo behavior, downstream consumers and publish refusal for unsupported features. Do not claim schema freeze or runtime enforcement yet.
