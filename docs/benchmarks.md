# Transfer throughput benchmarks

Risk gate for the russh-sftp read path (E2-S5). See `scripts/bench-transfer.sh`
and `crates/engine/examples/bench_download.rs`.

## Result — 2026-07-14 (localhost)

| Path                         | Size   | Time   | Throughput                             |
| ---------------------------- | ------ | ------ | -------------------------------------- |
| Engine download (russh-sftp) | 256 MB | 3.04 s | **84.3 MB/s**                          |
| `sftp` CLI                   | —      | —      | not measured (`sshpass` not installed) |

Environment: macOS (arm64), `atmoz/sftp` container running under amd64 emulation
on Docker Desktop, loopback network (~0 ms RTT).

## Interpretation

- 84 MB/s over an emulated-container loopback is healthy; the emulation layer is
  the likely bottleneck, not russh-sftp. On loopback there is no round-trip
  latency, so this run does **not** exercise the read-ahead concern.
- **No gate failure.** The E2-S5 threshold ("app < ~60% of CLI throughput on a
  ≥20 ms-RTT link") is about high-latency links; loopback can't reveal it.
  Nothing here triggers the "pipeline reads via `RawSftpSession`" follow-up.

## Still open (tracked in tasks.md risk register)

The definitive high-RTT test — the actual russh-sftp risk — requires a real
remote server or a `tc netem`-shaped link (Linux). russh-sftp's high-level
`File` reader issues sequential read requests without pipelining, which can cap
single-file throughput as RTT rises. The transfer path is isolated in
`crates/engine/src/transfer/io.rs`, so if a high-RTT benchmark later shows a
shortfall, overlapping reads via `RawSftpSession` (or parallel chunk ranges) can
be added there without touching callers.

## Re-running

```bash
scripts/bench-transfer.sh 256        # engine download of a 256 MB file
brew install sshpass                 # optional: enables the sftp CLI comparison
```
