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

## Download a Linux release

Download the current Linux binary from the
[Xylem releases](https://code.ornl.gov/research-enablement/xylem/-/releases),
make it executable, and place it on `PATH`:

```shell
chmod +x ./xylem
sudo mv ./xylem /usr/local/bin/xylem
xylem --version
```

## Build from source

Xylem requires the Rust toolchain declared by the workspace:

```shell
git clone https://code.ornl.gov/research-enablement/xylem.git
cd xylem
cargo install --locked --path xylem-cli
xylem --version
```

## Run a workflow

Run a configuration with informational logging:

```shell
xylem -vv run --config path/to/config.json
```

Add one or more inputs at the command line when the configuration does not
declare them:

```shell
xylem --threads 4 -vv run --config path/to/config.json --input first,second
```

Use `xylem --help` and `xylem run --help` for the exact options supported by
the installed version.
