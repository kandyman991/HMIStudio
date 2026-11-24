# REUSE-01 — Project model compatibility

Status: **proposed**. This document compares HMI Forge `core/model` (schema v1–v10, at `92a612d`) with HMIStudio
`crates/project-model` (schema version 1, RUST-03A) and `crates/engineering-core` (RUST-03B), both at `264ace7`.

## 1. Structural difference in one paragraph

HMI Forge separates **internal variables**, which screens bind to, from **external mappings**, which connect a
variable to a source and an address. Screens never contain addresses. Components carry string-typed
`properties` and `bindings` maps, plus optional `expressions` written in a typed read-only expression language.
Geometry is the legacy absolute `RectDp`, with an optional adaptive `ScreenLayout` tree from v2 onwards. Security
(roles, permissions) and write semantics (commands, confirmation, limits, commissioning) are part of the model.

HMIStudio binds tags directly. A tag carries an optional connection and address. Bindings are typed and limited to
`linear`, `invert`, `bit` and `map` transforms, with no expression language. Layout is a semantic node tree. There
is a global ID namespace, typed JSON values, data types including structs and arrays, units, recipes, localization,
device profiles, breakpoints and `extensions` maps on most objects. HMIStudio has no security section and no write
command semantics.

## 2. Top-level field mapping

| HMI Forge `HmiProject` | HMIStudio `Project` | Mapping |
| --- | --- | --- |
| `schemaVersion` (1–10) | `format`, `schemaVersion` (1) | import records source version in migration report |
| `metadata.name`, `description` | `metadata` | direct |
| `metadata.defaultScreenId` | `navigation` start screen | direct |
| `metadata.targetProfiles` (strings) | `deviceProfiles` | create profiles by name; dimensions unknown → diagnostic |
| `screens` (map) | `screens` (list) | ordered by `screenOrder`, then id |
| `screenOrder` | `navigation` items / screen order | direct |
| `componentDefinitions` | `components` (`ComponentDefinition`) | see §5 |
| `variables` (`InternalVariable`) | `tags` | see §3 |
| `externalSources` | `connections` | `driver` → `DriverKind` (`RUST_ENGINE_INTEGRATION.md` §5); `configuration` map → `parameters`; `commissioningAllowed` → **missing** |
| `externalMappings` | tag `connection` + `address` | one READ_WRITE mapping per variable maps directly; more than one mapping per variable → **missing** |
| `alarms` | `alarms` | `sourceVariableId` → `tag` + `IsTrue {}`; severity INFO→Info, WARNING→Warning, HIGH→Alarm, CRITICAL→Critical; `message` → `LocalizedText` |
| `assets` | `resources` | ids, media type, data |
| `theme` | `themes`, `styles` | ADAPT; per-field review in REUSE-03 |
| `security` (`SecurityModel`) | **none** | **missing**: permissions, roles, anonymous role, auto-logout |
| `enumerations` (value, text, colour) | none as a type | `map` transform per binding, or new section (decision O-3) |
| — | `units`, `dataTypes`, `recipes`, `localization`, `breakpoints`, `settings` | HMIStudio-only; empty on import |

## 3. Variables, mappings and tags

| HMI Forge | HMIStudio | Rule |
| --- | --- | --- |
| `InternalVariable.id`, `path` | `Tag.id`, `name` | id kept if valid in the global namespace; otherwise renamed with a recorded id map |
| `type` BOOL/INT/LONG/FLOAT/DOUBLE/STRING | `Bool`/`Int32`/`Int64`/`Float32`/`Float64`/`String` | lossless |
| `access` READ_ONLY/READ_WRITE | `Read`/`ReadWrite` | lossless |
| `access` COMMAND/SETPOINT | `ReadWrite` + **missing semantic** | preserved in `extensions`; blocking diagnostic until model supports it |
| `engineeringUnit` (string) | `unit` → `EngineeringUnit` | create unit by symbol |
| `description` | `description` | direct |
| `defaultValue` | `initialValue` | typed conversion |
| `simulationValue` | simulator configuration (tag-engine) | preserved in `extensions`; mapped when simulator config is part of the model |
| `ExternalMapping.externalAddress` | `Tag.address` | direct |
| `ExternalMapping.access` READ/WRITE/READ_WRITE | `TagAccess` | intersect with variable access |
| `ExternalMapping.writeConfirmed` | **missing** | safety-relevant → blocking |
| `ExternalMapping.command` (`WriteCommand`: MOMENTARY/MAINTAINED/SETPOINT, critical, confirm, pulseMs, min, max, step) | partly: `PulseTag`/`StepTag` actions, `range` | **missing** as tag semantics → blocking |
| limits (min/max) | `range` | `range` describes display range; write limits are a separate concept → keep separate (decision O-3) |

