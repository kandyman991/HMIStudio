#!/usr/bin/env bash
# Cross-builds hmi-engine for Linux ARM64 (panels, single-board computers) on an x86_64 Linux host.
# Needs: rustup target aarch64-unknown-linux-gnu and gcc-aarch64-linux-gnu.
# Output: engine/target/aarch64-unknown-linux-gnu/<profile>/hmi-engine
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
profile="${PROFILE:-release}"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER:-aarch64-linux-gnu-gcc}"
export CC_aarch64_unknown_linux_gnu="${CC_aarch64_unknown_linux_gnu:-aarch64-linux-gnu-gcc}"
flags=(--manifest-path "$root/engine/Cargo.toml" --bin hmi-engine --target aarch64-unknown-linux-gnu)
[ "$profile" = release ] && flags+=(--release)
cargo build "${flags[@]}"
echo "$root/engine/target/aarch64-unknown-linux-gnu/$profile/hmi-engine"
