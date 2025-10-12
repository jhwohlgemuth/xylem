```
    ███████  ██  ███████  ████▓▒░
    ██   ██  ██  ██   ██  ██
░▒▓██████████████████████████▓▒░
    ██       ██  ██       ██
    ██       ██  ██       ████▓▒░
Parallel Integration & Processing Engine
```

# Xylem
> Parallel Integration and Processing Engine (Xylem) command line (multi)tool for sustainable science

## Installation
> Xylem is currently only tested rigorously on Linux, full Windows support is coming soon

- Download newest binary from [releases page](https://code.ornl.gov/research-enablement/xylem/-/releases)
    ```shell
    curl -LO https://code.ornl.gov/api/v4/projects/15383/packages/generic/x86_64-unknown-linux-gnu/v0.0.5/xylem
    ```
- Make binary executable
    ```shell
    chmod +x ./xylem
    ```
- Move binary to folder on path (ex. `/usr/local/bin`)
    ```shell
    mv xylem /usr/local/bin
    ```
- Verify installation with `xylem --version`


## Documentation
> **TIP** Read the code docs at https://xylem.code.ornl and/or explore the [Wiki](https://code.ornl.gov/groups/research-enablement/xylem/-/wikis/home)

Print the help with `xylem help` or save the configuration below to `config.json` and use `xylem run`

```json
{
    "version": "0.0.1",
    "description": "Print Xylem help",
    "requirements": [],
    "modules": [
        {
            "name": "Meta Module",
            "type": "binary",
            "uri": "https://code.ornl.gov/api/v4/projects/15383/packages/generic/x86_64-unknown-linux-gnu/v0.0.3/xylem",
            "checksum": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "template": {
                "command": "xylem",
                "arguments": [
                    "help"
                ]
            }
        }
    ]
}
```

### Architecture
> See [ARCHITECTURE.md](./ARCHITECTURE.md)

## Roadmap
> See [ROADMAP.md](./ROADMAP.md)

## Contributing
> See [CONTRIBUTING.md](./CONTRIBUTING.md)


-------------