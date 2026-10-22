# Oklahoma rootless Docker collector conformance

This directory retains the successful synthetic collector-conformance run on
Oklahoma from 2026-10-09. The host was native x86_64 Linux and Docker 29.8.2 ran
locally as an unprivileged user with cgroup v2 CPU, memory, and PID
controllers delegated.

The run used the locally built baseline workload
`localhost/xylem-sandbox-runner@sha256:be1ac40b4674dc8f0da7eafc509c05b83132ebb8478cde4152028d60d14072d8`.
That digest identifies the Oklahoma-local image and is not a published registry
manifest.

Run `ZksTg4eTQ8` succeeded and retained a 5,792 KB per-input cgroup peak against
a 131,072 KB reservation. `docker-summary.json` is the terminal run summary;
`docker-journal.jsonl` is the complete ordered journal. Together with the
retained Podman run, this evidence closes synthetic collector conformance. It
does not validate the A/P estimator or replace the A/P sandbox comparison.
