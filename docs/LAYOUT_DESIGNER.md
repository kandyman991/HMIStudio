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
- Provisional grouped toolbars, component library, designer frame/ruler, layer tree and layout inspector. Original HTML artifact still unavailable (F-001); exact fidelity not claimed. Visual acceptance remains in #10/RUST-04. No new dependencies or native permissions.
- Pure mapping tests included in npm build; exact-head CI and independent review pending. No local execution claimed.
