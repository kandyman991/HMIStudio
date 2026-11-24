# Original visual reference — HMI Studio

Owner-supplied original HTML prototype, recovered 2026-11-24 and explicitly authorized for storage in this repository. Preserve the source unchanged.

- File: [hmi-forge.html](hmi-forge.html)
- Original byte count: 116579 (UTF-8, LF).
- SHA-256: `c6257aa279ad03c5ac72d1da1faa2e25a29f75b94e921f77308d694d3888de4d`
- Git blob SHA-1: `e4e74428c13142e2e7f36e7b3958e7576ead6590` (verified against the original upload and GitHub).

## Owner-approved design direction

Use this file as the primary visual and interaction reference for the actual desktop editor: compact dark industrial theme with mint accents, iconography, menu/submenu hierarchy, toolbars, activity rail, project/layer/component docks, tabbed designer, rulers, zoom/pan/snapping, property inspector, tabbed engineering output dock and status bar. Maintain the clean workspace organization and usability. Use HMI Studio branding. Review departures with the owner; visual acceptance remains open in issue #10.

## Implementation boundary

The prototype is documentation, not application code or a runtime dependency. Keep the existing Tauri/React/TypeScript/CSS frontend and authoritative Rust engineering engine, typed IPC, project schema, validation, transactions, undo/redo, native persistence and recovery. Do not load the prototype into the production WebView, copy its browser-local state or mock PLC/installer behavior into the application, weaken CSP, or replace semantic layouts with a fixed-coordinate-only model. Absolute placement is appropriate only in explicitly modeled absolute containers. Keep real and simulated operations distinct. Kotlin/Compose runtime targets and both delivery modes remain unchanged. Unsupported controls must be clearly labeled or disabled; prototype visuals do not prove implementation.

## Next action

Finish PR #12 under existing engineering gates. Then use this reference for RUST-04B.1 (workspace visual alignment), before RUST-04C (direct manipulation). Compare actual desktop screenshots and interactions to the reference during RUST-04 acceptance. F-001's missing-source blocker is resolved by recovery; implementation fidelity is still unverified. No private legacy repository code transfer or license grant is implied. Sample data is presented by the prototype as a virtual PLC demonstration; no real plant connection is authorized.

To inspect, download the HTML and open it as a standalone local browser prototype. Do not use it for production commissioning.
