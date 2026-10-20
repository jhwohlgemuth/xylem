# Acquisition platform qualification

SEC-04 (Linux amd64) and SEC-05 (Linux arm64) were accepted on 2026-10-20
against commit `b1e92978ffdb0517aef0af79134ebd3c4abe9976`. Both jobs ran
`cargo test -p xylem-lib acquisition::tests --locked` in
[pipeline 1034462](https://code.ornl.gov/research-enablement/xylem/-/pipelines/1034462).

| Phase | CI job | Runner | Runner image digest | Rust | ORAS | Result |
|---|---|---|---|---|---|---|
| SEC-04 | [4448364](https://code.ornl.gov/research-enablement/xylem/-/jobs/4448364) | RE Runner (kronk), amd64 | `savannah.ornl.gov/research-enablement/acorn/rust@sha256:73805fc3ffd1eb1a465469d550f2741574043f779462d83f51de2a3f01b6a519` | 1.98.0 (`88d9e12ae`, 2026-08-18) | 1.3.4, linux/amd64 | 33 passed, 0 failed |
| SEC-05 | [4448365](https://code.ornl.gov/research-enablement/xylem/-/jobs/4448365) | RE Runner (dante), arm64 | `ghcr.io/jhwohlgemuth/acorn@sha256:2b9f48b7eb3567a660afa178554f2f313d96cdc9034a78374f1e12554625d6b7` | 1.98.0 (`88d9e12ae`, 2026-08-18) | 1.3.4, linux/arm64 | 33 passed, 0 failed |

The amd64 job trace reports the Savannah digest; the image publisher supplied
its Rust version. The arm64 job trace reports the GHCR digest, and running
that immutable image with `rustc --version` returned the same Rust version.
Both traces report the ORAS version and the focused test result. The HTTPS Git
fixture test passed on both architectures.

## SEC-06 macOS arm64

SEC-06 was accepted on 2026-10-20 against commit
`938d8f195a3667fc35158ced0f1a7e33a6a90314` on a local macOS 26.7.1
arm64 host. The host used Rust 1.99.0 (`b940084d7`, 2026-09-28),
ORAS 1.3.4+Homebrew, Apple Git 2.54.0, and OpenSSH 10.3p1.

`cargo test -p xylem-lib acquisition::tests --locked --offline` passed with
33 tests, 0 failed, and 0 ignored. The passing cases include pinned Git over
verified HTTPS and explicit SSH, OCI registry and local layout acquisition,
cache concurrency, symlink containment, executable permissions, and bounded
timeouts. The suite used local loopback fixtures and did not require an
external acquisition service.

SEC-07 (Windows and dependency audit) remains open.
