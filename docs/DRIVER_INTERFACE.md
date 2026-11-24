# Communication driver interface (RUST-06A)

Defined in `crates/tag-engine/src/driver.rs`. Only the simulator implements it in
RUST-06A ([PLC_SIMULATION.md](PLC_SIMULATION.md)); this document is the contract for
future Siemens S7, S7CommPlus, OPC UA, Modbus TCP and other drivers.

## Trait

```rust
pub trait Driver: Send {
    fn capabilities(&self) -> DriverCapabilities;
    fn connect(&mut self, ctx: &OpContext) -> Result<(), DriverError>;
    fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
    fn read(&mut self, items: &[ReadItem], ctx: &OpContext)
        -> Result<Vec<ItemResult<TagValue>>, DriverError>;
    fn write(&mut self, items: &[WriteItem], ctx: &OpContext)
        -> Result<Vec<ItemResult<()>>, DriverError>;
}

pub trait DriverFactory: Send + Sync {
    fn create(&self, connection: &Connection, tags: &[&Tag], types: &TypeContext)
        -> Result<Box<dyn Driver>, DriverError>;
}
```

`DriverRegistry::register(DriverKind, Arc<dyn DriverFactory>)` maps project
`DriverKind`s to factories. Connections whose kind has no factory start as
`unavailable` and their tags have quality `bad/configError`.

## Contract

| Rule | Detail |
| --- | --- |
| Serialization | The engine holds each driver behind a mutex, so a driver never receives concurrent calls and may use plain mutable state. |
| Grouped operations | `read`/`write` take item lists. The engine never exceeds `max_items_per_read` / `max_items_per_write`. Individual operations are groups of one. |
| Results | `Err` = request-level failure (connection lost, timeout, cancellation, protocol). `Ok` must contain **exactly one result per item, in order**; a count mismatch is treated as a protocol error. |
| Reads | Always return the whole tag value, of exactly the tag's `DataType` (checked by the engine; a mismatch gives `bad/typeMismatch`). |
| Writes | `WriteItem.value` is already coerced to the member type and range-checked. Member writes are issued only if `member_writes` is advertised. |
| Timeouts | Every blocking call must finish within `ctx.timeout` and return `timeout` otherwise. |
| Cancellation | Long waits must observe `ctx.cancel` (`CancelToken::wait`) and return `cancelled` promptly; shutdown depends on it. |
| Connection state | `is_connected` must report drops detected by any operation; errors of kind `notConnected`/`connectionLost` make the engine mark the connection faulted. |
| Credentials | Never read from `parameters`; resolve `connection.credential_ref` through the host's secure store (future host API). |

## Types

- **`DriverCapabilities`**: `driver`, `grouped_read`, `grouped_write`,
  `max_items_per_read`, `max_items_per_write`, `member_writes`, `arrays`, `structs`,
  `min_scan_interval_ms`, `physical`.
- **`DriverError { kind, message }`**, where `kind` is one of: `notConnected`,
  `connectFailed`, `connectionLost`, `timeout`, `cancelled`, `addressInvalid`,
  `typeMismatch`, `accessDenied`, `unsupported`, `protocol`.
- **`ConnectionStatus`**: `disconnected`, `connecting`, `connected`,
  `faulted {error}` (retried), `unavailable {error}` (configuration; never retried),
  `stopped`.
- **`ReadItem { tag, address, data_type }`** and
  **`WriteItem { tag, address, data_type, member, value }`**.

## Engine behavior around drivers

| Situation | Engine reaction |
| --- | --- |
| Not connected and the reconnect delay has elapsed (or `poll_all`) | `connecting` → `connect` → `connected` (all groups become due) or `faulted` + tags `bad/connectionLost`. |
| Request `Err` with connection loss | Driver `disconnect`, status `faulted`, all readable tags of the connection `bad/connectionLost`, reconnect after `reconnectMs`. |
| Request `Err` timeout / protocol | Only that request's tags go bad (`timeout` / `communicationError`); the connection stays up. |
| Item error | That tag goes bad, with a reason mapped from the kind (`addressInvalid` → `configError`, `typeMismatch` → `typeMismatch`, …). |
| Device value outside `range` | Accepted, `uncertain/outOfRange`. |
| Write while disconnected | Fails immediately with `notConnected` (no queuing, see decision 06A-5). |
| Shutdown | Cancellation token fires, threads joined, `disconnect`, status `stopped`. |

## Scan groups and polling

- Readable device tags are grouped by effective interval: `scanRateMs` if > 0,
  otherwise the connection's `defaultScanMs` parameter, otherwise
  `EngineConfig.default_scan_ms` (1000). The interval is clamped to the driver's
  `min_scan_interval_ms`.
- A due group is read in chunks of `max_items_per_read`. `next_due = now + interval`
  (no catch-up bursts after stalls).
- Connection parameters understood by the engine: `timeoutMs` (default 2000),
  `reconnectMs` (2000), `defaultScanMs` (1000), all non-negative integers. Invalid
  values make the connection `unavailable`.
- `ScanMode::Threaded`: one thread per connection named `tag-scan-<id>`; it sleeps
  until the next due group or reconnect, capped by `max_idle_ms`, and wakes on
  cancellation. `ScanMode::Manual`: the caller invokes `poll_due` / `poll_all`.

## Adding a driver (checklist for future milestones)

1. Create a separate crate (e.g. `crates/driver-opcua`) depending on `hmi-tag-engine`.
   Review licenses before adding protocol libraries (D-003, issue #4 process).
2. Implement `Driver` and `DriverFactory`. Validate addresses in `create` where
   possible (returning `addressInvalid` item errors otherwise).
3. Map protocol status codes to `DriverErrorKind`. Honor timeouts and cancellation.
4. Port the engine-level behavior tests from `tests/simulation.rs` against a protocol
   test server (simulation first); keep physical hardware acceptance separate (D-008).
5. Never weaken the engine's access or range checks to make a device work.
