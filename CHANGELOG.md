# Changelog

All notable changes to this project will be documented in this file.

## [unreleased]

### 🚀 Features

- Add toml library; minor tweak to EnvironmentValue method
- Add pyproject.toml parsing capability
- Make programming language optional and generic the default
- Set environment values; Add module list variable

### 🐛 Bug Fixes

- *(tests)* Remove conditional test

### 📚 Documentation

- Update ROADMAP to identify features for v0.1.0
- Add install instructions to README

### ⚙️ Miscellaneous Tasks

- Publish new version

## [0.0.5] - 2024-10-06

### 🚀 Features

- Refactor run functions for better maintainability and extendibility
- Initial input concurrency capability (needs testing)
- Add executable check to binary module usage
- Add URL whitelist check to remote module preparation

### ⚙️ Miscellaneous Tasks

- Update what triggers docs build

### Dev

- Update run conditions for CI

## [0.0.4] - 2024-10-04

### 🚀 Features

- Use get_parent; improve logging a bit
- Prepare code for executing binaries
- Use which-rs crate for Command::test and update associated test to pass on windows
- Implement container attribute for Commands; Pave way for non-conda virtual environment managers
- More virtual environment and container support
- Enforce template data loading order in run execution phase
- Add architecture document
- Add download_binary function
- Complete template resolution capability
- Add capability to download binaries
- Remove unimplemented in binary module piepline
- Final steps to run remote binary modules
- Initial type design for input concurrency
- Bump version and publish

### 🐛 Bug Fixes

- Make checksum attribute optional
- *(tests)* Make tests pass on Windows
- Remove test requirement from config.json fixture
- Remove typo
- Use exec instead of run for docker containers
- Create CI test task

### 📚 Documentation

- Update ROADMAP and CONTRIBUTING

### 🧪 Testing

- Add some CLI tests

## [0.0.3] - 2024-09-17

### 🚀 Features

- Remove duplicate script run output; bump version

## [0.0.2] - 2024-09-17

### 🚀 Features

- Add Generic module language; Gate URLs with --offline; Add TODOs; Add descriptions to unimplementeds
- Add initial --skip-verify-requirements capability
- Prepare code for containers and abstract virtual environments
- Initial local binary module support
- Add checksum capability
- Initial local generic script module support
- Make Config::read a little better
- Bump version to v0.0.2

### 📚 Documentation

- Update ROADMAP

### Wip

- *(ci)* Updates to test jobs
- *(ci)* Use cargo test
- *(ci)* Readd git config to docs job

## [0.0.1] - 2024-09-03

### 🚀 Features

- Add rustfmt config; format code
- Point sandbox notebook to Rust kernel
- Rename to pipe
- Initial addition of run subcommand
- Initial addition of doctor command
- Initial creation of is_command
- Clear lint issues
- Abstract running terminal commands
- Make file reading modular, improve error handling
- Initial addition of script templates
- Add handlebars template functionality with tests
- Allow spaces in handlebars template variables
- Refactor subcommands into separate submodules
- Move CLI config into separate submodule; create run submodule
- Refactor functions into util
- Move enums to top of util file
- Add lib target and add version to config struct
- Add description to config schema
- Add doc alias to improve CWL support
- Update config; add docs; improve codemeta compliance
- Add URI parsing capability
- Consolidate Config functions in Config impl
- Add tests for get and set env var
- Create StringTemplate struct and impl
- Use static constants in StringTemplate struct
- Expand arguments
- Refine use of info and debug
- Update local working directory for script execution
- Rename conda_environment Command struct attribute to virtual_environment
- Add conda run support to Script run method
- Load template from JSON if exists
- Support using module.template attribute in config.json
- Make environment.yml optional
- Integrate verbosity flag into logging
- Only create environment if it does not already exist
- Create run_with_output to allow run to print output
- Add remote script module git download
- Support module variables
- Add tracing (replace log)
- Update sandbox notebook
- Simplify code with derive_more crate
- Simplify script Display trait
- Implement builder pattern using bon crate improve API
- Refine tracing and Command builder usage
- Add exits to run subcommand
- Add colors to trace logs
- Clear lint issue, add more color to logs
- More tweaks to logging color scheme
- More rigorously define logging strategy
- Abstract logging labels and standardize usage
- Support expanding multiple templates in arguments
- Add YAML format configuration support
- Update Config struct and ROADMAP
- Configuration module template attribute support path or ScriptTemplate
- Add output to Script::run and start to phase out run_with_output
- Improve logging and move is_command to Command::test
- Obviate and remove Script::run_with_output
- Refine logging and make progress with check command
- Move script functions to Script struct; move dry_run up an abstraction
- Refine Command test method
- Add preferred exit codes to run command code
- Filter URLs against whitelist
- Improve Script::run error handling
- Add input and output attributes to config
- Remove Lychee
- Create pipe-cli sub package
- Configure clippy to auto-fix and clear issue
- Move util into pipe-lib subpackage
- Add pipe-py structure
- Add temp python interface for testing PyO3 build with maturin
- Update bon
- Update derive_more
- Update, remain, and consolidate workspace dependencies
- Add run arguments for later

