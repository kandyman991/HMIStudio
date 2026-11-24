# REUSE-01 — Rust engine integration

Status: **proposed**. The source is HMI Forge `92a612d`, `engine/` (`hmi-engine` 0.1.0). The destination is HMIStudio
`264ace7`, plus `crates/tag-engine` from PR #7 (`de9e207`, **not merged**).

## 1. What the engine is

- It is a Rust library (`driver`, `engine`, `guard`, `protocol`) and a binary called `hmi-engine`. The binary reads
  newline-delimited JSON requests on stdin and writes messages on stdout. Logs go to stderr. It is built with
  `panic = "abort"`.
- **Protocol:** `PROTOCOL_VERSION` 3 and `CONFIG_VERSION` 1.
  - Requests: `hello`, `configure` (sources and points: mapping id, variable id, source id, address, value type,
    access, `writeConfirmed`, limits and `pulseMs`; credentials are kept separate), `start`, `stop`, `armWrites`
    (actor, TTL), `disarmWrites`, `write` (variable id, value and actor), `pulse`, `browse`, `setDemand` and
    `shutdown`.
  - Messages: `helloOk`, `ok`, `error`, `writeResult` (refusals `WRITES_DISARMED`, `DRIVER_ERROR`, `NOT_CONFIRMED`,
    `OUT_OF_LIMITS`, `WRONG_COMMAND`, `BUSY`), `browseResult`, `sourceState`, `values` (quality
    GOOD/BAD/UNCERTAIN, status and source timestamp), `writesArmed`, `writeAudit` and `pulseReleased`.
  - Unknown fields are refused (`deny_unknown_fields`). Non-finite numbers are never written.
- **Runtime:** one tokio task drives a `select!` over requests, driver events, a 500 ms tick and the next pulse due.
  Normal stdin EOF and explicit `shutdown` disconnect sources. This is not a guarantee under pipe failure, stalled clients or abrupt termination; see the independent review in AUDIT_SUMMARY.md.
- **Drivers:** an `async_trait` `Driver` trait (start, write, set demand, browse and stop) with an event sink.
  `DRIVERS = ["loopback", "opcua", "s7"]`.
- **Guard:** writes are refused unless they are armed. An arming lasts at most 900 s, belongs to an actor, and a
  pulse lasts 100–5000 ms. Engineering limits and the write-confirmed mapping are also checked. The same case table
  (`protocol/write-guard-cases.json`) is checked in Rust and Kotlin.

## 2. Separate executable, crate, or both

| Option | For | Against |
| --- | --- | --- |
| **A. Separate executable** (Tauri sidecar in Studio; child process in the runtime, as in HMI Forge) | Keeps D-009/D-033 crash isolation: a driver fault cannot take down the editor or the operator UI. The engine guard stays independent of UI code. It is the same binary and protocol on all five targets. The protocol and fixtures already exist and are tested. MPL-2.0 files stay in a separately distributed executable. | It costs process management in Tauri (sidecar packaging per target triple, supervision). Values are serialised as JSON lines, which is acceptable at HMI rates. |
| B. Library crate linked into the Tauri process | No IPC. Rust types can be shared directly with `tag-engine`. | A driver panic aborts the whole Studio process (`panic = abort`). It has tokio and async-opcua inside the desktop process, and a second code path that differs from the runtime's. It reopens D-009, which HMI Forge rejected for JNI for this exact reason. |
| C. Both | Tests and tools can link the crate (it already has `lib.rs`). Production uses the executable. | It needs a rule so that the two do not diverge. |

**Recommendation: C, used as A in production.** Studio and every runtime run the `hmi-engine` executable over
protocol 3. HMIStudio code may link the `hmi-engine` library only in tests and command-line tools, for example to
load `protocol/messages.json` and check the Rust client against the engine's own types. No HMIStudio production code
hosts drivers in-process. This keeps D-009/D-033 intact, so they do not have to be reopened.

## 3. Placement against HMIStudio `tag-engine`