## 4. Screens, components, layout, actions

| HMI Forge | HMIStudio | Rule |
| --- | --- | --- |
| `HmiScreen.widthDp`, `heightDp` | `designProfile` / root size | create a profile of that size |
| `HmiScreen.requiredPermission` | **missing** | blocking if security is used |
| `HmiScreen.actions` (events `open`, `close`) | screen handlers (`Show`, `Hide`) | `open`→`Show`, `close`→`Hide` |
| Component `geometry` (`RectDp`), no `layout` | `Element::Absolute` with `Position` + `Fixed` size | lossless for position/size |
| `ScreenLayout.Stack` ROW/COLUMN/GRID, `gapDp`, `paddingDp`, `weight`, `columns` | `Row`/`Column`/`Grid`, gap, `Insets`, `Fill{weight}`, tracks | ADAPT; ADAPTIVE direction → `responsive` rule (INVESTIGATE exact breakpoint semantics) |
| `Slot` (weight, min/max, alignH/V) | `LayoutParams`, `Alignment` | ADAPT |
| `ScaleGroup` (design size, origin, min/max scale) | **none** | INVESTIGATE; preserve in `extensions`, render with `Absolute` + diagnostic |
| `zIndex` | child order | sort by zIndex then id |
| `visible` | `Visibility` | direct |
| `properties: Map<String,String>` | typed `properties` | parse per catalog type; unparsable → keep raw in `extensions` + diagnostic |
| `bindings: slot → variableId` | `PropertyBinding { source: Tag }` | slot names mapped per component catalog |
| `expressions: slot → text` | `transform` where expressible (`scale(x,a,b,c,d)`→`linear`, `!x`→`invert`) | otherwise **missing** → preserve text + blocking diagnostic (decision O-4) |
| events `press`, `release`, `hold`, `valueChange` | `Press`, `Release`, `LongPress`, `ValueChanged` | `hold` with `holdMs` → `LongPress` (duration INVESTIGATE) |
| `navigate.screen`, `navigate.back` | `Navigate`, `Back` | direct |
| `navigate.startup` | **none** | map to `Navigate` to start screen (equivalent while start screen unchanged) |
| `open.popup`, `popup.close` | `OpenPopup`, `ClosePopup` | direct |
| `open.faceplate` (definition + parameter values) | **none** | **missing**; preserve + blocking |
| `value.set`, `value.toggle`, `value.pulse`, `value.step` | `WriteTag`, `ToggleTag`, `PulseTag`, `StepTag` | direct (pulse ms, step/min/max) |
| `requiredPermission` on component | **missing** | blocking if used |
| up to 8 actions per event | `EventHandler` action list | check HMIStudio limit |

### Component types

HMI Forge `StandardComponentCatalog` has 28 types. HMIStudio `ComponentKind` has 13 (Button, Text, Image,
Rectangle, Ellipse, NumericDisplay, NumericInput, Indicator, Gauge, Slider, Switch, ProgressBar, Container).

