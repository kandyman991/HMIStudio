# PLC simulation (RUST-06A)

`crates/tag-engine/src/simulator.rs`. An in-memory simulated PLC for development,
tests and (later) desktop simulation. It never touches hardware or the network.
Reads, writes, commands and setpoints against it need no production approval
(D-004, [DEVELOPMENT_SIMULATION_POLICY.md](DEVELOPMENT_SIMULATION_POLICY.md)).
Simulated connections are clearly distinguished: their driver capabilities report
`physical: false`.

## Components

| Type | Role |
| --- | --- |
| `SimulatorHub` | One `SimPlc` per simulator connection id; `factory()` for the driver registry. |
| `SimPlc` | Shared handle to one PLC's state: memory, reachability, latency, faults, counters. |
| `SimulatedDriver` | `Driver` implementation attached to a `SimPlc`. |
| `SimStep` | One scenario step; `SimPlc::apply(&[SimStep])` replays steps in order. |

```rust
let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(start));
let hub = SimulatorHub::new(clock.clone());
let (engine, report) = TagEngine::new(&project, &simulator_registry(&hub), clock, config);
let plc = hub.plc("conn-sim");
plc.set("DB1.Speed", TagValue::Float32(300.0));   // PLC-side change
plc.set_online(false);                            // connection loss
plc.apply(&[SimStep::FailNext { kind: DriverErrorKind::Protocol, count: 1 }]);
```

## Memory and addresses

- Any non-empty address string is a memory cell (e.g. `DB1.Motor`); its value has the
  tag's full data type (scalar, array or struct).
- **Seeding** at driver creation, in project order: `initialValue` if present;
  otherwise the type default, moved to `range.min` (rounded up for integers) when the
  default lies outside the engineering range. Existing cells are kept, so several
  engines can share one hub.
- Unknown addresses are created on first access with the type default, unless the
  connection sets `"strictAddresses": true` (then `addressInvalid`).
- A value of the wrong type in a cell (e.g. a test writes a string) is reported as a
  `typeMismatch` item error.
- Member writes (`codes[2]`, `running`) are applied atomically under the PLC lock.

## Generators (read-only, deterministic)

| Address | Value at time t |
| --- | --- |
| `sine:offset=O,amplitude=A,period=P` | `O + A·sin(2π·phase)` |
| `ramp:min=a,max=b,period=P` | `a + (b − a)·phase` |
| `square:period=P` | `phase < 0.5` (bool tags) |

`phase = (t mod P) / P`, computed in integer microseconds from the injected clock.
With `ManualClock` the values are fully deterministic. Periods accept `10s`, `500ms` or
plain seconds. Integer tags get the nearest integer. Writes return `accessDenied`.
The RUST-03A industrial fixture's `sine:offset=1.5,amplitude=0.5,period=10s` address
works unchanged.

## Faults and timing

| Control | Effect |
| --- | --- |
| `set_online(false)` / `SimStep::Online(false)` | Existing connections are lost (`connectionLost` on the next operation); new connects fail (`connectFailed`). |
| `set_online(true)` | Reconnects succeed; the engine reconnects after `reconnectMs`. |
| `set_latency(d)` / `"latencyMs"` parameter | Every operation waits `d`. If `d` exceeds the operation timeout, it waits for the timeout and returns `timeout`. The wait is interruptible by cancellation (`cancelled`). |
| `SimStep::FailNext { kind, count }` | The next `count` operations (connect/read/write) fail with `kind`. |
| `SimStep::FailAddress { address, kind }` | Item-level error for one address until cleared with `kind: None`. |
| `stats()` | `connects`, `readRequests`, `itemsRead`, `writeRequests`, `itemsWritten`, `failedRequests`. |

Capabilities: grouped reads and writes, 64 items per request, member writes, arrays,
structs, 10 ms minimum scan interval, `physical: false`.

## Deterministic scenarios

Combine `ManualClock`, `ScanMode::Manual` and `SimStep` scripts. The same script
produces byte-identical event logs (`tests/simulation.rs::deterministic_scenarios_are_repeatable`).
Threaded mode with `SystemClock` is for realistic timing, not determinism.

## Limitations

- No PLC program logic (no cyclic user program); values change only through writes,
  scenario steps and generators.
- No protocol emulation (S7/OPC UA wire formats); this is a driver-level simulator.
- Latency is uniform per PLC; there is no jitter model.
- State is in memory only and is not persisted between runs.
