```
    ███████  ██  ███████  ████▓▒░
    ██   ██  ██  ██   ██  ██
░▒▓██████████████████████████▓▒░
    ██       ██  ██       ██
    ██       ██  ██       ████▓▒░
Parallel Integration & Processing Engine
```

# PIPE Command
> Parallel Integration and Processing Engine (PIPE) command line (multi)tool for sustainable science

## Getting Started
> **UNDER CONSTRUCTION**

## Documentation
> **TIP** Read the code docs at https://pipe.code.ornl and/or explore the [Wiki](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/home)

Print the help with `pipe help` or save the configuration below to `config.json` and use `pipe run`

```json
{
    "version": "0.0.1",
    "description": "Print PIPE help",
    "requirements": [],
    "modules": [
        {
            "name": "Meta Module",
            "type": "binary",
            "uri": "https://code.ornl.gov/api/v4/projects/15383/packages/generic/x86_64-unknown-linux-gnu/v0.0.3/pipe",
            "checksum": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "template": {
                "command": "pipe",
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