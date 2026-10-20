![wireclaw](.github/preview.png)

```
 _   _   _   _   _   _   _   _  
/ \ / \ / \ / \ / \ / \ / \ / \ 
( w | i | r | e | c | l | a | w )
 \_/ \_/ \_/ \_/ \_/ \_/ \_/ \_/
```

# wireclaw — API Traffic Observability Platform

> **Auto-document your API by watching it work.**

Wireclaw is a local HTTP/HTTPS proxy that captures every API request and response, stores them in SQLite, and gives you a real-time web dashboard, terminal UI, and one-click OpenAPI export. No code changes. No SDKs. No manual documentation.

**Built with Rust. No `unsafe` in wireclaw's own code. Zero config.**

---

## Why Wireclaw?

| The Problem | The Cost |
|-------------|----------|
| API docs drift from code the moment they ship | Hours of manual updates, outdated contracts |
| Debugging production issues means hunting through logs | Slower incident response, frustrated teams |
| Onboarding new devs requires explaining API behavior | Repeated knowledge transfer, tribal knowledge |
| No easy way to compare "this request works, that one doesn't" | Staring at JSON diffs in text editors |

**Wireclaw turns API observability from a chore into a byproduct of normal development.**

Point your HTTP client at the proxy. Ship your code. Browse the dashboard. Export the spec. Done.

---

## Features

- **🔴 Capture** — Local HTTP/HTTPS proxy. Every request/response logged to SQLite, organized by named session.
- **🔒 HTTPS MITM** — Auto-generated per-host TLS certificates. Inspect encrypted traffic without touching client code.
- **📊 Real-Time Dashboard** — WebSocket-powered traffic visualization. One-click OpenAPI export. Three themes including Synthwave '84.
- **📋 OpenAPI Auto-Generation** — Generate OpenAPI 3.0 specs from live traffic. Real examples, inferred schemas, no manual work.
- **🔁 Replay & Chain** — Re-send any captured request. Dry-run, diff, edit in `$EDITOR`, batch replay, and chain requests with Lua variable extraction.
- **🔍 Search & Diff** — Regex search across method, path, headers, body. JSON-aware structural diff between any two requests.
- **📈 Performance Monitoring** — Latency percentiles (p50, p95, p99), error rates, slow request detection.
- **🖥️ Terminal UI** — Full ratatui interface with live streaming, keyboard navigation, JSON syntax highlighting. Works over SSH.
- **📤 Export** — HAR 1.2, curl commands, raw HTTP, Postman collections.
- **⚡ Zero Config** — Works out of the box. Customizable via `~/.config/wireclaw/config.toml` when you need it.

---

## Quick Start

### Install a Prebuilt Binary

```bash
# Linux / macOS (x86_64 and arm64): verifies the SHA-256 checksum
curl -fsSL https://raw.githubusercontent.com/synthalorian/wireclaw/main/install.sh | bash
```

Or download an archive for your platform from
[Releases](https://github.com/synthalorian/wireclaw/releases) (Linux x86_64/aarch64,
macOS x86_64/arm64, Windows x86_64) and check it against `SHA256SUMS`:

```bash
sha256sum -c SHA256SUMS --ignore-missing   # macOS: shasum -a 256 -c SHA256SUMS --ignore-missing
```

### Install from Source

```bash
git clone https://github.com/synthalorian/wireclaw.git
cd wireclaw
cargo install --path .
```

### Capture Traffic

```bash
# Start proxy + dashboard
wireclaw capture --session my-api --dashboard

# Point your client at the proxy (HTTPS needs the CA installed, see below)
export HTTP_PROXY=http://127.0.0.1:8080 HTTPS_PROXY=http://127.0.0.1:8080
curl https://api.example.com/users

# Open the dashboard
# → http://localhost:8746
```

### HTTPS Interception: Trust the wireclaw CA

To read HTTPS traffic, wireclaw terminates TLS with a per-host certificate signed by
its own local certificate authority. Generate the CA once, then tell your OS or client
to trust it:

```bash
wireclaw ca generate   # prints the PEM and the path of ca.crt
```

The CA lives in `<data dir>/certs/` (`ca.crt` public, `ca.key` private, mode `0600` on Unix):

| OS | Default data dir |
|----|------------------|
| Linux | `~/.local/share/wireclaw` |
| macOS | `~/Library/Application Support/wireclaw` |
| Windows | `%APPDATA%\wireclaw` |

Set `WIRECLAW_DATA_DIR` to override it.

**Linux (Debian/Ubuntu)**
```bash
sudo cp ~/.local/share/wireclaw/certs/ca.crt /usr/local/share/ca-certificates/wireclaw.crt
sudo update-ca-certificates
```

**Linux (Fedora/RHEL/Arch)**
```bash
sudo cp ~/.local/share/wireclaw/certs/ca.crt /etc/pki/ca-trust/source/anchors/wireclaw.crt   # Arch: sudo trust anchor ca.crt
sudo update-ca-trust
```

**macOS**
```bash
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain \
  "$HOME/Library/Application Support/wireclaw/certs/ca.crt"
```

**Windows (PowerShell as Administrator)**
```powershell
Import-Certificate -FilePath "$env:APPDATA\wireclaw\certs\ca.crt" -CertStoreLocation Cert:\LocalMachine\Root
# or, without admin, for the current user only:
certutil -user -addstore Root "%APPDATA%\wireclaw\certs\ca.crt"
```

**Firefox** uses its own store: Settings → Privacy & Security → Certificates → View
Certificates → Authorities → Import (or set `security.enterprise_roots.enabled` to `true`).

