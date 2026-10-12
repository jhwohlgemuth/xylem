# Getting started

## Install with Homebrew

On macOS or Linux, add the ORNL tap and install Xylem:

```shell
brew tap research-enablement/xylem https://code.ornl.gov/research-enablement/homebrew-xylem.git
brew install xylem
xylem --version
```

## Install with Cargo

Install the published command-line crate:

```shell
cargo install --locked xylem-cli
xylem --version
```

## Install a Linux release

Download the Debian package and matching checksum for your architecture from
the [Xylem releases](https://code.ornl.gov/research-enablement/xylem/-/releases).
Verify and install an amd64 package with:

```shell
sha256sum --check xylem_<VERSION>-1_amd64.deb.sha256
sudo apt install ./xylem_<VERSION>-1_amd64.deb
xylem --version
```

ARM64 Debian packages use the `arm64` suffix. AppImages use `x86_64` or
`aarch64`. Verify and launch an x86_64 AppImage with:

```shell
sha256sum --check xylem-v<VERSION>-x86_64.AppImage.sha256
chmod +x xylem-v<VERSION>-x86_64.AppImage
./xylem-v<VERSION>-x86_64.AppImage
```

Launching an AppImage without arguments opens the Xylem dashboard. Command-line
arguments such as `--version`, `--help`, and `run` are forwarded normally. The
release also includes the standalone static x86_64 Linux binary.

## Build from source

Xylem requires the Rust toolchain declared by the workspace:

```shell
git clone https://code.ornl.gov/research-enablement/xylem.git
cd xylem
cargo install --locked --path crates/xylem-cli
xylem --version
```

## Run a workflow

Run a configuration with informational logging:

```shell
xylem -vv run path/to/config.json
```

Add one or more inputs at the command line when the configuration does not
declare them:

```shell
xylem --threads 4 -vv run path/to/config.json --input first,second
```

Keep a crash-recoverable execution journal and a final JSON summary together:

```shell
xylem run path/to/config.json \
  --reporter console,json \
  --output ./xylem-run
```

The output directory will contain `<run-id>.journal.jsonl` and
`<run-id>.json`. The journal records every durable state transition; the JSON
file is the deterministic final report reconstructed from it.

Use `xylem --help` and `xylem run --help` for the exact options supported by
the installed version.
