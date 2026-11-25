# Failed and superseded approaches

No confirmed architectural failures have been established in this public repository as of 2026-11-24. Do not infer failure from a migration preference or unavailable evidence.

## F-001 — Direct reuse of original HTML prototype
- Status: INCONCLUSIVE
- Scope: RUST-01 initial shell, 2026-11-24.
- Hypothesis: integrate the original prototype into Tauri directly.
- Evidence: prototype source was not recovered during initial implementation; no faithful integration test was possible. PR #1 contains a reconstruction.
- Root cause: source unavailable to the implementation session; cause of unavailability unknown.
- Next: recover actual source, compare with shell and obtain visual acceptance. Do not claim original prototype failed.

## F-002 — Fixed-coordinate-only project model
- Status: REJECTED_BY_DESIGN
- Reason: conflicts with nested semantic and adaptive Compose layout goals; no comparative test claimed.
- Reconsider only for explicitly bounded absolute-positioning containers, not as a replacement for semantic layout.

Use ARCHITECTURE_MEMORY.md for entry format. Never fabricate negative evidence or import private operational records.

## F-001 follow-up — reference recovered (2026-11-24)
- Missing-source blocker resolved: owner supplied the original HTML prototype and authorized its preservation at `docs/reference/hmi-forge.html`. See `docs/reference/README.md` for verified provenance and scope.
- Direct integration of prototype application logic is not authorized or required. Preserve its visual layout while retaining the existing authoritative Rust architecture. Actual desktop visual fidelity remains INCONCLUSIVE until comparison and owner acceptance (#10).
