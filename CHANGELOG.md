# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2026-10-12

First release with prebuilt binaries.

### Added
- Prebuilt release archives for Linux (x86_64, aarch64), macOS (x86_64, arm64) and
  Windows (x86_64), each with a `.sha256` file, plus an aggregated `SHA256SUMS`.
- `install.sh`: downloads the right archive, verifies its checksum, and falls back to
  `cargo install` when no binary is available.
- `WIRECLAW_UPSTREAM_CA`: extra PEM CA bundle(s) to trust for upstream TLS, for internal
  services on a private CA and local self-signed dev servers.
- End-to-end tests that run the real binary against local HTTP and HTTPS servers:
  capture, session isolation, error handling, HAR export, replay (by id, `--count`,
  `--dry-run`, `--filter`, `--diff`), CA generation and persistence, per-host leaf
  signing, MITM capture of decrypted traffic, and rejection of untrusted CAs/upstreams.
- README: per-OS CA trust setup (Linux, macOS, Windows, Firefox, per-tool), CA removal,
  export usage, and install instructions. New `SECURITY.md`.
- CI: fmt, clippy `-D warnings`, and tests on Ubuntu, macOS and Windows; shellcheck for
  `install.sh`. Release workflow for `v*` tags.

### Changed
- Lua is now vendored (`mlua/vendored`), so no system Lua headers are needed and builds
  work on macOS and Windows.
- TLS uses the `ring` crypto provider only (dropped the unused `aws-lc-rs` build).
- The proxy logs the address it actually bound, so `--addr 127.0.0.1:0` is usable.
- Missing native root certificates are now a warning instead of a panic.

### Fixed
- HTTPS interception of IP-literal hosts (e.g. `CONNECT 127.0.0.1:8443`): leaf
  certificates now carry an IP-address SAN instead of failing to build.
- README claims corrected (line count, test count, repository URL, `HTTPS_PROXY` in the
  quick start).

## [1.0.0] - 2026-07-28

Tagged without release binaries. HTTPS MITM, replay with diff, TUI search, web
dashboard, OpenAPI generation, Lua hooks, WebSocket capture/replay, request chaining,
and HAR/Postman/curl/raw export.

## [0.3.0] - 2026-06-03

### Added
- Full WebSocket proxying, capture and replay.

## [0.2.0] - 2026-06-02

### Added
- Docker image, CI/CD, README refresh.

[1.1.0]: https://github.com/synthalorian/wireclaw/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/synthalorian/wireclaw/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/synthalorian/wireclaw/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/synthalorian/wireclaw/releases/tag/v0.2.0