### 🐛 Bug Fixes

- Update ASCII art
- Remove more instances of gerber string
- Make midas run using template check
- Add makefile for real
- *(ci)* Install dev dependencies
- *(ci)* Install more dev dependencies
- *(ci)* Pass some options to cargo binstall
- *(tests)* Refine test configuration
- *(tests)* Make is_command conda environment test conditional
- *(ci)* Remove tarpaulin coverage from CI
- *(ci)* Move generated docs to site/html
- *(ci)* Configure doc deployment to load static files
- Add environment support to Script from_template
- *(tests)* Refactor is_command to not panic
- Make command modifier string and finish integrating
- Update ModuleUri working_directory method for remote script modules
- Remove Command modifier attribute, allow scripts to run inside Conda virtual environments
- Clear lint issues
- *(docs)* Add middle initial to niki citation
- *(docs)* Add quotes around niki given name
- Enable ModuleUri hash support for URLs with no path
- Remove undeclared variable from Script::run error branch
- Improve get_conda_environment_list testability
- *(tests)* Try to fix Command::test doctests
- *(tests)* Remove doctest that relies on environment and is inconsistent
- Replace dotenv with dotenvy to clear security issue
- *(ci)* Update doc generation

### 📚 Documentation

- Create CONTRIBUTING file
- Add docs to util file
- Remove obviated comment
- Improve PIPE text logo
- Remove some unnecessary PIPE_ in variable names
- More improvements to PIPE text logo
- Incorporate parallelism into text logo
- Add documentation for using local file URIs
- Minor error output tweak
- Initial addition of roadmap to README
- Update CONTRIBUTING guide
- Update roadmap
- Improve ModuleUri docs and remove a println
- Move roadmap into separate file
- Ad some items to ROADMAP
- Add API server item to ROADMAP
- Add citation file and license
- Update ROADMAP

### 🧪 Testing

- Run tests
- Add coverage report
- Add test for Script::from_template run
- Update lcov report
- Update coverage
- Generate test coverage
- Update config files

### ⚙️ Miscellaneous Tasks

- Run cargo fmt
- Run cargo clean
- Add CI config
- Add release jobs

### Dev

- Add makefile

### Wip

- Initial creation of config Struct
- Add build and test files
- Update config test fixture
- Toward configuration v1
- Add comment for is_command windows support
- Add windows support to is_command
- Add placeholder functions for handlebars functionality
- Start working on parsing URIs
- Working on setting env vars
- Working on environment variable functions
- Refactor env var set; start env var get
- Working on pipe run
- Add todo comments for tracking short-term feature roadmap in run
- Still working on pipe run
- Environrment variable expansion
- Get name from Conda environment.yml
- Integrate command modifiers
- Start abstracting working directory creation
- Adding virtual environment installation
- Initial addition of script run from virtual env creation
- Add todo for env check and add par_iter just for fun
- Remove par_iter b/c modules should not be parallel
- Initial scaffolding of is_valid_python_module check
- Add build tasks ISO publishing binary releases
- Build out Script::run to provide better logging
- Prepare to add YAML support
- Add check to configuration
- Add link checking functionality
- Add more functionality to link checking with tokio
- Remove pipe-py from workspace until I can fix cc linking error
- *(ci)* Explicitly disable inheriting default
- *(ci)* Install tarpaulin with cargo

<!-- generated by git-cliff -->
