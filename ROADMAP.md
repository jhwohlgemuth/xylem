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
        pub resources: Vec<String>,
        /// Keywords or tags used to describe this module
        pub keywords: Option<Vec<String>>,
        pub modules: Vec<Module>,
        pub variables: Option<Map<String, Value>>,
        pub check: Vec<Rule>,
    }
    ```
    - [X] Add `resources` attribute in addition to `requirements` to designate physical requirements (*example*: GPU(s))
    - [ ] Add `skip` attribute for skipping various aspects of workflow (e.g., checksums, verification, etc...)
- [X] Load configuration ([JSON](https://www.json.org/json-en.html))
- [X] Load configuration ([YAML](https://yaml.org/))
- [ ] Export configuration to [CWL](https://www.commonwl.org/)
- [ ] Export configuration to [RO-Crate](https://www.researchobject.org/ro-crate/)
- [X] Type-safe programmatic API for using configuration data
- [X] Load environment variables from `.env` file
- [X] Robust and configurable logging using [tracing crate](https://docs.rs/tracing/latest/tracing/)
- [X] Namespaced workflow environment variables
- [X] Namespaced module environment variables
- [X] Validate environment requirements from list of string names
- [X] Programmatic wrapper for executing arbitrary shell commands
  - [X] Provide enhanced logging and troubleshooting
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
- [ ] Enhanced URL security
  - [X] Provide `--offline` flag to disable reachability check
  - [ ] Filter remote module URLs against white list
  - [ ] Check remote URLs are valid and reachable
- [ ] Multi-platform release binaries published to GitLab repository
  - [X] Linux ([tier 1](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Mac ([tier 2](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Windows ([tier 2](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Web Browser (WebAssembly)
- [ ] MVP<sup>[1](#1)</sup> (+binary module support)
    - [ ] Verify binary with checksum hash (provide `--skip-verify-checksum` flag)
    - [ ] Support local binary module
    - [ ] Support remote binary module
- [ ] Abstract "virtual environment" support (to include more than just `Conda`)
    - [ ] Add Docker (and Podman) environment
    - [ ] Add Apptainer environment
    - [ ] Add Pixi environment
- [X] `--skip-verify-requirements` flag for running workflow without verifying environment
- [ ] "Check" subcommand
    - [X] Create standardized function signature for check modules
        ```rust
        fn(&Path, Option<Parameters>) -> Vec<Option<Details>>
        ```
    - [ ] Configure checks using `config.{json|yaml}`
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
- [ ] Provide built-in API server to serve requests to tenant modules (data from an API will be listed as requirements)
- [ ] Provide portable containerized dependencies that can be used offline (and automatically added via `pipe doctor --fix`)
- [ ] Support creating Python virtual environments using [Pixi](https://pixi.sh/latest/) (which is more performant and arguably better in every way)
- [ ] Workflow graph generation, `config.json` -> `graph.png`
- [ ] Visual programming interface for workflow creation/update


-------------

**Footnotes**
-------------

[1]
---
> **M**inimum **V**iable **P**roduct - a product or service that has just enough core features to effectively deploy it to a select set of customers and early adopters, who can give direct and valuable feedback while it’s still early and inexpensive to make changes ([source](https://www.leansixsigmadefinition.com/glossary/minimum-viable-product/))