```
░  ░░░░  ░░  ░░░░  ░░  ░░░░░░░░        ░░  ░░░░  ░
▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒  ▒▒▒▒▒▒▒▒  ▒▒▒▒▒▒▒▒   ▒▒   ▒
▓▓▓    ▓▓▓▓▓▓    ▓▓▓▓  ▓▓▓▓▓▓▓▓      ▓▓▓▓        ▓
██  ██  ██████  █████  ████████  ████████  █  █  █
█  ████  █████  █████        ██        ██  ████  █

```

# Xylem
> "Digital xylem" that transports data through workflows with structure and support

## Installation
> Xylem is currently only tested rigorously on Linux, full Windows support is coming soon

### Install with cargo
> 🚧 Under Construction

### Download pre-compiled binary
- Download newest binary from [releases page](https://code.ornl.gov/research-enablement/xylem/-/releases)
    ```shell
    curl -LO https://code.ornl.gov/api/v4/projects/15383/packages/generic/x86_64-unknown-linux-musl/v0.0.11/xylem
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
> **TIP** Read the code docs at https://xylem.code.ornl and/or explore the [Wiki](https://code.ornl.gov/research-enablement/xylem/-/wikis/home)

- See [xylem.ornl.gov](https://xylem.ornl.gov) to see generated code documentation
- See example configurations in the [`/examples`](./examples/) folder

### Architecture
> See [ARCHITECTURE.md](./ARCHITECTURE.md)

## Roadmap
> See [ROADMAP.md](./ROADMAP.md)

## Contributing
> See [CONTRIBUTING.md](./CONTRIBUTING.md)


-------------