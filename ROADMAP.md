# Roadmap

- [X] Basic CLI application interface with modular subcommand architecture
- [X] Configuration schema
    ```rust
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct Config {
        /// Configuration version in semver format
        pub version: String,
        #[serde(alias = "doc")]
        pub description: String,
        pub requirements: Vec<String>,
        /// Keywords or tags used to describe this module
        pub keywords: Option<Vec<String>>,
        pub modules: Vec<Module>,
        pub variables: Option<Map<String, Value>>,
    }
    ```
- [X] Load configuration ([JSON](https://www.json.org/json-en.html))
  - [ ] Load configuration ([YAML](https://yaml.org/))
  - [ ] Load configuration ([CWL](https://www.commonwl.org/))
- [X] Type-safe programmatic API for using configuration data
- [X] Load environment variables from `.env` file
- [X] Robust and configurable logging using [tracing crate](https://docs.rs/tracing/latest/tracing/)
- [X] Namespaced workflow environment variables
- [X] Namespaced module environment variables
- [X] Validate environment requirements from list of string names
- [X] Programmatic wrapper for executing arbitrary shell commands
  - [ ] Provide enhanced logging and troubleshooting
- [X] Template abstraction for customizing module-to-shell interface (see `ScriptTemplate` struct)
  - [ ] Support `<%= value %>` templating format
- [X] Tests with [code coverage](https://github.com/xd009642/tarpaulin)
- [X] Documentation generated from code comments (see https://pipe.ornl.gov)
- [X] Create module template to enable community contributions
- [X] MVP<sup>[1](#1)</sup> (script module support)
  - [X] Support local script module (*example*: `ssh://git@code.ornl.gov/project/name.git`)
  - [X] Support remote script module (using `git clone <URI>`)
  - [X] Auto-create (Conda/[Mamba](https://mamba.readthedocs.io/en/latest/index.html)) virtual environments
  - [X] Parallelize module preparation
- [ ] Multi-platform release binaries published to GitLab repository
  - [ ] Linux
  - [ ] Mac
  - [ ] Windows
  - [ ] Web Browser (WebAssembly)
- [ ] MVP<sup>[1](#1)</sup> (+binary module support)
    - [ ] Support local binary module
    - [ ] Support remote binary module
    - [ ] Verify binary with checksum hash (provide `--skip-verify-checksum` flag)
- [ ] `--skip-verify-requirements` flag for running workflow without verifying environment
- [ ] "Check" subcommand
    - [X] Create standardized function signature for check modules
        ```rust
        fn(&Path, Option<Parameters>) -> Vec<Option<Details>>
        ```
    - [ ] Validate modules using check subcommand and associated `is_valid_python_module` check
- [ ] "Doctor" subcommand
  - [ ] Provide analysis of environment capabilities
  - [ ] Provide steps to address configuration issues
  - [ ] Install missing requirements when `--fix` is passed
- [ ] MVP<sup>[1](#1)</sup> (+[WebAssembly](https://webassembly.org/) module support)
  - [ ] Create example WebAssembly module
  - [ ] Add workflow support
  - [ ] Create "utility" module(s) (*example*: data loader module)
- [ ] Enhanced requirement validation using namespaced strings (*example*: `conda::numpy`)
- [ ] Provide portable containerized dependencies that can be used offline (and automatically added via `pipe doctor --fix`)
- [ ] Support creating Python virtual environments using [Pixi](https://pixi.sh/latest/) (which is more performant and arguably better in every way)
- [ ] Visual programming interface for workflow creation/update


-------------

**Footnotes**
-------------

[1]
---
> **M**inimum **V**iable **P**roduct - a product or service that has just enough core features to effectively deploy it to a select set of customers and early adopters, who can give direct and valuable feedback while it’s still early and inexpensive to make changes ([source](https://www.leansixsigmadefinition.com/glossary/minimum-viable-product/))