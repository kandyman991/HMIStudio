# HMI Studio project JSON format (schemaVersion 1)

Machine-readable schema: [`crates/project-model/schema/hmi-project.v1.schema.json`](../crates/project-model/schema/hmi-project.v1.schema.json)
(JSON Schema draft 2020-12, `$id` `urn:hmi-studio:project:schema:v1`).
It is generated from the Rust types (`cargo run --example export_schema` inside
`crates/project-model`) and a test fails if the committed copy is stale.

The JSON Schema describes **structure**. Cross-references, per-kind property
types, hierarchy rules, binding compatibility and the other semantic rules are
enforced by the validator — see [PROJECT_MODEL_VALIDATION.md](PROJECT_MODEL_VALIDATION.md).

Examples: [`crates/project-model/fixtures/valid/`](../crates/project-model/fixtures/valid/)
(`minimal.hmi.json`, `bottling-line.hmi.json`).

## Conventions

| Topic | Rule |
| --- | --- |
| Encoding | UTF-8 JSON, single root object. Recommended extension: `.hmi.json`. |
| Field names | `camelCase`. Unknown fields are **errors** (never ignored). |
| Identifiers | Strings matching `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`, unique across the whole document. |
| Tagged unions | Discriminator `type` (elements, actions, transforms, alarm conditions), `mode` (sizes, grid tracks) or `kind` (property types). Externally tagged where noted (`dataType`, binding `source`, action `target`). |
| Lengths | Density-independent pixels (dp). Font sizes in sp. |
| Optional values | Omitted when unset. Writers never emit `null` for model fields; free-form values (`extensions`, `initialValue`, property values) may contain `null`. |
| Numbers | Must be finite. Integers within the 64-bit signed/unsigned range are preserved exactly (not converted to floating point). |
| Order | Arrays are ordered and significant (children = paint order, rules = priority, actions = execution order). Map keys are unordered; writers sort them. |

## Canonical form

`hmi_project_model::to_json_string` produces the canonical text:

- two-space indentation, `\n` line endings, trailing newline;
- object fields in the order listed below; map keys sorted (byte order);
- empty arrays/maps, unset options and default values omitted;
- shortest round-trip number formatting (`1`, `0.30000000000000004`).

Serializing a model twice yields identical bytes, and
`serialize(parse(serialize(p))) == serialize(p)`. Readers must accept any key order
and the explicit form of defaults.

## Versioning

- `format` must be exactly `"hmi-studio.project"`.
- `schemaVersion` is a non-negative integer. This build reads and writes `1`.
- Newer versions are rejected (`UnsupportedVersion`, "saved by a newer HMI Studio");
  older versions are upgraded only by registered migrations, otherwise rejected.
  Readers must never guess.
- Additive, optional fields may be introduced within a version only if old readers
  can safely reject them (they will, because unknown fields are errors); in practice
  any new field means a new `schemaVersion` plus a migration.
- Tool- or vendor-specific data goes in `extensions` (namespaced keys), which every
  version preserves.

## Root object

| Field | Type | Req. | Notes |
| --- | --- | --- | --- |
| `format` | `"hmi-studio.project"` | ✓ | |
| `schemaVersion` | integer | ✓ | `1` |
| `id` | id | ✓ | Stable project id (UUID text recommended). |
| `metadata` | Metadata | ✓ | |
| `settings` | Settings | | `defaultTheme`, `defaultDeviceProfile` |
| `navigation` | Navigation | ✓ | |
| `deviceProfiles` | DeviceProfile[] | | |
| `breakpoints` | Breakpoint[] | | |
| `themes` | Theme[] | | |
| `styles` | Style[] | | |
| `resources` | Resource[] | | |
| `localization` | Localization | ✓ | |
| `units` | EngineeringUnit[] | | |
| `dataTypes` | StructType[] | | |
| `connections` | Connection[] | | |
| `tags` | Tag[] | | |
| `alarms` | AlarmDefinition[] | | |
| `recipes` | RecipeDefinition[] | | |
| `components` | ComponentDefinition[] | | Reusable definitions |
| `screens` | Screen[] | ✓ | At least one |
| `extensions` | object | | Free-form |

**Metadata**: `name` (required), `description`, `author`, `revision`, `created`,
`modified` (RFC 3339 text), `keywords[]`.

**Navigation**: `startScreen` (page id), `items[]` of `{ id, label: Text, screen, icon? }`.

