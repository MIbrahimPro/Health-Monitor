#!/usr/bin/env bash
# Build and run the offline rPPG benchmark against the recorded test video.
# Usage: scripts/bench.sh [--label name] [extra bench_rppg args...]
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p aegis-core --bin bench_rppg 2>&1 | tail -2
exec ./target/release/bench_rppg "$@"
