#!/usr/bin/env bash
# Builds hmi-engine for Android ABIs and places it where :runtime-android packages it (D-033):
#   runtime-android/build/engine-jni/<abi>/libhmiengine.so
# The file is the engine *executable*; Android extracts it to the app's native library directory,
# from where the runtime starts it as a child process (D-009 unchanged).
#
# Usage: engine/scripts/build-android.sh [arm64-v8a] [armeabi-v7a] [x86_64]   (default: all three)
# Needs: rustup targets for the ABIs, and the Android NDK in ANDROID_NDK_HOME, ANDROID_NDK_LATEST_HOME
# or ANDROID_NDK_ROOT. PROFILE=debug builds faster unoptimized binaries.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-${ANDROID_NDK_ROOT:-}}}"
[ -n "$ndk" ] && [ -d "$ndk" ] || { echo "Android NDK not found (set ANDROID_NDK_HOME)" >&2; exit 2; }
host="$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64"
bin="$ndk/toolchains/llvm/prebuilt/$host/bin"
api=26
profile="${PROFILE:-release}"
abis=("$@")
[ ${#abis[@]} -gt 0 ] || abis=(arm64-v8a armeabi-v7a x86_64)

for abi in "${abis[@]}"; do
  case "$abi" in
    arm64-v8a)   target=aarch64-linux-android;     clang="aarch64-linux-android$api-clang" ;;
    armeabi-v7a) target=armv7-linux-androideabi;   clang="armv7a-linux-androideabi$api-clang" ;;
    x86_64)      target=x86_64-linux-android;      clang="x86_64-linux-android$api-clang" ;;
    *) echo "unknown ABI $abi" >&2; exit 2 ;;
  esac
  upper="$(echo "$target" | tr '[:lower:]-' '[:upper:]_')"
  export "CARGO_TARGET_${upper}_LINKER=$bin/$clang"
  export "CC_${target//-/_}=$bin/$clang"
  export "AR_${target//-/_}=$bin/llvm-ar"
  flags=(--manifest-path "$root/engine/Cargo.toml" --bin hmi-engine --target "$target")
  [ "$profile" = release ] && flags+=(--release)
  cargo build "${flags[@]}"
  out="$root/runtime-android/build/engine-jni/$abi"
  mkdir -p "$out"
  cp "$root/engine/target/$target/$profile/hmi-engine" "$out/libhmiengine.so"
  echo "$abi: $out/libhmiengine.so ($(du -h "$out/libhmiengine.so" | cut -f1))"
done
