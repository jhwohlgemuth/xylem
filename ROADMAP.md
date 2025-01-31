# Roadmap

## v0.1.0<sup> [2](#2)</sup>
- [X] Basic CLI application interface with modular subcommand architecture
- [X] Configuration schema
    - [X] Add `resources` attribute in addition to `requirements` to designate physical requirements (*example*: GPU(s))
    - [X] Add `skip` attribute for skipping various aspects of workflow (e.g., checksums, verification, etc...)
- [X] Load configuration ([JSON](https://www.json.org/json-en.html))
- [X] Load configuration ([YAML](https://yaml.org/))
- [X] Type-safe programmatic API for using configuration data
- [X] Load environment variables from `.env` file
- [X] Robust and configurable logging using [tracing crate](https://docs.rs/tracing/latest/tracing/)
- [X] Namespaced workflow environment variables
- [X] Namespaced module environment variables
- [X] Validate environment requirements from list of string names
- [X] Programmatic wrapper for executing arbitrary shell commands
  - [X] Provide enhanced logging and troubleshooting
- [X] Template abstraction for customizing module-to-shell interface (see `ScriptTemplate` struct)
- [X] Tests with [code coverage](https://github.com/xd009642/tarpaulin)
- [X] Documentation generated from code comments (see https://pipe.ornl.gov)
- [X] Create module template to enable community contributions
- [X] MVP<sup>[1](#1)</sup> (script module support)
  - [X] Support local script module (*example*: `ssh://git@code.ornl.gov/project/name.git`)
  - [X] Support remote script module (using `git clone <URI>`)
  - [X] Auto-create (Conda/[Mamba](https://mamba.readthedocs.io/en/latest/index.html)) virtual environments
  - [X] Parallelize module preparation
- [X] URL security
  - [X] Provide `--offline` flag to disable reachability check
  - [X] Filter remote module URLs against white list
- [X] MVP<sup>[1](#1)</sup> (+binary module support)
    - [X] Verify binary with checksum hash (provide `--skip-verify-checksum` flag)
    - [X] Handle binary execution when not executable (`./cmd` vs `cmd`)
    - [X] Support local binary module
    - [X] Support remote binary module
- [X] Multi-platform release binaries published to GitLab repository
  - [X] Linux ([tier 1](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [X] Mac ([tier 2](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
- [X] Abstract "virtual environment" support (to include more than just `Conda`)
    - [X] Add Docker (and Podman) environment
    - [X] Add Apptainer environment
- [X] `--skip-verify-requirements` flag for running workflow without verifying environment
- [x] Workflow input concurrency
- [ ] Apply `{{ }}` string interpolation to values defined in `variable` attributes
- [ ] Set workflow environment variables (e.g., `MODULE_LIST`)

## v0.2.0
- [ ] TIFF Viewer capability (part of pipe and/or separate app)
- [ ] Test modules for automated end-to-end testing
- [ ] Enhanced URL security
  - [ ] Check remote URLs are valid and reachable
- [ ] Configuration interoperability and export
  - [ ] Export configuration to [CWL](https://www.commonwl.org/)
  - [ ] Export configuration to [RO-Crate](https://www.researchobject.org/ro-crate/)
- [ ] Implement module conventions (documentation and existing modules)
  - [X] Finalize "run" function function signature
    ```rust
    fn(input_directory: PathBuf, output_directory: PathBuf, options: Option<Options>)
    ```
  - [X] Folder/file structure
  - [ ] Draft module specification
- [ ] Add Pixi virtual environment support (run support is in place, still need to add support for virtual environment creation)
- [ ] "Check" subcommand
    - [X] Create standardized function signature for check modules
        ```rust
        fn(&Path, Option<Parameters>) -> Vec<Option<Details>>
        ```
    - [ ] Configure checks using `config.{json|yaml}`
    - [ ] Validate modules using check subcommand and associated `is_valid_python_module` check
- [ ] "Doctor" subcommand
  - [ ] Provide analysis of environment capabilities
    - [ ] Git details (`user.name` and `user.email`)
    - [ ] Architecture and OS
    - [ ] Hardware (e.g., GPUs, etc...)
  - [ ] Provide user steps to address configuration issues
  - [ ] Install missing requirements when `--fix` is passed

## v1.0.0
- [ ] MVP<sup>[1](#1)</sup> (+[WebAssembly](https://webassembly.org/) module support)
  - [ ] Create example WebAssembly module
  - [ ] Add workflow support
  - [ ] Create "utility" module(s) (*example*: data loader module)
- [ ] Multi-platform release binaries published to GitLab repository
  - [X] Linux ([tier 1](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Mac ([tier 2](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Windows ([tier 2](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Tiers-of-Support))
  - [ ] Web Browser (WebAssembly)
- [ ] User experience enhancements
  - [ ] Progress indication during long running workflows
  - [ ] Provide `--env` option to specify custom location of `.env` file
- [ ] Enhanced requirement validation using namespaced strings (*example*: `conda::numpy`)
- [ ] Provide built-in API server to serve requests to tenant modules (data from an API will be listed as requirements)
- [ ] Provide portable containerized dependencies that can be used offline (and automatically added via `pipe doctor --fix`)
- [ ] Support creating Python virtual environments using [Pixi](https://pixi.sh/latest/) (which is more performant and arguably better in every way)
- [ ] Workflow graph generation, `config.json` -> `graph.png`

## v2.0.0
- [ ] Visual programming interface for workflow creation/update
- [ ] Support `<%= value %>` `ScriptTemplate` format


-------------

**Footnotes**
-------------

[1]
---
> **M**inimum **V**iable **P**roduct - a product or service that has just enough core features to effectively deploy it to a select set of customers and early adopters, who can give direct and valuable feedback while it’s still early and inexpensive to make changes ([source](https://www.leansixsigmadefinition.com/glossary/minimum-viable-product/))


[2]
---
> Version with features as described in paper submitted to IEEE Big Data