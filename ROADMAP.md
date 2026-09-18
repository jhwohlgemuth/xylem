# Roadmap

Xylem 1.0.0 is released. Earlier `v0.1` and `v0.2` headings described planning
phases rather than the versions ultimately published, so the roadmap now uses
the released 1.0 baseline and forward-looking 1.x/2.0 milestones.

## v1.0 baseline — released

- [x] Modular CLI and JSON/YAML workflow configuration
- [x] Type-safe Rust configuration API
- [x] Workflow and module-scoped variables
- [x] Local and remote script modules
- [x] Local and remote binary modules with SHA-256 verification
- [x] Concurrent workflow inputs and parallel module preparation
- [x] Conda/Mamba, Docker/Podman, and Apptainer integration
- [x] Offline, dry-run, and requirement/checksum bypass options
- [x] Linux release binaries

## v1.x module reliability

- [x] Apply `{{ VARIABLE }}` interpolation inside variable values
- [x] Expose reserved workflow values, including `MODULE_LIST`
- [x] Pass resolved, namespaced workflow and module values to child processes
- [x] Add automated end-to-end test modules
- [x] Publish the stable [module specification](./MODULE_SPEC.md)
- [x] Create Pixi environments from `pixi.toml` or Pixi-enabled `pyproject.toml`
- [x] Add a runnable [Python with Pixi example](./examples/python-with-pixi/README.md)
- [ ] Add parameterized URL allowlists and remote reachability checks
- [ ] Complete the `check` subcommand configuration and module validation
- [ ] Add the `doctor` subcommand with environment diagnosis and `--fix`
- [ ] Add progress indication for long-running workflows
- [ ] Add `--env` for selecting a custom dotenv file

## v1.x interoperability and portability

- [ ] Export configurations to CWL
- [ ] Export configurations to RO-Crate
- [ ] Publish rigorously tested macOS and Windows binaries
- [ ] Provide portable, offline containerized dependencies
- [ ] Generate workflow graphs (`config.json` to an image)
- [ ] Add WebAssembly module and browser execution support
- [ ] Provide a built-in API server for tenant modules
- [ ] Evaluate a TIFF viewer as a separate application or optional capability

## v2.0

- [ ] Visual workflow creation and editing
- [ ] Support the `<%= value %>` `ScriptTemplate` format
