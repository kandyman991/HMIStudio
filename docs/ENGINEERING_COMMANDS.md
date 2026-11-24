# Engineering commands (RUST-03B)

All edits go through `Transaction`s of typed `Command`s
(`crates/engineering-core/src/command.rs`). JSON uses `op` as the discriminator,
camelCase fields, and rejects unknown fields and unknown operations.

## Transaction

```json
{
  "commands": [
    { "op": "insertNode", "parent": "n-ov-root", "index": 0,
      "node": { "id": "n-new", "element": { "type": "component", "kind": "text" },
                "properties": { "text": "Hello" } },
      "assignNewIds": true },
    { "op": "setProperty", "node": "n-ov-title", "name": "fontSize", "value": 28 }
  ],
  "expectedRevision": 41,
  "label": "Add greeting",
  "mergeKey": "drag-7",
  "validation": "rejectNewErrors"
}
```

| Field | Meaning |
| --- | --- |
| `commands` | Applied in order to a private copy; all or nothing. Must not be empty. |
| `expectedRevision` | Optional optimistic check; stale → `staleRevision`, nothing applied. |
| `label` | Undo-menu text (default: the op name, or "N changes"). |
| `mergeKey` | Consecutive transactions with the same key form one undo step. |
| `validation` | `rejectNewErrors` (default) or `strict`. |

## Commands

| `op` | Fields | Notes |
| --- | --- | --- |
| `createScreen` | `id?`, `name`, `kind` (`page`/`popup`), `root?`, `index?` | Allocates `scr-N` and an empty fill/fill column root when omitted. |
| `renameScreen` | `screen`, `name` | Id unchanged. |
| `setScreenTitle` | `screen`, `title?` | Absent clears. |
| `deleteScreen` | `screen` | Rejected by validation while referenced (start screen, menu, actions). |
| `duplicateScreen` | `screen`, `name?` | New screen id and node ids; `navigate`/`openPopup` actions that targeted the source inside the copy are redirected to the copy; other references kept. Inserted after the source. |
| `insertNode` | `parent`, `index?`, `node`, `assignNewIds` | Without `assignNewIds`, every id in the subtree must be new (`alreadyExists`). |
| `removeNode` | `node` | Removes the subtree; roots cannot be removed. |
| `duplicateNode` | `node`, `parent?`, `index?` | Fresh ids for the whole subtree; default position right after the source. |
| `moveNode` | `node`, `parent`, `index?` | Reparent and/or reorder, across trees too. Index is in the target's children *after* removal. Rejects moving into itself or a descendant (`circularHierarchy`) and moving roots. |
| `reorderNode` | `node`, `index` | Within the current parent. |
| `renameNode` | `node`, `name?` | Designer layer name. |
| `setElement` | `node`, `element` | E.g. row spacing, grid tracks, instance parameters. |
| `setProperty` | `node`, `name`, `value?` | Absent value removes (must exist). Typed by the RUST-03A catalog via validation. |
| `setNodeStyle` | `node`, `style?` | |
| `setLayout` | `node`, `layout` | Replaces all layout parameters. |
| `setResponsive` | `node`, `rules` | Replaces responsive rules. |
| `setBinding` | `node`, `binding` | Adds or replaces the binding with the same `property`. |
| `removeBinding` | `node`, `property` | |
| `setEventHandler` | `node`, `handler` | Adds or replaces the handler for the same `event`. |
| `removeEventHandler` | `node`, `event` | |
| `createTag` | `tag`, `index?` | Id must be new. |
| `updateTag` | `tag` | Replaces the tag with the same id. |
| `deleteTag` | `tag` | Rejected by validation while referenced. |
| `upsertStyle` | `style` | Adds or replaces by id. |
| `deleteStyle` | `style` | Rejected while referenced. |
| `createComponentDefinition` | `definition` | All ids must be new. |
| `updateComponentDefinitionInfo` | `definition`, `name`, `description?`, `category?` | |
| `setComponentParameters` | `definition`, `parameters` | Instances are re-validated (missing required parameters are rejected). |
| `deleteComponentDefinition` | `definition` | Rejected while instances exist. |
| `duplicateComponentDefinition` | `definition`, `name?` | New definition id and node ids; self-references remapped. |
| `setMetadata` | `metadata` | |
| `setSettings` | `settings` | |
| `setNavigation` | `navigation` | |
| `setExtension` | `key`, `value?` | Absent value removes (must exist). |

Nodes inside reusable component definitions are edited with the same node
commands (node ids are project-wide unique).

## Identifier allocation

- Allocation is deterministic: `<prefix>-<n>` with the smallest unused `n`
  (RUST-03A `IdAllocator`), where the prefix is the source id (duplicates) or
  `scr` (new screens). Example: duplicating `n-hello` yields `n-hello-1`.
- Ids already seen anywhere in the session (including undone states) are never
  reissued.
- Every allocated id is reported in `TransactionOutcome.allocatedIds`, keyed
  `"<command index>:<key>"`: `screen`, `root`, `definition`, or
  `node:<source node id>`. Example: `{"0:node:n-hello": "n-hello-1"}`.
- Later commands in the same transaction may address allocated ids (allocation
  is deterministic, so clients can also predict them; the outcome is authoritative).

## Errors

`TransactionError` (`kind`):

| `kind` | When |
| --- | --- |
| `empty` | No commands. |
| `staleRevision` | `expectedRevision` ≠ current revision. |
| `command` | `{ index, op, error }` with `CommandError`: `notFound`, `alreadyExists`, `rootNode`, `circularHierarchy`, `indexOutOfRange`, `invalidArgument`. |
| `validation` | `{ errors }`: the RUST-03A diagnostics the result would introduce (or all errors under `strict`). |
| `closed` | Session closed. |

The project, revision and history are unchanged after every error.

## Change sets

`changes: [{ kind, entity, id? }]`, deduplicated, in first-occurrence order.
`kind`: `added`, `modified`, `moved`, `removed`. `entity`: `metadata`, `settings`,
`navigation`, `extensions`, `screen`, `node`, `tag`, `style`,
`componentDefinition`, `project`. Undo reports the inverted set (added ↔ removed);
reload reports `{modified, project}`. Views should treat change sets as hints and
fetch a snapshot when in doubt.
