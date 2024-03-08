Midas <sup>[1](#1)</sup>
======
> Command line (multi)tool for sustainable science


Getting Started
---------------
> **UNDER CONSTRUCTION**

Validation Rules ("checks")
---------------------------
> **UNDER CONSTRUCTION**

Contributing
------------
### Setup Development Environment
- Start a container using
    ```docker
    docker run -it --name <NAME> ghcr.io/jhwohlgemuth/rust
    ```
- Within your container, clone the Midas project <sup>[2](#2)</sup> with
    ```bash
    git clone git@code.ornl.gov:GSHS/common/midas/core.git
    ```
- Midas development involves mulitple tasks:
    - `cargo run` - Execute Midas code
        - `cargo run -- help` - Print Midas CLI help
        - `cargo run -- check --help` - Print Midas check command CLI help
    - `cargo fmt` -  Format code
    - `cargo clippy` - Identify code issues that should be addressed
    - `cargo test` - Run tests

---

[1]
---
> As in [King Midas](https://en.wikipedia.org/wiki/Midas). Just as everything King Midas touched turned to gold, so shall software touched by this tool ;)

[2]
---
> To setup SSH, please see the [Setup SSH wiki page](https://code.ornl.gov/groups/geoai/common/-/wikis/Setup-SSH)