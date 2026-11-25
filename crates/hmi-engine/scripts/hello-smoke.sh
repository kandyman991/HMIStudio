#!/usr/bin/env bash
# Starts an engine binary (optionally through an emulator such as qemu-aarch64), sends the protocol
# hello and checks the reply: the binary runs and speaks protocol 3 on that architecture.
# Usage: engine/scripts/hello-smoke.sh BINARY [RUNNER...]
set -euo pipefail
binary="$1"; shift
reply="$(printf '{"type":"hello","id":1,"protocol":3}\n' | timeout 30 "$@" "$binary" | head -n 1)"
echo "$reply"
echo "$reply" | grep -q '"type":"helloOk"' || { echo "no helloOk from $binary" >&2; exit 1; }
echo "$reply" | grep -q '"protocol":3' || { echo "unexpected protocol from $binary" >&2; exit 1; }
