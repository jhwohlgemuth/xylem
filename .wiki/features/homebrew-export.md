---
type: Plan
title: "Activate the Homebrew release export"
description: "Finish and verify the implemented Homebrew publication path with protected credentials, required runners, and the v1.0.3 release."
difficulty: medium
impact: high
package: [workspace, xylem-cli]
tags: [homebrew, release, distribution, macos, linux, packaging]
status: in-progress
---

# Goal

Activate the implemented Homebrew publication path and verify that macOS and
Linux users can install and upgrade Xylem through the official tap.

# Current evidence

- Formula generation, tag-only tap merge-request creation, tap CI, offline formula testing, and upgrade coverage are implemented.
- The immutable v1.0.0 archive cannot compile with Homebrew Rust 1.98 because it locks `ethnum` 1.5.2. The published v1.0.3 archive locks the compatible `ethnum` 1.5.3 release.
- The v1.0.3 release pipeline published both crates and opened tap merge request `!2`. Its Linux validation job passes.
- The v1.0.3 formula passes `brew style`, `brew audit --strict`, a macOS source install with Homebrew Rust 1.98.1, and `brew test`.
- The tap's macOS CI job failed with `stuck_pending_no_matching_runners`, but the pipeline remains successful because that job may fail. Activation still requires the macOS job to pass, both platform jobs to become required, and merge request `!2` to merge.

# Project code boundaries

- **Update:** `README.md`, `CONTRIBUTING.md`, and release documentation after the first successful publication.
- **Create:** a compatible macOS runner assignment and required merge checks in the tap project settings.
- **Delete:** None.

# Dependencies and ordering

The immutable v1.0.3 tag and artifacts were produced from the release workflow. The tap merge request must pass both platform jobs before merge. Update the public installation documentation with the verified command after activation.

# Recommendation

1. Assign a compatible macOS runner to the tap project and require both validation jobs before merge.
2. Retry the macOS job for the v1.0.3 tap merge request after assigning the runner.
3. Review and merge the generated formula after both platform jobs pass.
4. Verify fresh installation and upgrade on clean Linux and macOS Homebrew environments.
5. Publish the tested tap command and operational recovery steps.

# macOS runner setup

