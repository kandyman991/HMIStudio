//! HMI Studio runtime engine.
//!
//! Hosts communication drivers behind a small, versioned message boundary (see [`protocol`]).
//! The engine never initiates a write: values reach a device only as the direct result of a
//! `write` request that passes [`guard::check_write`].

pub mod driver;
pub mod engine;
pub mod guard;
pub mod protocol;

/// OPC UA server for tests only (never part of the engine binary).
#[cfg(feature = "test-server")]
pub mod testserver;

/// S7 test PLC for tests only (never part of the engine binary).
#[cfg(feature = "test-server")]
pub mod testserver_s7;
