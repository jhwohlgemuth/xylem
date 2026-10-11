# Docker Sandbox provider

This example runs a Xylem workflow through the experimental public
`docker_sandbox` provider. Xylem uses the standalone local `sbx` CLI to create a
mountless preparation microVM, transfer the prepared module through a bounded
archive, create a separate network-denied execution microVM, copy in the input,
and publish `greeting.txt` after cleanup.

The policy pins Docker's published multi-platform `shell` template by digest.
It also uses limits accepted by the qualified CLI range: one whole CPU and 512
MiB of memory. Offline mode maps to `sbx create --pull never`; remove `offline`
from `.xylem.jsonc` to allow `--pull missing` on the first run.

Prerequisites:

- standalone `sbx` version 0.45.x, 0.46.x, or 0.47.x on `PATH`;
- a running local Docker Sandbox daemon; and
- the pinned template already present locally when using the checked-in offline
  configuration.

Run the example from the repository root:

```shell
cargo run -p xylem-cli -- \
  --sandbox-policy examples/docker-sandbox-provider/sandbox-policy.yaml \
  run examples/docker-sandbox-provider/.xylem.jsonc
```

To pull the pinned template when missing, copy the example, set `offline` to
`false`, and run the same command. Xylem always applies `--deny-network "**"`
to execution. Preparation also denies network in the checked-in policy.

The native end-to-end test is ignored by default because it creates and removes
real microVMs:

```shell
cargo test -p xylem-cli \
  test_docker_sandbox_example_native_end_to_end \
  -- --ignored --exact
```

Docker Sandbox currently does not give Xylem portable enforcement or evidence
for PID limits, scratch limits, read-only roots, or rootless operation. The CLI
prints an experimental-provider warning that names those gaps.