Choose the shell executor for the simplest setup on a dedicated Mac. Use the
Parallels executor when the host is also used for other work and each CI job
needs an isolated macOS VM. Parallels Desktop requires a separate license, and
GitLab classifies the Parallels executor as maintenance mode. See GitLab's
[executor comparison](https://docs.gitlab.com/runner/executors/).

## Shell executor

Use a dedicated Mac or a dedicated macOS account without personal files,
credentials, or SSH keys. The shell executor runs CI commands directly as that
user. The supported macOS service is a per-user `LaunchAgent`, so the runner
account must remain logged in. Keep the Mac connected to power and prevent it
from sleeping while jobs may run. See GitLab's [macOS installation
notes](https://docs.gitlab.com/runner/install/osx.html).

### Create the project runner

In `research-enablement/homebrew-xylem`, open **Settings > CI/CD > Runners** and
select **Create project runner**. Configure the runner as follows:

- Select macOS and add the `macos` tag.
- Disable untagged jobs.
- Use a description that identifies the host, such as `Homebrew MacBook`.
- Leave **Protected** disabled for the current merge-request pipeline. A
  protected runner requires both merge-request branches and protected-resource
  access to be configured separately.
- Set the maximum timeout to at least 60 minutes.

Create the runner and copy its short-lived `glrt-...` authentication token. Do
not save the token in the repository or CI configuration. GitLab documents this
flow under [create a project
runner](https://docs.gitlab.com/ci/runners/runners_scope/#create-a-project-runner-with-a-runner-authentication-token).

### Prepare the Mac

Log in as the runner account. Install Homebrew first if it is not already
available to that account, then run:

```shell
xcode-select --install
brew install gitlab-runner
chsh -s /bin/bash
```

Log out and back in after changing the shell.

### Register and start the runner

Use the command shown by GitLab after creating the runner:

```shell
gitlab-runner register \
  --url "https://code.ornl.gov" \
  --token "glrt-REDACTED"
```

Choose `shell` when prompted for the executor. In the new `[[runners]]` entry in
`~/.gitlab-runner/config.toml`, set the service path for the Mac's architecture.

Apple Silicon:

```toml
environment = ["PATH=/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"]
```

Intel:

```toml
environment = ["PATH=/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin"]
```

Start the service and confirm that the registration is valid:

```shell
brew services start gitlab-runner
gitlab-runner verify
brew services list
```

The runner must appear online in the tap project's Runners page with the
`macos` tag.

## Parallels executor

The Parallels executor keeps the runner manager on the MacBook but executes
each job in a clean clone of a macOS base VM. It connects to the guest over SSH.
Local runner caches are not supported, so do not depend on a persistent
Homebrew cache between jobs. See GitLab's [Parallels executor
documentation](https://docs.gitlab.com/runner/executors/parallels/).

### Prepare the host and base VM

Install and activate Parallels Desktop on the host. Install GitLab Runner in
the dedicated host account that will manage the VMs:

```shell
brew install gitlab-runner
ssh-keygen -t ecdsa -b 521 -f ~/.ssh/gitlab-runner-parallels -N ""
```

The host account must remain logged in because the Homebrew service runs as a
per-user `LaunchAgent`. Keep the host connected to power and prevent sleep.

Create a macOS VM named `homebrew-xylem-base` with the same CPU architecture as
the host. Allocate enough storage for Xcode Command Line Tools, Homebrew, Rust,
and temporary build files. In the guest:

1. Create a standard account named `ci-runner` without personal credentials.
2. Enable **System Settings > General > Sharing > Remote Login** for that
   account.
3. Install Xcode Command Line Tools and Homebrew.
4. Confirm that `bash`, `brew`, `git`, and `ssh` are available.
5. Add a dedicated host-side SSH public key to
   `/Users/ci-runner/.ssh/authorized_keys`.

From the host account, verify that the private key can connect to the running
guest without a password. The private key must not have a passphrase because
GitLab Runner cannot prompt for one:

```shell
ssh -i ~/.ssh/gitlab-runner-parallels ci-runner@VM_IP_ADDRESS true
```

Shut down the base VM after verification. Do not use the base VM interactively
while the runner is active; update its tools only while no CI job is running,
then shut it down again.

### Register and configure the Parallels runner

Create a project runner using the settings under **Create the project runner**
above. Use a distinct description such as `Homebrew MacBook Parallels`, retain
the `macos` tag, and copy the new `glrt-...` token. Register it from the host:

```shell
gitlab-runner register \
  --url "https://code.ornl.gov" \
  --token "glrt-REDACTED"
```

Choose `parallels` as the executor, enter `homebrew-xylem-base` as the base VM,
and supply `ci-runner` and the host path to the dedicated SSH private key when
prompted. Leave the SSH password empty.

Review the new `[[runners]]` entry in `~/.gitlab-runner/config.toml`. Preserve
the generated URL and token and make its executor-specific settings match this
example:

```toml
limit = 1
executor = "parallels"
environment = ["PATH=/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"]

[runners.parallels]
  base_name = "homebrew-xylem-base"
  disable_snapshots = false

[runners.ssh]
  user = "ci-runner"
  port = "22"
  identity_file = "/Users/HOST_RUNNER_ACCOUNT/.ssh/gitlab-runner-parallels"
```

For an Intel guest, put `/usr/local/bin` and `/usr/local/sbin` first in the
configured `PATH`. Do not add `allowed_images`; omitting it prevents a CI job
from selecting an arbitrary local VM. The `[runners.parallels]` and
`[runners.ssh]` fields are documented in GitLab's [advanced runner
configuration](https://docs.gitlab.com/runner/configuration/advanced-configuration/#the-runnersparallels-section).

Start the host service and verify the registration:

```shell
brew services start gitlab-runner
gitlab-runner verify
brew services list
```

The runner must appear online with the `macos` tag. Run the first CI job while
watching Parallels Desktop and confirm that the runner creates a clone, runs the
job inside it, and leaves the base VM unchanged. If the job cannot upload
artifacts, install the same GitLab Runner version inside the base VM; GitLab
requires the guest binary for artifact transfer with VM executors.

## Retry and require macOS validation

Retry the failed v1.0.3 job from GitLab or run:

```shell
glab api -X POST projects/22654/jobs/4378427/retry
```

After the runner has completed a job successfully, remove `when: manual` and
`allow_failure: true` from `homebrew-macos` in
`scripts/homebrew/tap.gitlab-ci.yml` and from the tap merge request's
`.gitlab-ci.yml`. Require the resulting pipeline before merge.

# Validation

- Confirm the protected credential is unavailable to branch and merge-request pipelines.
- Confirm an incomplete or failed release cannot open or update the tap merge request.
- Install from the rendered formula in clean macOS and Linux Homebrew environments and run the formula test offline.
- Upgrade from the previous formula version and verify that `brew upgrade xylem` selects the new immutable release.

# Acceptance criteria

- A documented `brew install <tap>/xylem` command installs the tagged Xylem release on every declared Homebrew platform.
- Audit, style, install, and formula tests are required before the tap change can merge.
- The v1.0.3 tap update passes both platform jobs and merges through review.
- The publication credential is protected and scoped only to the tap update operation.

# Related plans
