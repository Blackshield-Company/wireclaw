# Security Policy

## Supported versions

Only the latest release receives security fixes.

| Version | Supported |
|---------|-----------|
| 1.1.x   | ✅ |
| < 1.1   | ❌ |

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Report privately through
[GitHub Security Advisories](https://github.com/synthalorian/wireclaw/security/advisories/new).
Include the version (`wireclaw --version`), your OS, steps to reproduce, and the impact.
You should get an acknowledgement within 7 days. We'll agree on a disclosure date with you
once a fix is ready.

## Threat model and safe use

wireclaw is a **local debugging proxy** that deliberately intercepts traffic. Keep in mind:

- **The CA private key (`<data dir>/certs/ca.key`) is sensitive.** Anyone who has it can
  impersonate any HTTPS site to a machine that trusts the wireclaw CA. It is created with
  mode `0600` on Unix. Never share it, commit it, or bake it into images. Remove the CA from
  your trust stores when you no longer need it (see the README).
- **Captured sessions contain secrets.** Session databases (`<data dir>/sessions/*.db`) and
  exports (HAR, curl, Postman) store full headers and bodies, including `Authorization`
  headers, cookies and tokens. Treat them like credentials and scrub them before sharing.
- **Bind to localhost.** The proxy and dashboard listen on `127.0.0.1` by default and have
  no authentication. Do not expose them on a public interface.
- **Upstream TLS is verified** against the OS trust store (plus `WIRECLAW_UPSTREAM_CA` if
  set). wireclaw has no option to disable upstream certificate verification.
- **Lua hooks run with your user's privileges.** Only run scripts you trust.

## Release integrity

Release archives come with per-file `.sha256` files and an aggregated `SHA256SUMS`, all
built by GitHub Actions from the tagged commit. `install.sh` verifies the checksum before
installing.