**Per tool, no system change:** `curl --cacert <data dir>/certs/ca.crt ...`,
`NODE_EXTRA_CA_CERTS=<path>`, `REQUESTS_CA_BUNDLE=<path>` (Python requests),
`SSL_CERT_FILE=<path>` (many OpenSSL-based tools).

**Upstreams with a private CA:** wireclaw verifies upstream servers against the OS trust
store. To also trust an internal CA (or a local self-signed dev server), point
`WIRECLAW_UPSTREAM_CA` at a PEM file (several files: separate with `:` on Unix, `;` on
Windows). Untrusted upstreams are rejected with `502`.

> ⚠️ Anyone holding `ca.key` can impersonate any site to a machine that trusts this CA.
> Keep it private, and remove the CA from your trust store when you're done
> (`sudo rm /usr/local/share/ca-certificates/wireclaw.crt && sudo update-ca-certificates --fresh`,
> `sudo security delete-certificate -c "Wireclaw Proxy CA" /Library/Keychains/System.keychain`,
> or `certutil -delstore Root "Wireclaw Proxy CA"`).

### Export (HAR, curl, Postman, raw)

```bash
wireclaw export --format har --session my-api --output my-api.har   # HAR 1.2, opens in browser devtools
wireclaw export --format curl --session my-api
```

### Generate OpenAPI from Live Traffic

```bash
# After capturing traffic, export the spec
wireclaw openapi --session my-api --output api-spec.json
```

### Replay & Diff

```bash
# List captured requests
wireclaw list --session my-api

# Replay a specific request
wireclaw replay --id <request-id>

# Compare two requests side-by-side
wireclaw diff --a <id1> --b <id2> --session my-api
```

### Launch the TUI

```bash
wireclaw tui --session my-api
```

---

## Demo

```bash
# Full demo: capture + dashboard + sample traffic
./demo.sh
```

The demo script starts a proxy, generates sample API traffic, and opens the dashboard. Perfect for screen recording a submission video.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        wireclaw                              │
│                                                              │
│  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────────────┐  │
│  │  CLI   │  │  TUI   │  │ Config │  │  Web Dashboard │  │
│  │(clap)  │  │(ratatui│  │(TOML)  │  │   (axum+ws)   │  │
│  └───┬────┘  └───┬────┘  └───┬────┘  └───────┬────────┘  │
│      │           │           │               │             │
│  ┌───▼───────────▼───────────▼───────────────▼─────────┐  │
│  │                   Core Dispatch                        │  │
│  └──┬──────┬────────┬─────────┬──────────┬────────────┘  │
│     │      │        │         │          │                │
│  ┌──▼──┐ ┌──▼──┐ ┌──▼───┐ ┌──▼──┐  ┌───▼────┐           │
│  │Proxy│ │Replay│ │Search│ │Export│  │ Logger │           │
│  │(hyper│ │      │ │(regex)│ │(HAR) │  │        │           │
│  └──┬──┘ └──────┘ └──────┘ └──────┘  └───┬────┘           │
│     │                                      │                │
│     │  ┌───────────────────────────────────▼──────────┐   │
│     └──│          SQLite Storage (sqlx)               │   │
│        │   sessions.db → requests → responses        │   │
│        │   ~/.local/share/wireclaw/sessions/*.db       │   │
│        └───────────────────────────────────────────────┘   │
│                                                              │
│  Data Flow: Client → Proxy → Target → Proxy → Client        │
│                       ↓                                      │
│                   Logger → SQLite                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Technical Highlights

- **~9,000 lines of Rust** — no `unsafe` blocks in wireclaw's own code
- **53 unit tests + end-to-end tests** — capture, replay and HTTPS MITM against local HTTP/HTTPS servers, on Linux, macOS and Windows CI
- **SQLite + sqlx** — type-safe async database operations
- **HTTPS MITM** — auto-generated per-host certificates via `rcgen`
- **Lua scripting** — hooks for request/response transformation
- **WebSocket proxy** — captures and replays WebSocket frames
- **HAR/Postman/curl export** — industry-standard formats

---

## Configuration

wireclaw looks for config at `~/.config/wireclaw/config.toml`. Sensible defaults are used if it doesn't exist.

```toml
listen_addr = "127.0.0.1:8080"
data_dir = "~/.local/share/wireclaw"

[session]
auto_create = true
default_name = "default"

[proxy]
listen_addr = "127.0.0.1:8080"
timeout_secs = 30
max_body_size = 10485760  # 10MB
capture_headers = true
capture_bodies = true

[replay]
delay_ms = 0
follow_redirects = true
max_redirects = 10
```

---

## Data Storage

| Path | Purpose |
|------|---------|
| `~/.config/wireclaw/config.toml` | Configuration file |
| `<data dir>/sessions/<name>.db` | Per-session SQLite database |
| `<data dir>/certs/ca.crt`, `ca.key` | Local CA for HTTPS interception |

Each session gets its own SQLite database with indexed tables for requests, responses, and session metadata.

---

## Development

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt
```

---

## Security

See [SECURITY.md](SECURITY.md) for how to report vulnerabilities and for the CA key threat model.

---

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

---

## Credits

Developed by **synth**.

*This is the wave. ⚫🦞🌆*

---

## ☕ Support the Developer

If this project saved you time, solved a problem, or just made your day a little more neon, you can fuel the next one:

[![Buy Me A Coffee](https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png)](https://buymeacoffee.com/synthalorian)