**DeviceProfile**: `id`, `name`, `platform` (`any`|`windows`|`linux`|`android`),
`widthPx`, `heightPx`, `density` (px per dp), `orientation` (`landscape`|`portrait`), `touch`.

**Breakpoint**: `id`, `name`, `minWidth` (inclusive), `maxWidth` (exclusive, optional), in dp.

**Screen**: `id`, `name`, `kind` (`page` default | `popup`), `title?: Text`,
`designProfile?`, `events[]` (`show`/`hide` only), `root: Node`, `extensions`.

## Text, colors, resources

- **Text** (operator-visible): `"literal"` or `{ "textKey": "nav.overview" }`.
- **Color** property values: `"#RRGGBB"`, `"#RRGGBBAA"` or `{ "token": "primary" }`.
- **Resource** property values: a resource id string.

## Node

```json
{
  "id": "n-speed",
  "name": "Speed input",
  "element": { "type": "component", "kind": "numericInput" },
  "layout": { "width": { "mode": "fixed", "value": 200 }, "weight": 1 },
  "children": [],
  "style": "style-value",
  "properties": { "min": 0, "max": 600, "decimals": 0 },
  "bindings": [ { "property": "value", "source": { "tag": "tag-speed-sp" }, "mode": "readWrite" } ],
  "events": [ { "event": "valueChanged", "actions": [ { "type": "back" } ] } ],
  "responsive": [ { "when": { "breakpoint": "bp-compact" }, "set": { "width": { "mode": "fill" } } } ],
  "extensions": {}
}
```

### `element`

| `type` | Fields | Children |
| --- | --- | --- |
| `row`, `column` | `arrangement` (`start`, `center`, `end`, `spaceBetween`, `spaceAround`, `spaceEvenly`), `alignment` (`start`, `center`, `end`), `spacing`, `wrap` | any |
| `box` | `contentAlignment` (`topStart` … `bottomEnd`, `center`) | any |
| `grid` | `columns: Track[]`, `rows: Track[]` (≥ 1 each), `columnGap`, `rowGap` | any; each needs `layout.gridCell` |
| `scroll` | `direction` (`vertical`, `horizontal`, `both`) | exactly 1 |
| `absolute` | — | any; each needs `layout.position` |
| `component` | `kind` (see catalog) | `container`: ≤ 1, others: 0 |
| `instance` | `definition`, `parameters: { name: value }` | 0 |

**Track**: `{ "mode": "fixed", "value": 120 }`, `{ "mode": "auto" }`, `{ "mode": "fraction", "weight": 1 }`.

### `layout`

| Field | Type | Valid when |
| --- | --- | --- |
| `width`, `height` | Size | always |
| `minWidth`, `maxWidth`, `minHeight`, `maxHeight` | number ≥ 0 | always; min ≤ max; fixed size within range |
| `aspectRatio` | number > 0 | always |
| `padding` | `{ top, end, bottom, start }` ≥ 0 | always |
| `weight` | number > 0 | parent is `row`/`column` |
| `crossAlign` | `start`/`center`/`end` | parent is `row`/`column` |
| `align` | Alignment | parent is `box` |
| `position` | `{ x, y }` | parent is `absolute` (required there) |
| `gridCell` | `{ column, row, columnSpan?, rowSpan? }` zero-based | parent is `grid` (required there) |
| `visibility` | `visible` (default), `hidden`, `collapsed` | always |
| `clip` | bool | always |
| `zIndex` | integer | always |

**Size**: `{ "mode": "auto" }` (default), `{ "mode": "fill", "fraction": 0.5 }`
(fraction in (0, 1], default 1), `{ "mode": "fixed", "value": 48 }`.

### `responsive[]`

`{ "when": Condition, "set": Override }`, applied in order (last match wins).

- **Condition** (≥ 1 field; all must match): `breakpoint`, `minWidth`, `maxWidth`,
  `orientation`, `platform`, `deviceProfile`.
- **Override** (≥ 1 field): `width`, `height`, min/max constraints, `padding`, `weight`,
  `visibility`, `spacing` and `direction` (`horizontal`|`vertical`; row/column only),
  `columns` (grid only), `gridCell`, `position`, `zIndex`.

### `bindings[]`

| Field | Notes |
| --- | --- |
| `property` | Target property (or value parameter name on instances). |
| `source` | `{ "tag": id }`, `{ "tagParameter": name }` (definitions only), `{ "parameter": name }` (definitions only, read-only). |
| `member` | Path into struct/array tags: `status.running`, `codes[3]`, `axes[0].pos`. |
| `mode` | `read` (default), `write`, `readWrite`. |
| `transform` | `{ "type": "linear", "factor": f, "offset": o }`, `{ "type": "invert" }`, `{ "type": "bit", "index": n }`, `{ "type": "map", "entries": [ { "when": v, "value": p } ], "default": p }`. |