`crates/tag-engine` (PR #7) has a synchronous `Driver` trait with read and write items, a `DriverRegistry`,
`CancelToken`, the `SimulatorHub`/`SimPlc` simulator, a scan loop, subscriptions and the binding evaluator.

The overlap is in the driver layer. Duplicating OPC UA or S7 in `tag-engine` would conflict with the instruction
"Do not duplicate functionality already implemented". Proposed split:

| Concern | Owner |
| --- | --- |
| Value model (13 types, arrays, structs), quality, timestamps | `tag-engine` |
| Design-time simulation (no communication) | `tag-engine` simulator |
| Property bindings and transforms | `tag-engine` binding evaluator (plus expressions if decision O-4 adopts them) |
| OPC UA, S7, loopback communication, write guard, pulses, demand | `hmi-engine` (unchanged) |
| Translation | **new** `EngineDriver` in `tag-engine` (or a crate `engine-link`) implementing the synchronous `Driver` trait by talking protocol 3 to a supervised `hmi-engine` child |

`EngineDriver` responsibilities:

- launch, `hello` version check, `configure`, `start`, `setDemand` from active subscriptions, `values` into the
  tag-engine value cache, `stop` and `shutdown`;
- restart supervision with the same back-off as the Kotlin `LiveData` (1, 2, 4, 8, 16, then every 30 s; reset after
  60 s connected);
- arming is never restored after a restart, and every value shows BAD quality until it returns;
- type conversion per §4, refusing a point that cannot be represented, never truncating;
- writes as `armWrites` plus `write`/`pulse` with an explicit actor. Studio Live writes stay limited to isolated
  simulated or test endpoints with `commissioningAllowed` (D-004 here, D-031/decision 14 there). No production
  approval is added for development or simulation.

The client-side `WritePolicy` (login, permission, confirmation prompt, critical reason, read-back) lives in Kotlin
`runtime/live/WriteController`. Studio needs an equivalent before it offers operator-style writes. Until then,
Studio Live is read-only plus an explicit engineer-armed test write.

## 4. Value-type compatibility (blocking detail)

The engine carries 6 types: `Bool`, `Int` (i32), `Long` (i64), `Float` (f32), `Double` (f64) and `String`.
HMIStudio `DataType` has `Bool`, signed and unsigned integers from 8 to 64 bits, 32- and 64-bit floats, `String`,
`DateTime`, `Array` and `Struct`.

| HMIStudio | Protocol 3 | Rule |
| --- | --- | --- |
| Bool | Bool | lossless |
| Int8, Int16, Int32, UInt8, UInt16 | Int | lossless (range fits i32) |
| UInt32, Int64 | Long | lossless |
| UInt64 | Long | only values ≤ i64::MAX; otherwise refuse the value (BAD quality, never wrap) |
| Float32 | Float | lossless |
| Float64 | Double | lossless |
| String | String | lossless |
| DateTime | — | **not representable**; refuse configuration with a diagnostic |
| Array, Struct | — | **not representable**; expand to scalar members only if each member has its own address, otherwise refuse |

Proposed protocol 4 (decision O-9): add `DateTime`, `UInt64`, arrays and structs. Protocol 3 stays supported:
`hello` currently checks an exact version, rather than negotiating. A backward-compatible codec/handshake must be implemented and tested; protocol 3 fixtures must keep passing unchanged.

## 5. Driver-kind compatibility

| HMIStudio `DriverKind` | Engine driver | Note |
| --- | --- | --- |
| `Simulator` | — (tag-engine simulator) | the engine `loopback` is a test device, not a simulator |
| `OpcUa` | `opcua` | endpoint plus security preset in the engine's `configuration` map |
| `ModbusTcp` | **none** | no implementation in either repository |
| `Custom` | — | |
| — | `s7` | **missing in HMIStudio**; add `S7` (decision O-3), with the address syntax of `docs/SIEMENS.md` |
| — | `loopback` | add as a test kind or map `Custom` + parameter; decide in REUSE-02 |

## 6. Target integration

| Target | Studio (Tauri) | Runtime (Kotlin) |
| --- | --- | --- |
| Windows x64 | sidecar `hmi-engine-x86_64-pc-windows-msvc.exe` in the MSI; built on hosted `windows-latest` | unchanged (MSI resources) |
| Linux x64 | sidecar `hmi-engine-x86_64-unknown-linux-gnu` in the .deb | unchanged |
| Linux ARM64 | not a Studio target yet | cross build plus qemu handshake (exists) |
| Android | not a Studio target | engine as `libhmiengine.so` in the native library directory; child process (D-033, emulator-verified only) |
| Simulation | tag-engine simulator, no engine process needed | runtime `SIMULATED` packages; engine loopback for Live tests |

## 7. Security and supply chain

`cargo audit` (2026-11-24) on the engine's `Cargo.lock` found 2 vulnerabilities and 12 warnings.

- **RUSTSEC-2023-0071**, `rsa` 0.9.10 (Marvin timing side channel; no fixed release), pulled in by `async-opcua-client`
  and `async-opcua-crypto`. It matters only where RSA decryption with a private key is reachable, that is in OPC UA
  security modes that use RSA. Assess it per security policy (decision O-6).
- **RUSTSEC-2026-0592**, `url` 2.5.8 (stack overflow in `Url::origin()` on nested `blob:` URLs), pulled in by
  `async-opcua-core`. Check whether that call can be reached with untrusted input, and update when a fixed release
  is compatible.
- **Warnings:**
  - unmaintained: `chrono`, `unsafe-libyaml` (through the deprecated `serde_yaml`) and `atomic-polyfill`;
  - `heapless` 0.7.17 unsound advisories, which come only through `async-opcua-server`. The test server is
    test-only, so these do not ship in the `hmi-engine` binary.
- **`rust7` =0.1.2** is a pinned, single-author S7 client. Review it again when it is imported.

None of these block the audit. All of them block a release claim. They go into issue #4's tracking.

## 8. Protocol compatibility rules

1. Do not change protocol 3 messages. Any protocol 4 additions require explicit compatibility negotiation; no such negotiation exists today.
2. Keep `engine/protocol/messages.json` and `write-guard-cases.json` the single source of truth. Wherever they are
   used, verify them by SHA-256 against the engine copy.
3. The Rust client must pass the same round-trip and rejected-request fixtures as the Kotlin `EngineContractTest`.
