# Tag engine (RUST-06A)

Status: **candidate** — `crates/tag-engine/` (`hmi-tag-engine`), based on `main`
`d17f299` (RUST-03A, RUST-03B and RUST-02A merged). Not integrated with Tauri or any runtime.
Simulation only; no physical PLC connectivity.

Related: [DRIVER_INTERFACE.md](DRIVER_INTERFACE.md), [PLC_SIMULATION.md](PLC_SIMULATION.md),
[TAG_ENGINE_VALIDATION.md](TAG_ENGINE_VALIDATION.md), [PROJECT_MODEL.md](PROJECT_MODEL.md).

## Purpose and boundaries

The tag engine executes the industrial contracts that the RUST-03A project model
declares: tags, data types, connections and bindings. It is framework-independent
(no Tauri, UI or Kotlin types) and is intended to back both the desktop editor's
simulation/preview and, through an equivalent implementation or a future bridge,
the deployed runtime.

It **consumes the project model unchanged**: `Tag`, `DataType`, `StructType`,
`TagAccess`, `ValueRange`, `Connection`, `DriverKind`, `PropertyBinding`,
`BindingSource`, `BindingMode`, `ValueTransform` and the component catalog's
`PropertyType`/`Bindability`. It introduces no serialization format; runtime values
convert to and from the same JSON value form used in project documents
(`initialValue`, recipe values, action values). No change was made to
`crates/project-model` or `crates/engineering-core`.

```
Project (RUST-03A) ──▶ TagEngine::new ──▶ value store (RwLock) ──▶ subscribers (mpsc)
                              │                 ▲
                              ▼                 │ poll / write
                       per-connection runtime: Mutex<Box<dyn Driver>>, scan groups, status
                              │
                       DriverRegistry ──▶ SimulatorFactory ──▶ SimulatedDriver ──▶ SimPlc
                                          (future: S7, S7CommPlus, OPC UA, Modbus TCP)
```

## Public interface (summary)

| Item | Purpose |
| --- | --- |
| `TagEngine::new(&Project, &DriverRegistry, Arc<dyn Clock>, EngineConfig) -> (TagEngine, InitReport)` | Deterministic initialization (project order); configuration problems reported, never fatal. |
| `read`, `read_member`, `read_device` | Cached value (with quality, timestamp, sequence), member of it, or a fresh grouped device read. |
| `write`, `write_json`, `write_member`, `write_many` | Typed writes; access, type and range checks; grouped per connection. |
| `subscribe(Option<&[TagId]>) -> (Subscription, Receiver<TagEvent>)` | Initial snapshot + ordered change events; RAII unsubscribe. |
| `poll_due`, `poll_all` | Drive scan groups (manual mode) or force a refresh. |
| `connection_status`, `capabilities` | Connection state and driver capabilities. |
| `shutdown` (also on `Drop`) | Idempotent, deterministic stop. |
| `binding::compile`, `compile_for_node`, `CompiledBinding::{read, write, evaluate_value, inverse_value}` | Binding evaluation. |
| `TypeContext`, `TagValue`, `MemberPath` | Typed values, strict conversion, member addressing. |
| `simulator_registry(&SimulatorHub)` | Registry with the simulator registered for `DriverKind::Simulator`. |

## Values, types and members

- `TagValue` has one variant per RUST-03A scalar type plus `Array` and `Struct`.
- `TypeContext::value_from_json` mirrors the validator's rules: integers must be exact
  and in range, floats finite (`float32` within ±`f32::MAX`), arrays of exact length,
  structs with exactly their members.
- `coerce` converts between numeric kinds only when lossless in kind (integral and in
  range for integers, finite and in range for floats). It never converts between bool,
  numbers and strings.
- Member paths use the RUST-03A syntax (`status.running`, `codes[3]`, `[0].x`).
  Member writes are atomic read-modify-write updates under the store lock (internal
  tags) or the PLC lock (simulator).

## Access, ranges, quality, timestamps

- HMI reads require `read`/`readWrite`; writes require `write`/`readWrite`
  (`accessDenied` otherwise). Write-only tags are never polled or reported to
  subscribers.
- Engineering `range` (numeric tags): operator writes outside the range are
  rejected before anything is sent. Device values outside the range are accepted with
  quality `uncertain / outOfRange`.
- Quality `{ level: good|uncertain|bad, reason }`. Reasons: `none`, `notConnected`,
  `connectionLost`, `communicationError`, `timeout`, `outOfRange`, `typeMismatch`,
  `notReadable`, `configError`, `shutdown`. Bad values keep the last known value.
- Timestamps are microseconds since the epoch from the injected `Clock`
  (`SystemClock` or `ManualClock`). They are updated on every read confirmation.
  Notifications fire only when value or quality changes.

## Subscriptions

- On subscribe, the receiver gets one `changed` event per subscribed readable tag
  (project order). After that come changes in global `sequence` order, and finally
  `closed` at shutdown, after which the channel ends.
- Unfiltered subscriptions also receive `connection` status events.
- Dropping the `Subscription` or the `Receiver` unsubscribes.
- Channels are unbounded `std::sync::mpsc` (see limitations).

## Concurrency model

`TagEngine` is `Send + Sync`. Fixed lock order: connection driver → connection
status → value store → subscriber table. Updates and their notifications happen under
the store's write lock, which makes delivery order global and lets a subscriber's
initial snapshot be gap-free. Poisoned locks are recovered, because updates are
computed before they are assigned. Driver I/O is serialized per connection; writes
and polls on one connection queue behind each other, and different connections run in
parallel.

## Lifecycle