### `events[]`

`{ "event": "click" | "press" | "release" | "longPress" | "valueChanged" | "show" | "hide", "confirm"?: Text, "actions": Action[] }`

**Actions** (`type`): `navigate {screen}`, `back {}`, `openPopup {screen}`,
`closePopup {}`, `writeTag {target, member?, value}`, `toggleTag {target, member?}`,
`stepTag {target, member?, step, min?, max?}`, `pulseTag {target, member?, durationMs}`,
`acknowledgeAlarms {alarm? | group?}`, `loadRecipe {recipe, set?}`,
`saveRecipe {recipe, set?}`, `setLocale {locale}`.
`target` is `{ "tag": id }` or `{ "tagParameter": name }`.

## Component definitions

```json
{
  "id": "cmp-motor-faceplate",
  "name": "Motor faceplate",
  "category": "Drives",
  "parameters": [
    { "name": "motor", "type": { "kind": "tag", "dataType": { "struct": "dt-motor" }, "writable": true } },
    { "name": "title", "type": { "kind": "text" }, "default": "Motor" }
  ],
  "root": { "id": "n-mf-root", "element": { "type": "component", "kind": "container" } }
}
```

**PropertyType** (`kind`): `bool`, `number {min?, max?}`, `integer {min?, max?}`,
`string`, `text`, `color`, `resource {kinds?}`, `enum {options}`,
`tag {dataType?, writable?}` (definitions only; no default allowed).

## Styles, themes, resources

- **Theme**: `id`, `name`, `dark`, `colors {name: hex}`, `typography {name: { fontFamily?, font?, size, weight (100–900, default 400), italic, lineHeight? }}`, `metrics {name: number}`.
- **Style**: `id`, `name`, `target?` (component kind), `properties {…}` (validated like node properties).
- **Resource**: `id`, `kind` (`image`, `vector`, `font`, `data`), `path` (relative, `/`-separated, no `..`, `:` or `\`), `mediaType?`, `sha256?` (64 lowercase hex), `extensions`.

## Industrial sections

- **DataType**: `"bool"`, `"int8"`…`"int64"`, `"uint8"`…`"uint64"`, `"float32"`,
  `"float64"`, `"string"`, `"dateTime"`, `{ "array": { "element": DataType, "length": n } }`,
  `{ "struct": dataTypeId }`.
- **StructType**: `id`, `name`, `members[] { name, dataType, description? }`.
- **Connection**: `id`, `name`, `driver` (`simulator`, `opcUa`, `siemensS7`, `modbusTcp`, `custom`), `endpoint?`, `parameters {}` (non-secret), `credentialRef?`, `extensions`.
- **Tag**: `id`, `name`, `dataType`, `connection?`, `address?` (required iff `connection`), `access` (`read` default, `write`, `readWrite`), `unit?`, `range? {min, max}`, `initialValue?`, `scanRateMs?`, `description?`, `extensions`.
- **EngineeringUnit**: `id`, `symbol`, `name?: Text`, `quantity?`.
- **AlarmDefinition**: `id`, `name`, `tag`, `member?`, `condition` (`high {limit}`, `low {limit}`, `equals {value}`, `bitSet {index}`, `isTrue {}`, `isFalse {}`), `deadband?`, `delayMs?`, `severity` (`info`, `warning`, `alarm` default, `critical`), `priority` (1–1000, default 500), `message: Text`, `group?`, `acknowledge` (default true), `latched`.
- **RecipeDefinition**: `id`, `name: Text`, `description?`, `fields[] { id, name: Text, tag, member? }`, `sets[] { id, name: Text, values { fieldId: value } }`.
- **Localization**: `defaultLocale`, `locales[]`, `strings { key: { locale: text } }`.

## Known schema limitations

- `pattern`-level checks (ids) are expressed in the schema, but references, uniqueness
  and per-kind property typing are not expressible in plain JSON Schema and are left
  to the validator.
- `properties`, `parameters`, `initialValue`, map `when/value` and `extensions` are
  typed as free JSON in the schema.
- The schema uses `oneOf` for tagged unions; generators that do not understand
  `oneOf` + `const` discriminators need a custom mapping (see `PROJECT_MODEL.md`).
