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
> Xylem is currently only tested rigorously on Linux, full Windows support is coming soon

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
- Download newest binary from [releases page](https://code.ornl.gov/research-enablement/xylem/-/releases)
    ```shell
    curl -LO https://code.ornl.gov/api/v4/projects/15383/packages/generic/x86_64-unknown-linux-musl/v1.0.2/xylem
    ```
- Make binary executable
    ```shell
    chmod +x ./xylem
    ```
- Move binary to folder on path (ex. `/usr/local/bin`)
    ```shell
    mv xylem /usr/local/bin
    ```
- Verify installation with `xylem help`

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
  xylem help
  ```

## Documentation
> **TIP** Read the Xylem book at https://xylem.ornl.gov and/or explore the [Wiki](https://code.ornl.gov/research-enablement/xylem/-/wikis/home)

- See the [Xylem book](https://xylem.ornl.gov) for installation, configuration, and module guidance
- See example configurations in the [`/examples`](./examples/) folder
- Run the self-contained [Python with Pixi example](./examples/python-with-pixi/README.md) or the [Python with Conda example](./examples/python-with-conda/README.md) with `cargo run --example python-with-pixi` and `cargo run --example python-with-conda`
- Read the versioned [Xylem module specification](./MODULE_SPEC.md)

### Architecture
> See [ARCHITECTURE.md](./ARCHITECTURE.md)

## Roadmap
> See [ROADMAP.md](./ROADMAP.md)

## Contributing
> See [CONTRIBUTING.md](./CONTRIBUTING.md)


-------------