| HMI Forge | HMIStudio |
| --- | --- |
| button, label, image, svgImage, rectangle, ellipse, numericField, numericInput, indicator, gauge, slider, switch, composite | direct or near-direct (label→Text, numericField→NumericDisplay, svgImage→Image, composite→Container/Instance) |
| levelIndicator | ProgressBar (vertical) — INVESTIGATE fidelity |
| line, arrow, polygon | **missing** primitive shapes |
| textInput, dropDown | **missing** inputs |
| motor, valve, conveyor, sensor, pump, tank, pipe, thermalIndicator, flowIndicator | **missing** industrial symbols; proposal: a built-in component-definition library rather than new kinds (decision O-3) |

## 5. Reusable definitions

| HMI Forge `ComponentDefinition` | HMIStudio `ComponentDefinition` |
| --- | --- |
| `bindingSlots`, `defaultProperties` | `parameters` + root node properties |
| `kind` COMPONENT/FACEPLATE | no kind; faceplate = definition opened as popup → needs `open.faceplate` |
| `parameters` (BOOL/INT/FLOAT/STRING/COLOR, BOOL_/NUMBER_/TEXT_VARIABLE) | `PropertyType` Bool/Integer/Number/String/Color/**Tag** | variable parameters → `Tag` parameter (`TagParameter` binding) |
| `composition` | `root: Node` | ADAPT |
| `category`, `description` | same | direct |

## 6. Canonical contract proposal

1. **There is one editable model: HMIStudio `project-model`.** D-001 and AGENTS.md already make Rust the
   authoritative engineering core. The HMI Forge JSON is not kept as a second editable format.
2. **HMI Forge v1–v10 is a legacy input only.** A read-only Rust decoder that mirrors the HMI Forge types turns it
   into canonical projects (`LEGACY_PROJECT_MIGRATION.md`). It never writes HMI Forge JSON back.
3. **The runtime model is derived.** Until the Kotlin runtime reads the canonical model natively (REUSE-04 phase 2),
   Studio compiles the canonical project into the HMI Forge runtime package format. This is a build output, like a
   compiled binary, never edited by hand. These rules prevent the two from diverging:
   - The compiler is the only writer of runtime packages, and it **refuses** to publish any canonical feature that
     it cannot express. It never drops a feature silently.
   - Golden fixtures, run in both repositories, check that canonical → package → Kotlin decoder produces the
     expected runtime model, and that the result validates with the HMI Forge `core/validation` rules.
   - The schema versions are pinned on both sides: canonical `schemaVersion` N maps to runtime package format F. A
     change on either side needs a fixture update in both repositories.
   - Each new canonical feature is added to the runtime first or at the same time, or it is blocked by the compiler.
4. **Canonical schema version 2.** It is proposed for REUSE-03, through the existing `MigrationRegistry`, with an
   automatic v1 → v2 step. It would add:
   - a `S7` driver kind (and possibly a test `loopback` kind);
   - write semantics on tags: command kind, confirmation, critical, write limits separate from the display range,
     `writeConfirmed`, and the connection-level `commissioningAllowed`;
   - a security section with permissions, roles, the anonymous role, auto-logout and `requiredPermission` on
     screens and nodes;
   - an `openFaceplate` action and a `navigateStartup` action, if they cannot be expressed otherwise;
   - optionally, expressions (decision O-4).

   Every addition needs its own engineering-core commands and validation rules, under the RUST-03B rules.
5. **Extensions are the preservation mechanism.** Anything that has no canonical equivalent is copied verbatim into
   `extensions["hmiforge.<path>"]` and produces a blocking diagnostic. The project then opens, but cannot be
   published until the gap is resolved.

## 7. Engineering-core overlap

| HMI Forge | HMIStudio engineering-core | Disposition |
| --- | --- | --- |
| 39 `ProjectOperation` kinds | 34 typed commands | do not port; add commands for model additions in §6.4 only |
| `OperationHistory` undo/redo | snapshot undo (03B-1) | keep HMIStudio |
| `.hmistudio` revision bundle (D-006) | staging + verified backup + sidecar autosave (03B-5/6) | keep HMIStudio; legacy reader only |
| validation (509 lines) | RUST-03A validation | port missing rules (e.g. write-path consistency, permission references) with the model additions |
| cross-reference/search (IDE-12A) | none | PORT under RUST-06 search |
