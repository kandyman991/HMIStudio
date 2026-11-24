# RUST-04 — interactive semantic designer (draft sequencing)

Dependency: RUST-02 implementation merged at 4ca18a5973e0b624d06318c4767812989802c29b; manual acceptance issue #10 remains open. Preparatory draft only; no acceptance waiver.

## RUST-04A
Insert row/column containers and built-in components into selected row/column or the selected leaf's flow parent. Disable insertion when placement semantics are unsupported. Move earlier/later within flow siblings using existing authoritative ReorderNode command. No client-owned document changes. Selection helpers are pure and covered by Node tests; native IPC, generation and revision guards unchanged. Existing structural preview remains labeled structural. Tests not yet executed for this candidate.

## Subsequent proposed slices
- Model-backed flow rendering (spacing, alignment, width/height and constraints), with measured DOM bounds and explicit preview limitations.
- Pointer selection, drag/reorder and reparent with immutable gesture preview, one transaction on commit, cancel and stale-generation handling.
- Bounded absolute positioning and resize/snap/alignment; grid and box operations respecting container semantics.
- Multi-selection, zoom/pan, responsive profile previews and reproducible fixtures; verify editor/runtime parity separately in RUST-05.

Do not weaken CSP, bypass Rust validation, or use absolute coordinates as the default representation. No production operations. No claim of Compose parity or full designer acceptance.
