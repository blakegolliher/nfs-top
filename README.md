# nfs-top

Ratatui-inspired Linux NFS client monitor for `/proc/*` data sources.

## Build and run

- `cargo run --release`
- `cargo run --release --no-default-features --features=termion`
- `cargo run --release --no-default-features --features=termwiz`

## Portable build (Makefile)

- `make portable-host`
  - Builds a static musl binary for the current Linux CPU architecture.
- `make portable TARGET=x86_64-unknown-linux-musl`
  - Builds one target (uses `cargo-zigbuild` when available).
- `make portable-all`
  - Builds static binaries for:
    - `x86_64-unknown-linux-musl`
    - `aarch64-unknown-linux-musl`
    - `armv7-unknown-linux-musleabihf`

Artifacts are placed in `dist/` as `nfs-top-<target>`.

## Packages

- `make rpm` — Build an `.rpm` for the host arch (set `RPM_TARGET=<triple>`
  to cross-package). Output: `dist/nfs-top-<version>-<release>.<arch>.rpm`.
  Requires `rpmbuild` (`dnf install rpm-build`).
- `make rpm-all` — Build `.rpm`s for all targets.
- `make deb` / `make deb-all` — Equivalent for `.deb`. Works on RHEL hosts
  via an `ar`+`tar` fallback when `dpkg-deb` isn't installed.

Override per-package metadata with `PKG_LICENSE=...`, `PKG_MAINTAINER=...`,
`RPM_RELEASE=...`, etc. See `make help`.

## CLI

- `--interval-ms <N>` sampling interval, default `1000`
- `--history <N>` rolling samples for charts, default `120`
- `--mount <substring>` initial mount filter
- `--mp <substring>` alias for `--mount`
- `--sort <read|write|ops|rtt|exe|mount|nconnect|obsconn>`
- `--units <auto|m|g|t>`
- `--no-dns`
- `--raw-dump <path>` dump one parsed snapshot and exit
- `--remote-ports <csv>` default `2049,20049`

## Keybinds

- `q` quit
- `h/l` or `Left/Right` change tab
- `j/k` or `Up/Down` select mount
- `space` pause/resume
- `r` reset baseline/history
- `s` cycle sort
- `p` cycle trends mode (`all`, `avg`, `p90`, `p95`, `p99`)
- `?` help tab
- `a/m/g/t` units mode
- `+/-` adjust local UI interval indicator

## Data sources

- `/proc/self/mountstats`
- `/proc/mounts` (fallback `/etc/mtab`)
- `/proc/net/rpc/nfs`
- `/proc/net/tcp` + `/proc/net/tcp6`

## Limitations

- Connection attribution to mounts is heuristic and primarily based on `addr=` and DNS resolution of `server:/export` hostnames.
- Per-op timing fields vary across kernel/NFS versions, so some latency cells can show `-` when unavailable.
- PID/inode ownership correlation is not enforced in this MVP; observed connections are remote-IP based.

## Future work

- **eBPF-based sampling.** Replace (or augment) the `/proc`-based samplers
  with eBPF probes on the kernel's NFS/SUNRPC tracepoints (e.g.
  `nfs:nfs_initiate_read`, `sunrpc:rpc_task_begin`, `sunrpc:xprt_transmit`).
  This would give per-call latency histograms, per-PID attribution, and
  fewer surprises from kernel-version drift in the `/proc/self/mountstats`
  format. Cost: requires `CAP_BPF` (or root) and a kernel new enough for
  CO-RE; the `/proc` path stays as a fallback for restricted environments.
- **Richer packaging.** The `.deb` target is functional but minimal
  (control file + binary). A future pass could add proper
  `/usr/share/doc/<pkg>/copyright`, a `man` page, shell completions, and
  signed releases for both `.deb` and `.rpm`.
