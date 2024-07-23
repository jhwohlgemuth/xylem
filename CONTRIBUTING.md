# Contributing
## Setup Development Environment
- Ensure [Docker](https://www.docker.com/products/docker-desktop/) is installed
- Start a container using
    ```docker
    docker run -it --name <NAME> -p 1337:1337 ghcr.io/jhwohlgemuth/rust
    ```
- Within your container, clone the project <sup>[1](#1)</sup> with
    ```shell
    git clone git@code.ornl.gov:GSHS/common/pipe/command.git
    ```
- Ensure Docker container is running, then chose one of more of the following:
    - Open a web browser <sup>[2](#2)</sup> and navigate to `https://localhost:1337` (password = `password`)
    - Open local install of VS Code and connect to container using [remote development extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.vscode-remote-extensionpack)
- Development involves mulitple [make](./Makefile) and cargo tasks:
    - `cargo run` - Execute code
        - `cargo run -- help` - Print PIPE CLI help
        - `cargo run -- check --help` - Print check command CLI help
        - `cargo run -- run --config /path/to/config.json --dry-run -vvv` - Run workflow dry run with logging
    - `make lint` -  Format and lint code
    - `make test` - Run tests and generate code coverage
    - `make doc` - Generate documentation (see https://pipe.ornl.gov)
    - ***WIP*** `make release` - Build release binaries
- Make a git branch, push changes to it, open a merge request, resolve issues

---

[1]
---
> To setup SSH, please see the [Setup SSH wiki page](https://code.ornl.gov/groups/geoai/common/-/wikis/Setup-SSH)

> **Note**:
> You can copy your local SSH key with `docker cp /path/to/.ssh/id_ed25519 <NAME>:/root/.ssh` and run `fix_ssh_key_permissions` within your container to get setup without making a new key pair

[2]
---
> Firefox is strongly recommended