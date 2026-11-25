# RUST-04 — interactive semantic designer (draft sequencing)

Dependency: RUST-02 implementation merged at 4ca18a5973e0b624d06318c4767812989802c29b; manual acceptance issue #10 remains open. Owner authorized engineering continuation in #10; duplicate Linux manual checklist waived, visual acceptance remains open.

## RUST-04A
Insert row/column containers and built-in components into selected row/column or the selected leaf's flow parent. Disable insertion when placement semantics are unsupported. Move earlier/later within flow siblings using existing authoritative ReorderNode command. No client-owned document changes. Selection helpers are pure and covered by Node tests; native IPC, generation and revision guards unchanged. Existing structural preview remains labeled structural. PR #11 merged at 58d83ca540463d9507c1cd76555d0bac5bb6378b; exact-head desktop CI 45421552926 and handoff 45421552619 passed.

## Subsequent proposed slices
- Model-backed flow rendering (spacing, alignment, width/height and constraints), with measured DOM bounds and explicit preview limitations.
- Pointer selection, drag/reorder and reparent with immutable gesture preview, one transaction on commit, cancel and stale-generation handling.
- Bounded absolute positioning and resize/snap/alignment; grid and box operations respecting container semantics.
- Multi-selection, zoom/pan, responsive profile previews and reproducible fixtures; verify editor/runtime parity separately in RUST-05.

Do not weaken CSP, bypass Rust validation, or use absolute coordinates as the default representation. No production operations. No claim of Compose parity or full designer acceptance.

## RUST-04B candidate
- Base row/column CSS flex rendering, fixed/auto/fractional fill, min/max, padding, gap, main/cross alignment, wrap, weight, visibility, clipping and z-index mapping. Fixed 800×480 dp engineering preview surface, ruler and static components. CSS flex approximations, not a Compose solver.
- Explicit unsupported placeholders for non-flow containers/instances and unknown components; descendants remain accessible via layer tree. Responsive overrides, live binding values, styles, intrinsic Compose measurements and cross-platform parity are not evaluated. No drag/resize, snapping or runtime simulation yet.
- Size and flow arrangement edits use existing typed SetLayout/SetElement transactions, preserving other serialized fields and native revision/generation gates. Selection chrome does not participate in layout. CSS properties are assigned via React DOM style properties; unchanged CSP, actual WebView behavior requires acceptance.
- Original HTML recovered in PR #13. The independent review corrects the provisional workspace against the authoritative reference, without copying prototype state or mock operations. Visual acceptance remains in #10/RUST-04. No new dependencies or native permissions.
- Pure mapping tests included in npm build; see review evidence below. Exact-head hosted CI pending.

## PR #12 independent review corrections — 2026-11-25

Reconciled draft head 1dd36b5f1f1cf7fe5eed3f610bcae4b93e78ee20 with main 0a74f65558392ce7e8cfb67202244057d7d3a2d9. Resolved shared records against the canonical roadmap, retained both D-008 and selective-reuse D-009 without duplicate decision IDs. Predecessor Desktop CI 45434390655 and handoff 45434390848 passed; they do not validate the new head. Verified original prototype SHA-256 c6257aa279ad03c5ac72d1da1faa2e25a29f75b94e921f77308d694d3888de4d.

Findings and corrections:
- Provisional blue/pale workspace diverged from reference. Use its dark shell/mint tokens and compact header, icon toolbar, activity rail, switchable project/layers/components docks, tabbed engineering output, dark artboard and both rulers. Preserve real menu actions, dock resizing/swapping, persistence, native revision/generation checks and undo/redo; unimplemented preview, simulation and publishing remain disabled.
- Fixed 100% canvas overflowed typical workspace; fit-to-view and bounded 25–200% zoom now scale only presentation, keeping the 800×480 logical artboard. This deliberately retains the fixed preview profile rather than copying the prototype's 860×480 example.
- Explicit wrap line alignment, hidden noninteraction, decimal numeric parsing and unresolved style/event/text limitations; localized nonliteral text is marked unresolved. Accept valid fractional fill below 0.01% when representable. CSS flex remains an approximation rather than Compose parity.
- Follow-up RUST-04B.1 completes visual alignment, then RUST-04C implements direct graphical manipulation. Native visual acceptance, pointer drag/resize/pan/snap, responsive overrides, non-flow layouts, live bindings, style resolution and Compose parity are not claimed. Output tabs show actual diagnostics/status; other engineering views remain future work.

Local verification on reconciled source: 13/13 frontend tests (Node 24.22.1), TypeScript no-emit and Vite production build passed. The system Node 22.22.1 package lacks TypeScript stripping; used an existing separate Node executable, not a dependency change. Isolated headless Chromium via Playwright served built assets with the unchanged Tauri CSP, used an in-memory synthetic IPC fixture (root row, 10 dp gap, 160 dp text, 100 dp hidden text, 80 dp button), and loaded the unchanged reference separately. At 1280×800, compared shell #151a23, mint #62e2c3, header 58 px, toolbar 46 px and rail 49 px; logical 800×480, displayed width 496 within viewport 739; hidden node kept 100 dp width and next sibling x=280; gap 10 px. Both rulers, component/layer navigation, layout inspector, zoom/fit and 850×500 minimum window checked; no runtime or write IPC and no browser page errors. This is browser fixture evidence, not native IPC, WebView2/WebKitGTK or owner acceptance. Initial raw CDP harness did not complete; Playwright run passed. No PLC or production use.
