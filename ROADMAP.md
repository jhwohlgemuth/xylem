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
- [x] Add parameterized URL allowlists and remote reachability checks
- [x] Add progress indication for long-running workflows
- [x] Add `--env` for selecting a custom dotenv file

## v1.x interoperability and portability

- [ ] Publish rigorously tested macOS and Windows binaries
- [ ] Export configurations to CWL
- [ ] Provide a built-in API server for tenant modules
- [ ] Provide portable, offline containerized dependencies
- [ ] Add WebAssembly module and browser execution support
- [ ] Evaluate a TIFF viewer as a separate application or optional capability

## v2.0

- [ ] Generate workflow graphs (`config.json` to an image)
- [ ] Support the `<%= value %>` `ScriptTemplate` format
- [ ] Visual workflow creation and editing
