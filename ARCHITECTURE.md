# Architecture
> This file describes the high-level architecture and some specific design decisions of PIPE

## Components
- `pipe-cli` (CLI Application)
  - Depends on `pipe-lib`
  - Depends on [clap](https://docs.rs/clap/latest/clap/) crate for CLI arguments
  - Contains code for CLI [commands](./pipe-cli/src/commands/) (e.g., `run`, `check`, etc...) and [reporters](./pipe-cli/src/reporters/)
- `pipe-lib` (Rust code)
  - Utility functions
  - Code for handling user input, ingesting configuration data, running workflows, etc...
- `pipe-py` (Python library)
  - 🚧 Under construction
  - Export Rust code as Python library using [PyO3/Maturin](https://github.com/PyO3/maturin)
  - Export select `pipe-lib` functions as Python functions

## Diagram
```mermaid
graph LR
    a[PIPE] --> b[pipe-cli/]
    a --> c[pipe-lib/]
    a --> d[pipe-py/]
    b --> e[CLI application]
    c --> f[Rust crate]
    d --> g[Python</br>package]
    e --> h[commands]
    e --> i[reporters]
    f --> j[pipe-cli</br>utility functions]
    g --> k[Module</br>utility functions]
```

## Notes
- Tests are written for modules in adjacent `tests.rs` files within the associated module directory
- Generated documentation is published to [https://pipe.ornl.gov](https://pipe.ornl.gov)