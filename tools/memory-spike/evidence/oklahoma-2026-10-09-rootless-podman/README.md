# Oklahoma rootless Podman collector conformance

This directory retains the successful synthetic collector-conformance run on
Oklahoma from 2026-10-09. The host was native x86_64 Linux and Podman 3.4.4 ran
locally as the unprivileged `o9w` user with cgroup v2 CPU, memory, and PID
controllers delegated.

The run used the locally built baseline workload
`localhost/xylem-sandbox-runner@sha256:66ae05e65efa313d4c4535452292a5e72cc1dcef3e67db01b1b9c4d2d71dac26`.
That digest identifies the Oklahoma-local image and is not a published registry
manifest.

Run `FsifEzCO84` succeeded and retained a 3,056 KB per-input cgroup peak against
a 131,072 KB reservation. `podman-summary.json` is the terminal run summary;
`podman-journal.jsonl` is the complete ordered journal. Together with the
retained Docker run, this evidence closes synthetic collector conformance. It
does not validate the A/P estimator or replace the A/P sandbox comparison.
