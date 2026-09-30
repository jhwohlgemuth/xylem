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

The supported release target is static Linux (`x86_64-unknown-linux-musl`).
Homebrew is tested on Linux and macOS. Windows builds are published for early
use, but rigorous macOS and Windows support remains on the roadmap.

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

### Download pre-compiled binary
- Download the Linux binary attached to the newest version on the
  [releases page](https://code.ornl.gov/research-enablement/xylem/-/releases).
- Make binary executable
    ```shell
    chmod +x ./xylem
    ```
- Move binary to folder on path (ex. `/usr/local/bin`)
    ```shell
    mv xylem /usr/local/bin
    ```
- Verify installation with `xylem --version` and `xylem --help`

### Build from source
- Clone this project and navigate to it
  ```shell
  git clone https://code.ornl.gov/research-enablement/xylem
  cd ./xylem
  ```

- Install `xylem` command
  ```shell
  cargo install --path ./xylem-cli
  ```

- Test the installation
  ```shell
  xylem --version
  xylem --help
  ```

## Documentation
> **TIP** Read the Xylem book at https://xylem.ornl.gov and/or explore the [Wiki](https://code.ornl.gov/research-enablement/xylem/-/wikis/home)

- See the [Xylem book](https://xylem.ornl.gov) for installation, configuration, and module guidance
- Use the [workflow configuration reference](./docs/src/configuration.md) for
  the current JSON and YAML contract
- See the supported paths and dependency tiers in the [`examples`](./examples/) catalog
- Start with the self-contained [offline smoke example](./examples/offline-smoke/README.md),
  which needs only a POSIX shell
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
