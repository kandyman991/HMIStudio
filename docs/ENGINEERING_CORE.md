# Engineering core (RUST-03B)

Status: **candidate** — implemented in `crates/engineering-core/` (`hmi-engineering-core`),
not yet wired into the Tauri shell. Stacked on RUST-03A (PR #2) at verified head
`ca60316e163eb0441e7d2eb9ccb3be2900d0dcfd`; see [Dependency](#dependency-on-rust-03a).

Related: [ENGINEERING_COMMANDS.md](ENGINEERING_COMMANDS.md),
[PROJECT_PERSISTENCE.md](PROJECT_PERSISTENCE.md),
[ENGINEERING_CORE_VALIDATION.md](ENGINEERING_CORE_VALIDATION.md),
[PROJECT_MODEL.md](PROJECT_MODEL.md).

## Purpose

The engineering core is the single authority over an open HMI project. The
graphical editor, tests and (later, optionally) AI agents all change a project
the same way: by submitting typed `Command`s, grouped into atomic
`Transaction`s, to a `ProjectSession`. Nothing else mutates the project. This
satisfies the repository rules that UI and optional AI use the same validated
engineering operations and that every project stays editable without AI.

```
 React UI ──(future Tauri IPC)──▶ protocol::dispatch(Request) ──▶ ProjectSession
     ▲                                                              │ owns Arc<Project>
     └──────── Response / ChangeNotification / ProjectSnapshot ◀────┘
                                                                    │
                                     persistence (open/save/autosave/recovery)
                                                                    │
                                                    fs::FileSystem (StdFileSystem)
```

The crate depends only on `hmi-project-model` (path), serde, serde_json and
thiserror. No Tauri, React, Kotlin, drivers, network or shell execution.

## Module map

| Module | Responsibility |
| --- | --- |
| `session` | `ProjectSession`: state ownership, revisions, dirty state, transactions, validation gate, undo/redo, grouping, status, snapshots. |
| `command` | `Command` vocabulary (34 operations), `Transaction`, `ValidationPolicy`. |
| `apply` (private) | Applies one command to a working copy and records changes / allocated ids. |
| `ids` | Id collection, deterministic allocation with session-wide retirement, subtree id reassignment and reference remapping. |
| `locate` | Finds nodes across screen and component-definition trees. |
| `change` | `ChangeSet` (added / modified / moved / removed, per entity). |
| `error` | `CommandError`, `TransactionError` (serializable). |
| `persistence` | Open, save, save-as, atomic replacement, backups, autosave, recovery, external modification, reload, close, migration on open. |
| `fs` | Narrow `FileSystem` trait, `StdFileSystem`, `FaultyFileSystem` for fault injection. |
| `protocol` | Serializable `Request` / `Response` / `ChangeNotification` and `dispatch`. |

## State model

- The session holds the project as an immutable `Arc<Project>` together with its
  cached `ValidationReport` and a **state id**.
- **Revision**: `u64`, starts at 0, +1 on every committed transaction, undo,
  redo and reload. Never reused. Clients pass `expectedRevision` for optimistic
  concurrency; a mismatch is `StaleRevision` and changes nothing.
- **Dirty**: the current state id differs from the state id last written to (or
  read from) the project file. Undo back to the saved state is clean again; a new
  edit that happens to reproduce saved content is a new state and is dirty.
- **Lifecycle**: new (untitled, clean) → opened / recovered → edited → saved →
  closed. Closing refuses unsaved changes unless forced. A closed session rejects
  every mutation.

## Transactions

1. Reject if closed, empty or stale.
2. Clone the current project into a private working copy.
3. Apply commands in order. Each command gets a fresh deterministic id allocator
   built from the working copy plus every id ever seen in the session.
4. If the working copy equals the current project, return a no-op outcome
   (no revision change, no undo step).
5. Validate the working copy (RUST-03A validator). Policy:
   - `rejectNewErrors` (default): reject if any error appears that the current
     state did not already have. On a valid project this equals strict; on an
     invalid project (opened as-is) it allows incremental repair.
   - `strict`: reject unless the result has no errors.
6. Commit: swap the snapshot, bump revision, clear redo, push or merge the undo
   entry, retire the new ids.

Any failure in steps 1–5 drops the working copy, so a failed transaction never
leaves a partially modified project. Reference integrity (deleting a referenced
tag, screen, style or definition; dangling bindings, cycles, invalid hierarchy or
layout) is enforced by the same validation gate, so commands and the validator can
never disagree.

## Undo/redo

- One undo step per committed transaction (multi-command transactions are one step).
- **Grouping**: consecutive transactions with the same `mergeKey` (for example
  every update of one slider drag) merge into the previous step. Merging stops at
  any transaction without that key, at undo/redo and at save.
- **Redo invalidation**: any new commit clears the redo stack; failed transactions
  do not.
- **Limits**: `SessionConfig.history_limit` (default 200; 0 disables undo); oldest
  steps are dropped first.
- **Exact restoration**: steps store complete before/after snapshots (shared
  `Arc`s, so consecutive steps share memory). Undo restores identifiers and
  references bit-for-bit; tests compare canonical JSON.
- **No id reuse**: ids allocated in the session are retired, so an undone object's
  id is never given to a different object later.

## Frontend integration contract

`protocol.rs` defines the IPC-ready contract (`PROTOCOL_VERSION = 1`):

| Request (`request`) | Response (`response`) | Notification |
| --- | --- | --- |
| `snapshot` | `snapshot` (revision, project, diagnostics, status) | — |
| `status` | `status` | — |
| `validate` | `diagnostics` | — |
| `execute {transaction}` | `changed {outcome}` or `error` | on change |
| `undo` / `redo {expectedRevision?}` | `changed` / `nothing` / `error` | on change |
| `save {options}` / `saveAs {path, options}` | `saved {outcome, status}` / `error` | — |
| `autosave` | `autosaved {outcome, status}` / `error` | — |
| `checkExternal` | `external {external}` | — |
| `reload {force}` | `changed` / `error` | on change |
| `close {force}` | `closed` / `error` | — |

`TransactionOutcome` carries previous/new revision, `changed`, cause, undo label,
change set, allocated ids, merge flag, current diagnostics and `SessionStatus`
(revision, dirty, can undo/redo with labels, path, error/warning counts, migrated
from, autosave freshness, closed). Errors are `{ code, message, details }` where
`details` is the serialized error enum (`kind` discriminator).

Requests reject unknown fields and unknown request names; there is no generic
filesystem, shell or JSON-patch request.

### Planned Tauri exposure (later integration milestone — not implemented here)

1. `src-tauri` depends on `hmi-engineering-core` by path (or via a root workspace).
2. Managed state: `Mutex<Option<ProjectSession>>` plus `MigrationRegistry::builtin()`.
3. One typed command `engineering_request(request: Request) -> Response`, calling
   `protocol::dispatch`; emit `ChangeNotification` as a Tauri event
   (`project://changed`) to all windows.
4. Host-level commands, not part of `Request`: `project_new`, `project_open(path)`,
   `project_recover(path)`, `project_discard_recovery(path)`; paths come from the
   native dialog plugin, never from free text in the webview.
5. Timer in Rust (not JS) calls `autosave` every N seconds while dirty; window-close
   handler calls `close(false)` and prompts on `unsavedChanges`.
6. The existing in-memory demo `Engine` in `src-tauri/src/main.rs` (PR #1) is
   replaced; PR #1's capability file must not gain filesystem permissions for the
   webview.

## Architectural decisions

| ID (proposed) | Decision | Alternatives considered |
| --- | --- | --- |
| 03B-1 | Snapshot-based transactions and undo (clone → apply → validate → swap). | Inverse-command undo: smaller memory, but every command needs a correct inverse and partial failure handling; rejected for this milestone. |
| 03B-2 | Validation gate is the RUST-03A validator, policy `rejectNewErrors` by default. | Strict-only: blocks repair of invalid projects that must open unrepaired. Per-command integrity checks only: duplicates validator logic. |
| 03B-3 | Revision (monotonic) separate from state id (content identity). | Revision-only dirty tracking breaks clean-after-undo. |
| 03B-4 | Session-wide id retirement for allocation. | Allocating from current ids only: undo + new insert could reuse an id for a different object. |
| 03B-5 | Engine-owned persistence with a narrow `FileSystem` trait. | UI-driven file writes: violates "no privileged filesystem bridge". |
| 03B-6 | Sidecar directory `<file>.studio/` for backups and autosave. | OS temp/app-data dirs: harder to find, lost on reinstall; still available for untitled projects via `untitled_recovery_dir`. |

These are recorded here because the shared decision register
(`docs/ENGINEERING_DECISIONS.md`) is owned by PR #1 until it merges; they should be
copied there (as D-009…) during integration. No rejected approach (F-001, F-002) is
revived.

## Dependency on RUST-03A

PR #2 is not merged. This branch is stacked on its verified head
`ca60316e163eb0441e7d2eb9ccb3be2900d0dcfd` (Project model CI succeeded on
ubuntu-24.04 and windows-latest for that SHA). If PR #2 changes before merging,
rebase this branch onto the new head and re-run both crates' tests. The PR targets
`main`, so its diff also shows PR #2's commits until PR #2 merges.

## Limitations

- Memory: each undo step keeps one full project snapshot (shared between
  neighbours). Large projects with long histories cost memory; structural sharing
  or inverse commands are future work.
- Validation runs on the whole project per transaction (linear in project size).
  Fine for tested sizes (5,000+ nodes); incremental validation is future work.
- `rejectNewErrors` compares diagnostics by code, path and message; on an already
  invalid project, edits that shift JSON paths of existing errors can be reported
  as "new" and rejected (conservative).
- Node lookup is a linear search; with duplicate node ids (only possible in an
  invalid project) commands address the first match.
- Commands cover screens, nodes, properties, styles, layout, responsive rules,
  bindings, events, tags, component definitions, metadata, settings, navigation and
  extensions. Themes, resources, units, data types, connections, alarms, recipes
  and localization have no dedicated commands yet (they load, save and undo as
  part of the project, but cannot be edited through commands).
- Screen-level event handlers have no dedicated command yet.
- No multi-user/real-time collaboration; optimistic revision checks only protect a
  single session's clients.
- No file locking: concurrent editors are detected (external modification) but not
  prevented.
- Not wired into Tauri; no UI. Manual desktop acceptance is not applicable to this
  milestone (D-008: RUST-03 uses automated engineering tests).

## Next exact action

1. Independent review of this PR; verify exact-head CI for both crates on Ubuntu and
   Windows.
2. Merge PR #1, then PR #2 (rebase this branch if PR #2's head changes), then this PR.
3. On integration, record decisions 03B-1…03B-6 in `docs/ENGINEERING_DECISIONS.md`,
   update `CHAT_HANDOFF.md`, `ROADMAP.md` and `docs/MILESTONE_PLAN.md` (owned by PR #1
   until merged).
4. Start the Tauri integration milestone described above (RUST-02/RUST-04 boundary):
   replace the demo engine with `ProjectSession` via `protocol::dispatch`.
