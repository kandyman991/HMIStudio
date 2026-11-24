# HMI Studio project model (RUST-03A)

Status: **candidate** — implemented in `crates/project-model/` (`hmi-project-model`),
not yet integrated with the desktop shell or any runtime. See
[Limitations](#limitations) and [Integration steps](#recommended-integration-steps).

Related documents:

- [PROJECT_SCHEMA.md](PROJECT_SCHEMA.md) — JSON document format reference.
- [PROJECT_MODEL_VALIDATION.md](PROJECT_MODEL_VALIDATION.md) — validation rules, diagnostic codes, test evidence.

## Purpose

The project model is the single, authoritative description of an HMI project.
It is the interchange contract between:

- the **Rust engineering environment** (Tauri 2 desktop editor; React/TypeScript UI calls
  typed Rust commands that mutate this model), and
- the **Kotlin Multiplatform / Compose runtimes** on Windows, Linux and Android.

Both sides consume the *same semantic representation*. The editor's HTML/CSS
canvas is a *rendering* of the model, exactly like the Compose runtime is; neither
is translated into the other. No HTML is ever converted into Kotlin.

```
                 ┌───────────────────────────────┐
  React/TS UI ──▶│ Tauri commands (src-tauri/)   │  (RUST-00/01, not this PR)
                 └──────────────┬────────────────┘
                                │ typed mutations
                 ┌──────────────▼────────────────┐
                 │ hmi-project-model (this crate)│  model · catalog · validation
                 │  canonical JSON + JSON Schema │  serialization · migration
                 └──────┬─────────────────┬──────┘
                        │ .hmi.json       │ .hmi.json (+ assets)
          HTML canvas renderer     Kotlin/Compose runtime
          (editor preview)         (Windows · Linux · Android)
```

## Crate boundaries

- No dependency on Tauri, WebView, React/HTML/CSS, Kotlin or any UI toolkit
  (`cargo tree` lists only serde, serde_json, serde_path_to_error, schemars,
  thiserror and their proc-macro support crates).
- No I/O beyond string in / string out; file access, packaging and publishing
  belong to callers.
- No communication drivers, alarm evaluation, recipe execution, Compose generation
  or deployment. Those consume the declarative contracts defined here.
- The crate is its own Cargo workspace root (`[workspace]` in its `Cargo.toml`) with
  its own committed `Cargo.lock`, so it builds and tests independently of `src-tauri/`.

## Module map

| Module | Contents |
| --- | --- |
| `id` | Typed identifier newtypes (`ScreenId`, `NodeId`, `TagId`, …), syntax rules, deterministic `IdAllocator`. |
| `model::project` | `Project` root, metadata, settings, device profiles, breakpoints, screens, navigation, localization. |
| `model::layout` | `Node` tree, `Element` kinds, `LayoutParams`, sizes, tracks, insets, responsive rules. |
| `model::component` | Built-in `ComponentKind`s, `PropertyType`, reusable `ComponentDefinition`s, bindings, transforms, events, actions. |
| `model::style` | Themes (design tokens), named styles, resources/assets. |
| `model::industrial` | Connections, tags, data types (incl. structs/arrays), units, alarms, recipes. |
| `catalog` | Per-kind typed property catalog, bindability, allowed events, Compose mapping hints. |
| `serialization` | Load pipeline, version detection, canonical JSON, lossless save check. |
| `migration` | `Migration` trait and `MigrationRegistry` (raw-JSON, single-step upgrades). |
| `validation` | Deterministic validator producing structured `Diagnostic`s. |
| `schema` | JSON Schema (draft 2020-12) generation; committed copy in `crates/project-model/schema/`. |

## Project structure

| Concept | Representation |
| --- | --- |
| Stable identifiers | Explicit string ids on every addressable object, one project-wide namespace, `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`. Never derived from names; renaming or moving never changes them. |
| Schema version | `format: "hmi-studio.project"` + integer `schemaVersion` (currently `1`). |
| Metadata | Name, description, author, revision, caller-supplied timestamps, keywords. |
| Screens & navigation | `screens[]` (pages and popups, each with a root `Node`), `navigation.startScreen`, ordered menu `items`. |
| Device & resolution profiles | `deviceProfiles[]` (platform, pixel resolution, density, orientation, touch) and `breakpoints[]` (dp width classes). |
| Component definitions & instances | `components[]` reusable definitions with typed parameters (including `tag` parameters); `instance` nodes reference them. |
| Resources | `resources[]` with kind, portable lexical relative path (I/O containment and symlink checks are separate), media type, optional SHA-256. |
| Styles & themes | `themes[]` (color/typography/metric tokens), `styles[]` (named property sets, optionally per component kind). |
| Extensible metadata | `extensions` maps (namespaced keys, arbitrary JSON) on the project, nodes, screens, definitions, tags, connections and resources. Preserved verbatim. |

## Semantic layout system

A screen is a tree of `Node`s. Each node has an `element` (what it is), `layout`
parameters (how its *parent* sizes and places it), `children`, typed `properties`,
`bindings`, `events` and ordered `responsive` overrides.

Lengths are density-independent pixels (dp in Compose; CSS px at 1:1 in the editor).

| Model | Meaning | Compose mapping |
| --- | --- | --- |
| `row` (`arrangement`, `alignment`, `spacing`, `wrap`) | Horizontal flow | `Row(horizontalArrangement = …spacedBy/SpaceBetween…, verticalAlignment = …)`; `wrap` → `FlowRow` |
| `column` | Vertical flow | `Column(...)`; `wrap` → `FlowColumn` |
| `box` (`contentAlignment`) | Overlay / stack | `Box(contentAlignment = …)` |
| `grid` (`columns`, `rows`, gaps) | Track grid with spans | Custom `Layout` measuring fixed/auto/fraction tracks (Compose's lazy grids lack row spans) |
| `scroll` (`direction`) | Scroll viewport, exactly one child | `Modifier.verticalScroll / horizontalScroll(rememberScrollState())` on a `Box` |
| `absolute` | Explicit coordinates (mimics, overlays) | `Box` + child `Modifier.offset(x.dp, y.dp)` |
| `component` (`kind`) | Built-in widget | See component table |
| `instance` (`definition`, `parameters`) | Reusable component | Generated `@Composable fun <Definition>(params…)` call |
| `width/height: auto` | Hug content | `wrapContentWidth/Height()` (default) |
| `width/height: fill(fraction)` | Fill container | `fillMaxWidth/Height(fraction)` |
| `width/height: fixed(value)` | Fixed size | `width/height(value.dp)` |
| `minWidth`…`maxHeight` | Constraints | `widthIn/heightIn(min, max)` |
| `aspectRatio` | Ratio | `Modifier.aspectRatio(r)` |
| `padding` (start/top/end/bottom) | Inner padding (RTL-aware) | `Modifier.padding(start, top, end, bottom)` |
| `weight` | Flex share in row/column | `Modifier.weight(w)` (RowScope/ColumnScope) |
| `crossAlign` | Self cross-axis alignment | `Modifier.align(Alignment.Vertical/Horizontal)` |
| `align` | Self alignment in a box | `Modifier.align(Alignment.X)` (BoxScope) |
| `position` | Coordinates in `absolute` | `Modifier.offset` |
| `gridCell` | Cell + spans in `grid` | Parent-data modifier read by the grid `Layout` |
| `visibility: hidden` | Keeps space, not drawn/interactive | `Modifier.alpha(0f)` + disabled input |
| `visibility: collapsed` | Not composed | `if (visible) { … }` |
| `clip` | Clip to bounds | `Modifier.clipToBounds()` / `clip(shape)` |
| `zIndex` | Paint order (ties keep child order) | `Modifier.zIndex(z)` |
| `responsive[]` | Ordered overrides when conditions match | `BoxWithConstraints` / window size class; last matching rule wins |

Responsive rules (`when` → `set`) can match a named breakpoint, a min/max window
width, orientation, platform or device profile, and override sizes, constraints,
padding, weight, visibility, spacing, flow `direction` (row ⇄ column), grid
`columns`, `gridCell`, `position` and `zIndex`. This is what lets a Figma-like
editor offer auto-layout *and* free positioning while the runtime still produces
responsive Compose layouts; fixed coordinates are only used inside explicit
`absolute` containers.

## Component model

Built-in kinds and their catalog live in `src/catalog.rs` (single source of truth).
Every node also accepts the common properties `visible`, `enabled`, `opacity`,
`background`, `borderColor`, `borderWidth`, `cornerRadius`, `tooltip`,
`accessibilityLabel`.

| Kind | Key properties (bindable **R**/**RW**) | Events | Compose mapping |
| --- | --- | --- | --- |
| `button` | text R, icon R, behavior, pressedColor, text styling | click, press, release, longPress | `Button` + `Text`/`Icon` |
| `text` | text R, maxLines, overflow, text styling | click | `Text` |
| `image` | source R, contentScale, tint R | click | `Image(painterResource / vector)` |
| `rectangle` | fill R, strokeColor R, strokeWidth | click | `Box(background(shape)).border` |
| `ellipse` | fill R, strokeColor R, strokeWidth | click | `Box(CircleShape)` / `Canvas.drawOval` |
| `numericDisplay` | value R, decimals, showUnit, prefix | click | `Text` with number formatting |
| `numericInput` | value **RW**, min, max, step, decimals | valueChanged | `OutlinedTextField(KeyboardType.Decimal)` + keypad |
| `indicator` | active R, activeColor R, inactiveColor R, shape, label R, blink R | click | `Box(CircleShape)` with animated color |
| `gauge` | value R, min, max, lowLimit R, highLimit R, variant, majorTicks | click | Custom `Canvas` composable |
| `slider` | value **RW**, min, max, step, orientation | valueChanged | `Slider` |
| `switch` | checked **RW**, label R | valueChanged | `Switch` |
| `progressBar` | value R, min, max, orientation, barColor R | click | `LinearProgressIndicator` (vertical via rotation/custom) |
| `container` | elevation (+ common); at most one child | click | `Surface` / `Card` |

Property value forms: colors are `#RRGGBB[AA]` or `{ "token": "<theme color>" }`;
operator text is a literal string or `{ "textKey": "<key>" }`; resources are
resource ids.

**Reusable components.** A `ComponentDefinition` has typed `parameters` and a root
`Node`. Parameters of kind `tag` let one faceplate be wired to different tags per
instance (`{ "tagParameter": "motor" }` as a binding source or action target).
Value parameters (`{ "parameter": "title" }`) flow into properties read-only, and
can themselves be bound to tags on the instance. Definitions may nest instances of
other definitions; cycles are rejected. In Compose each definition becomes one
`@Composable` function whose parameters are the definition's parameters (tag
parameters become tag handles).

## Industrial engineering contracts

Declarations only — no drivers or execution engines in this milestone.

- **Connections**: driver family (`simulator`, `opcUa`, `siemensS7`, `modbusTcp`,
  `custom`), endpoint, non-secret parameters, `credentialRef` into the runtime's
  secure store. Projects never carry secrets (the validator warns on secret-looking
  parameter names).
- **Tags & data types**: scalar types (bool, int8…uint64, float32/64, string,
  dateTime), fixed-length arrays and user-defined structs (PLC UDTs); connection +
  address or internal (HMI memory) tags; access rights; unit; engineering range;
  initial/simulation value; scan rate.
- **Bindings**: property ← source (`tag`, `tagParameter`, `parameter`), optional
  member path (`status.running`, `codes[3]`), mode `read`/`write`/`readWrite`, and
  transforms `linear` (invertible), `invert`, `bit`, `map` (read-only).
- **Events & actions**: navigate, back, open/close popup, writeTag, toggleTag,
  stepTag, pulseTag, acknowledgeAlarms, load/saveRecipe, setLocale; optional
  operator confirmation text.
- **Alarms**: tag (+member), condition (high, low, equals, bitSet, isTrue, isFalse),
  deadband, delay, severity, priority 1–1000, localized message, group,
  acknowledge/latched flags.
- **Recipes**: fields mapped to writable tags; named value sets.
- **Localization**: declared BCP-47 locales, default locale, key → locale → text.
- **Engineering units**: symbol, localized name, quantity.

## Runtime interoperability and deployment modes

The runtime reads the same canonical JSON. Two deployment modes are anticipated,
both carrying the identical document:

1. **Standalone runtime application**: the project document and its `resources`
   are embedded in a runtime build (e.g. as app assets / bundle resources).
2. **Project-only publishing**: the document plus resources are packaged and sent
   to an already-installed, paired runtime. `Project.id`, `metadata.revision`,
   resource `sha256` values and `schemaVersion` give the runtime what it needs to
   identify, verify and accept/refuse a publish.

A runtime must refuse documents whose `schemaVersion` it does not support, exactly
as this crate does. Connection `endpoint`s and `credentialRef`s are expected to be
overridable per deployment target at publish time (publishing profiles are a later
milestone). Packaging format, pairing, signing and trusted deployment are explicitly
out of scope here (RUST-07 and later).

Interoperability has **not yet been verified** against a Kotlin implementation; the
mapping tables above are the design intent. Recommended verification is listed below.

## Architectural decisions

1. **Semantic tree, not coordinates.** Flow containers plus constraints are the
   default; `absolute` is an explicit, local opt-in. This matches both Figma
   auto-layout and Compose and keeps layouts responsive.
2. **One global id namespace.** Simplifies references, diffs and runtime lookups;
   ids are validated, not auto-repaired.
3. **Reject unknown fields.** `deny_unknown_fields` everywhere and empty struct
   variants instead of unit variants (serde silently ignores extra keys on unit
   variants of internally tagged enums). Forward data goes in `extensions`.
4. **Structural deserialization is lenient on values, strict on shape.** Malformed
   ids, out-of-range numbers and broken references deserialize and are reported by
   the validator with JSON Pointer paths, so the editor can show every problem at
   once instead of failing on the first.
5. **Raw-JSON migrations.** Each migration upgrades one version on
   `serde_json::Value`, so old Rust types never need to be kept around.
6. **Canonical JSON with a lossless self-check.** Sorted maps, declaration-order
   fields, omitted defaults, trailing newline; every save re-parses its output and
   compares models, so NaN/∞ can never be silently written as `null`.
7. **Schema generated from code.** `schemars` derives the JSON Schema from the same
   types serde uses; a test fails if the committed copy drifts.
8. **Property catalog in Rust.** Typed properties per kind with explicit
   bindability; extending it is additive.
9. **Bounded depth.** Layout depth ≤ 32 (`MAX_NODE_DEPTH`), below `serde_json`'s
   recursion limit, protecting recursive processing on every platform.

## Limitations

- No expression language for bindings (only the four transforms); no multi-source
  bindings, no string formatting patterns beyond `decimals`/`prefix`.
- No user roles/permissions or security levels on actions (planned with guarded
  writes).
- No trend/chart, alarm viewer, table or recipe-editor components yet; the example
  uses a labeled placeholder for the alarm list.
- `dateTime` values are not parsed (only required to be strings). Timestamps in
  metadata are not validated.
- Unit conversion between engineering units is not modeled.
- Grid responsive `gridCell` overrides are validated against the grid's *base*
  tracks, not against simultaneously overridden `columns`.
- Cycle detection reports at least one cycle per strongly connected group of
  definitions/types (all cycles in the tests), not necessarily every elementary cycle.
- No packaging format (project + assets archive), no publish profiles, no signing.
- Compose mapping is documented, not generated or tested against Kotlin.
- The model has no command/transaction or undo/redo layer (RUST-03 follow-up).
- JSON Schema covers structure only; semantic rules require the Rust validator
  (or a port of it).

## Recommended integration steps

1. Review and merge RUST-00/01 first; this crate does not touch its files.
2. Decide workspace layout: either add a root `Cargo.toml` workspace with members
   `src-tauri` and `crates/project-model` (remove this crate's empty `[workspace]`
   table and merge lockfiles), or keep it standalone and depend on it by path from
   `src-tauri/Cargo.toml`.
3. Fold the dedicated `project-model` CI job into the main workflow once PR #1's CI
   is settled (or keep it separate and path-filtered).
4. Replace the in-memory demo `Engine` in `src-tauri` with a model-backed engine:
   load/save via `load_project`/`save_project`, expose `ValidationReport` to the UI.
5. Build the command/transaction + undo/redo layer (RUST-03 remainder) over this
   model, using `IdAllocator` for deterministic ids.
6. Port or generate Kotlin data classes from `schema/hmi-project.v1.schema.json`
   (e.g. kotlinx.serialization), and add a cross-language golden test: Kotlin
   parses every `fixtures/valid/*.hmi.json`, re-serializes canonically and
   compares with Rust output.
7. Implement the HTML editor renderer and the Compose renderer from the same
   fixtures and add layout-parity screenshots per device profile (RUST-05).
8. Add publish packaging and both deployment modes on top of this document format
   (RUST-07).
