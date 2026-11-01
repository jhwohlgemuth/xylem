# Failure policy

This shell-only workflow demonstrates `failure_policy: continue`. It admits
three inputs one at a time. The second input fails intentionally, and the
third still runs:

```text
first (succeeded) -> failure (failed) -> later (succeeded)
```

Run from any directory:

```shell
cargo run --example failure-policy
```

The command exits with status 70 because one input failed. With the default
`fail_fast` policy, `later` would remain `not_started` because `batch: 1`
prevents it from being admitted before the failure.
