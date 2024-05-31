# Contributing
## Setup Development Environment
- Start a container using
    ```docker
    docker run -it --name <NAME> -p 1337:1337 ghcr.io/jhwohlgemuth/rust
    ```
- Within your container, clone the project <sup>[1](#1)</sup> with
    ```bash
    git clone git@code.ornl.gov:GSHS/common/PIPE/command.git
    ```
- Development involves mulitple tasks:
    - `cargo run` - Execute code
        - `cargo run -- help` - Print PIPE CLI help
        - `cargo run -- check --help` - Print check command CLI help
    - `cargo fmt` -  Format code
    - `cargo clippy` - Identify code issues that should be addressed
    - `cargo test` - Run tests

---

[1]
---
> To setup SSH, please see the [Setup SSH wiki page](https://code.ornl.gov/groups/geoai/common/-/wikis/Setup-SSH)