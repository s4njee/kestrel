#!/usr/bin/env bash
# bench-transfer.sh — E2-S5 throughput risk gate.
#
# Measures the engine's SFTP download throughput against a local atmoz/sftp
# container, and (if sshpass is available) compares to the OpenSSH `sftp` CLI.
# Records the russh-sftp read path's real-world speed. NOTE: localhost has ~0 ms
# RTT, so this does not exercise the high-latency read-ahead concern — that
# needs a real remote or `tc netem` and is tracked in tasks.md's risk register.
#
# Usage: scripts/bench-transfer.sh [SIZE_MB]   (default 256)
set -euo pipefail

SIZE_MB="${1:-256}"
PORT=2222
NAME=sftp-bench

cleanup() { docker rm -f "$NAME" >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "== starting atmoz/sftp on :$PORT =="
docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" -p "$PORT:22" atmoz/sftp foo:pass:::upload >/dev/null
sleep 6

echo "== creating ${SIZE_MB} MB bench file on the server =="
docker exec "$NAME" sh -c "dd if=/dev/urandom of=/home/foo/upload/bench.bin bs=1048576 count=${SIZE_MB} 2>/dev/null"

echo "== engine download =="
SFTP_PORT="$PORT" cargo run --quiet --release --example bench_download -- /upload/bench.bin /tmp/engine-out.bin

if command -v sshpass >/dev/null 2>&1; then
  echo "== sftp CLI download =="
  rm -f /tmp/cli-out.bin
  START=$(date +%s.%N)
  sshpass -p pass sftp -q -P "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    foo@127.0.0.1:/upload/bench.bin /tmp/cli-out.bin
  END=$(date +%s.%N)
  DT=$(echo "$END - $START" | bc)
  MBPS=$(echo "scale=1; $SIZE_MB / $DT" | bc)
  echo "sftp CLI download: ${SIZE_MB} MB in ${DT}s = ${MBPS} MB/s"
else
  echo "(sshpass not installed — skipping CLI comparison; install with 'brew install sshpass')"
fi
