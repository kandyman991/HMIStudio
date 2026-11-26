# Staged production-parser conformance test

This source is deliberately outside Cargo's automatic test discovery. The
current REUSE-03 reservation covers only crates/legacy-import/. Do not copy
it into the engine crate on a shared branch without a coordinated scope
expansion and regenerated roadmap.

For a separately authorized, disposable checkout, install this file as
crates/hmi-engine/tests/legacy_address_conformance.rs and run:

    cargo test --locked --manifest-path crates/hmi-engine/Cargo.toml --test legacy_address_conformance

The relative module path is correct at that destination. Review and format
the test in that checkout. This test has not yet been compiled or executed;
normal hosted importer CI does not run it. No device connections are needed.
