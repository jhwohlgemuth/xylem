```
░  ░░░░  ░░  ░░░░  ░░  ░░░░░░░░        ░░  ░░░░  ░
▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒  ▒▒▒▒▒▒▒▒  ▒▒▒▒▒▒▒▒   ▒▒   ▒
▓▓▓    ▓▓▓▓▓▓    ▓▓▓▓  ▓▓▓▓▓▓▓▓      ▓▓▓▓        ▓
██  ██  ██████  █████  ████████  ████████  █  █  █
█  ████  █████  █████        ██        ██  ████  █

```

# Xylem
> Digital xylem that transports data through workflows with structure and support

## Installation

The supported release targets are Linux x86_64 and ARM64 Debian packages and
AppImages, plus a standalone static x86_64 binary. Homebrew is tested on Linux
and macOS. Windows builds are published for early use, but rigorous macOS and
Windows support remains on the roadmap.

### Install with Homebrew

Add the ORNL Xylem tap, then install the formula:

```shell
brew tap research-enablement/xylem https://code.ornl.gov/research-enablement/homebrew-xylem.git
brew install xylem
```

### Install with cargo

Install the published command-line crate:

```shell
cargo install --locked xylem-cli
```

### Install a Linux release

Download the artifact and matching `.sha256` file for your architecture from
the [releases page](https://code.ornl.gov/research-enablement/xylem/-/releases),
then verify it before installation:

```shell
sha256sum --check xylem_<VERSION>-1_amd64.deb.sha256
sudo apt install ./xylem_<VERSION>-1_amd64.deb
xylem --version
```

ARM64 Debian packages use the `arm64` suffix. AppImages use `x86_64` or
`aarch64`; they are portable and open the Xylem dashboard when launched without
arguments:

```shell
sha256sum --check xylem-v<VERSION>-x86_64.AppImage.sha256
chmod +x xylem-v<VERSION>-x86_64.AppImage
./xylem-v<VERSION>-x86_64.AppImage
```

The release also includes the standalone static x86_64 Linux binary.

### Build from source
- Clone this project and navigate to it
  ```shell
  git clone https://code.ornl.gov/research-enablement/xylem
  cd ./xylem
  ```

- Install `xylem` command
  ```shell
  cargo install --path ./crates/xylem-cli
  ```

- Test the installation
  ```shell
  xylem --version
  xylem --help
  ```

## Run workflows and open the dashboard

Pass a JSON, JSONC, YAML, or YML workflow as the first argument:

```shell
xylem run path/to/workflow.json
xylem tui path/to/workflow.json
```

When the argument is omitted, Xylem discovers `.xylem.json`,
`.xylem.jsonc`, `.xylem.yaml`, then `.xylem.yml`. The dashboard can also open
without a configuration to inspect retained run history. Every executed run
writes its journal, metrics, summary, and attributed logs to the platform
Xylem cache.

## Documentation
> **TIP** Read the Xylem book at https://xylem.ornl.gov and/or explore the [Wiki](https://code.ornl.gov/research-enablement/xylem/-/wikis/home)

- See the [Xylem book](https://xylem.ornl.gov) for installation, configuration, and module guidance
- Use the [workflow configuration reference](./docs/src/configuration.md) for
  the current JSON, strict JSONC, and YAML contract
- See the supported paths and dependency tiers in the [`examples`](./examples/) catalog
- Start with the self-contained [offline smoke example](./examples/offline-smoke/README.md),
  which needs only a POSIX shell
- Inspect intentional failures and their logs with the shell-only
  [failure logging example](./examples/failure-logging/README.md)
- Compare failure admission behavior with the shell-only
  [failure policy example](./examples/failure-policy/README.md)
- Run the broader [kitchensink example](./examples/kitchensink/README.md),
  [multi-module example](./examples/multi-module/README.md),
  [Python with Pixi example](./examples/python-with-pixi/README.md), or
  [Python with Conda example](./examples/python-with-conda/README.md) with its
  corresponding `cargo run --example <name>` command after installing the
  dependencies listed in that example's README
- Read the versioned [Xylem module specification](./MODULE_SPEC.md)

### Architecture
> See [ARCHITECTURE.md](./ARCHITECTURE.md)

## Roadmap
> See [ROADMAP.md](./ROADMAP.md)

## Contributing
> See [CONTRIBUTING.md](./CONTRIBUTING.md)


-------------