- **Initialization** (deterministic, project order): tags are indexed (duplicate ids:
  the first one wins). Connections get a driver from the registry, or are
  `unavailable` (no factory, bad parameters). Scan groups are built per effective
  interval. Initial values are applied: `initialValue`, otherwise the type default.
  Initial quality is good for internal tags, `notConnected` for device tags,
  `notReadable` for write-only device tags, and `configError` for problems.
- **Scanning**: `ScanMode::Manual` (the caller polls, fully deterministic) or
  `ScanMode::Threaded` (one thread per connection, interruptible waits).
- **Shutdown**: cancel the token, join the threads, disconnect the drivers (project
  order), set status `stopped`, mark all tags bad/`shutdown`, send `closed`, drop the
  subscribers. It is idempotent, and later calls return `shutDown`.

## Binding evaluation

`compile` resolves the source (`tag`, or `tagParameter` through a `BindingScope`
built from an instance's arguments; value `parameter`s are a renderer concern and
return `notATagBinding`). It then resolves the member type, checks the access rights
for the binding mode, and validates the transform against the source and property
types. These are the same rules as the RUST-03A validator:

| Transform | Read | Write (inverse) |
| --- | --- | --- |
| none | Bool→bool, numeric→number, integer→integer, string→string/text/enum | Input converted to the source type (exact for integers) |
| `linear {factor, offset}` | `x·factor + offset` (integer properties: nearest integer) | `(y − offset)/factor`; integer sources rounded to nearest; `factor = 0` → `notInvertible` |
| `invert` | `!b` | `!b` |
| `bit {index}` | Two's-complement bit of an integer source | `notInvertible` |
| `map {entries, default}` | First numeric-aware match, else default, else `noMapping` | `notInvertible` |

Writes go through `TagEngine::write_member`, so access and range checks apply again.

## Architectural decisions (proposed, RUST-06A)

| ID | Decision | Alternatives / rationale |
| --- | --- | --- |
| 06A-1 | Runtime `TagValue` derived from RUST-03A `DataType`; JSON form only at the edges. | Using `serde_json::Value` at runtime: weak typing, no `f32` fidelity. A new value schema: would compete with the project model. |
| 06A-2 | Fixed lock order; notifications delivered under the store lock. | Delivery outside the lock can reorder events between concurrent writers and race with subscribe. |
| 06A-3 | Synchronous driver trait (`Send`, per-connection mutex, timeout and cancellation), no async runtime dependency. | An async runtime (tokio) adds a large dependency and couples future drivers to it; it can be introduced behind the same trait later. |
| 06A-4 | `Manual` and `Threaded` scan modes over the same step logic. | Threads only: tests become timing-dependent. |
| 06A-5 | Writes fail fast when disconnected; no write queue. | Queued writes could execute stale operator commands after reconnection, which is unsafe for machinery. |
| 06A-6 | Bindings re-check the validator's rules at compile time. | Trusting design-time validation only: a runtime may be handed an unvalidated project. |
| 06A-7 | Member writes require the driver capability; no read-modify-write emulation in the engine. | Emulated RMW on a PLC races with PLC logic. |
| 06A-8 | The simulator seeds memory from `initialValue`, or from `range.min` when the type default is outside the range. | Zero seeding produced out-of-range simulations (see validation regression). |

These belong in `docs/ENGINEERING_DECISIONS.md`, which is being updated concurrently
by the RUST-02 work. They are listed here so the integrator can copy them without
conflicting edits.

## Limitations

- No physical drivers; `opcUa`, `siemensS7`, `modbusTcp` and `custom` connections are
  `unavailable` until a factory is registered.
- Unbounded subscriber channels: a consumer that never reads grows memory. A bounded
  or coalescing delivery policy is needed before UI integration (see validation gaps).
- Device writes are not transactional across items or connections.
- No deadband or change filtering for analog values; every changed value notifies.
- No alarm evaluation, recipe execution, action execution, history/trending or tag
  search (RUST-06 follow-ups).
- `dateTime` values are opaque strings.
- Engine construction takes a whole `Project`; there is no incremental reconfiguration
  when the project changes (it has to be rebuilt).
- The member path parser duplicates the RUST-03A syntax (the model's resolver is
  private). Exposing it publicly from `hmi-project-model` would remove the duplication
  and needs coordination with that crate's owner.
- Threaded mode spawns one OS thread per connection; it has not been sized for
  hundreds of connections.

## Integration requirements (later milestones)

1. **Desktop simulation (with the RUST-02 host):** next to the `ProjectSession`
   host added in RUST-02A (`src-tauri/src/session_host.rs`), a host-owned
   `TagEngine` built from the `ProjectSession` snapshot, using
   `simulator_registry`. Rebuild it on project changes that affect tags or connections.
   Expose typed Tauri commands for read, write and subscribe. Forward `TagEvent`s as
   events with bounded/coalesced delivery. Never expose driver registration or
   addresses as free-form commands from the webview.
2. **Runtime parity (RUST-05/07):** the Kotlin runtime must follow the same semantics
   (quality, range, access, transforms, inverse rounding). Use the tests in
   `crates/tag-engine/tests/` as the behavioral specification and add a cross-language
   golden suite.
3. **Physical drivers (later RUST-06 milestones):** implement `Driver` +
   `DriverFactory` per protocol in separate crates; hardware acceptance at explicitly
   defined hardware milestones (D-008). Credentials go through `credentialRef`, never
   project parameters.
4. **Alarms and recipes:** subscribe to the engine. Alarm conditions use the same
   member paths and types. Recipe downloads use `write_many`.
