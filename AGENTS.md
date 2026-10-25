# AGENTS.md

**USE [guidelines](./.agents/skills/guidelines/SKILL.md) WHENEVER CODE IS TOUCHED OR CONSIDERED**

**USE [planning](./.agents/skills/planning/SKILL.md) WHENEVER `.wiki` PLANS ARE CREATED, REVIEWED, UPDATED, EXECUTED, PRUNED, OR RETIRED**

For Rust work, use [rust-best-practices](./.agents/skills/rust-best-practices/SKILL.md) as supplemental guidance and [rust-review](./.agents/skills/rust-review/SKILL.md) for requested code reviews. The [guidelines skill](./.agents/skills/guidelines/SKILL.md) takes precedence over both Rust skills whenever their instructions conflict. These skills are adapted from BeautifulRust guidance under its [MIT license](./.agents/skills/BeautifulRust-LICENSE).

When skill guidance conflicts with the user's request, this file, or repository configuration, follow the user's request first, then this file and the repository's established lint and build configuration. Keep changes within the requested scope. Do not apply recommended lint configuration or run workspace-wide fixes solely because a skill recommends them. The workspace forbids unsafe Rust and denies missing public documentation; preserve these rules.

This file guides agents working in the Xylem repository.

Xylem is a Rust CLI and library for configuring and running data workflows, with durable run journals, reporting, and an optional terminal dashboard.

## Concurrent Implementation Coordination

When multiple implementations share a working tree:

- Declare and stay within the exact write set for each assignment.
- Do not edit files owned by concurrent work unless the owners coordinate the overlap.
- Do not run workspace-wide formatting or fixes while another implementation is active.
- Run focused checks independently. Serialize final lint, test, and release gates through one agent after implementations stabilize.
- If a concurrent change temporarily breaks compilation, coordinate with its owner instead of patching their files.
- Respect dependencies and exclusive locks recorded in active plans.

## Quick Start

1. Read the relevant module, adjacent tests, and current configuration or CLI help before changing behavior.
2. For a workflow format change, check `docs/src/configuration.md`, `MODULE_SPEC.md`, examples, and the configuration validation code together.
3. For execution changes, trace the plan, workspace, acquisition, execution, journal, outcome, and reporting paths that own the behavior.
4. For CLI changes, inspect `crates/xylem-cli/src/cli.rs`, the relevant command module, and generated help under `docs/src/generated/`.
5. For dashboard changes, inspect `crates/xylem-tui/` and its use of persisted run state.
6. Identify any active `.wiki` plan and follow the planning skill before editing a plan.

Use CodeGraph before grep or file discovery when locating or understanding code in this indexed repository. The `.codegraph/` directory is present; use `codegraph explore "<question or symbols>"` and request exact symbols when source detail matters.

## Development Commands

### Common commands

```bash
# Format and run the repository lint workflow. Note: make lint applies clippy fixes.
make lint

# Run the normal test suite
make test

# Run ignored tests as well
make test-all

# Build the CLI in release mode for configured release targets
make build

# Generate API documentation
make doc

# Run doctests
make doctest

# Regenerate checked-in CLI help files after changing command arguments or help text
make help

# Run coverage
make cover

# Run dependency security audit
make audit
```

`make clean` removes build outputs and `bin/`; use it only when cleanup is needed. `make lint` runs `cargo fmt` and Clippy with `--fix --allow-dirty`, plus shell and container checks, so inspect the worktree before and after running it. Prefer focused Cargo commands during implementation and avoid automatic fixes unless they are needed.

### Run from source

```bash
cargo run --package xylem-cli -- --help
cargo run --package xylem-cli -- run path/to/workflow.json
cargo run --package xylem-cli -- export --gitlab path/to/workflow.json
cargo run --package xylem-cli -- tui path/to/workflow.json
```

CLI options may change. Consult `docs/src/generated/xylem-help.txt` or run the relevant `--help` command for current syntax.

## Common Tasks

### Changing workflow configuration

1. Inspect the corresponding type and validation in `crates/xylem-lib/src/config/`.
2. Update the supported JSON, JSONC, YAML, and YML behavior consistently where applicable.
3. Update `docs/src/configuration.md`, `MODULE_SPEC.md`, and affected examples when the public contract changes.
4. Add or update focused configuration tests and verify the affected formats.

### Changing workflow execution

1. Identify whether the behavior belongs to planning, acquisition, execution, sandboxing, journaling, or reporting.
2. Preserve deterministic module ordering, workspace isolation, cancellation and failure outcomes, and journal compatibility.
3. Update the owning module's adjacent tests and integration coverage for observable CLI behavior.
4. Update `ARCHITECTURE.md` for structural changes and the book for user-facing behavior.

### Adding or changing a CLI command

1. Define arguments and help in `crates/xylem-cli/src/cli.rs`.
2. Implement dispatch and behavior in `crates/xylem-cli/src/commands/`.
3. Update CLI integration tests under `crates/xylem-cli/tests/`.
4. Regenerate the checked-in help with `make help` when argument or help text changes.
5. Update book documentation and examples as needed.

### Changing retained runs or the dashboard

1. Keep durable execution facts in `xylem-lib` journal and run-store types.
2. Treat summaries and TUI views as projections of persisted state.
3. Preserve recovery behavior and compatibility when changing journal records.
4. Update the run-store, projection, CLI, or TUI tests that cover the changed behavior.

## Testing and Verification

- Unit tests usually live in adjacent `tests.rs` files or `#[cfg(test)]` modules.
- CLI behavior is covered under `crates/xylem-cli/tests/`.
- Examples under `crates/xylem-cli/examples/` are executable workflow demonstrations.
- `cargo nextest run` is the normal suite (`make test`); `make test-all` includes ignored tests.
- Use focused package or test filters while iterating. Run broader gates when the scope warrants them and report which checks were run.
- For workflow-format changes, validate representative JSON, JSONC, and YAML inputs. For execution changes, verify journal and final outcome behavior as well as command success.

Do not run workspace-wide formatting or fixes solely to satisfy a copied skill. Follow the repository's current `Cargo.toml`, `Makefile`, and CI configuration.

## Rust and Workspace Rules

- `unsafe_code = "forbid"`; do not introduce unsafe code.
- `missing_docs = "deny"`; document public Rust items.
- `broken_intra_doc_links = "forbid"`; keep rustdoc links valid.
- Clippy warns on `unwrap_used`, `expect_used`, panic, indexing, and several conversion and API hazards. Avoid panics on fallible paths and use checked conversions where needed.
- Do not suppress a lint broadly. If a narrow exception is necessary, follow repository conventions and state the reason.
- Preserve the workspace's no-default-feature choices and feature boundaries when changing dependencies or crate code.
- Shared dependency versions belong in `[workspace.dependencies]` in the root `Cargo.toml`; keep dependency-policy files in sync when required.

## Project Structure

### Workspace crates

- `crates/xylem-cli`: CLI parsing, commands, process entry point, examples, and CLI integration tests.
- `crates/xylem-lib`: configuration, workflow planning and execution, acquisition, sandboxing, journaling, persistence, and library APIs.
- `crates/xylem-tui`: optional terminal dashboard and projections of stored run data.
- `tools/memory-spike`: memory and scheduling experiments.
- `xtask`: repository maintenance and release tasks.

### Key paths

- Configuration: `crates/xylem-lib/src/config/`
- Workflow planning and execution: `crates/xylem-lib/src/plan.rs`, `execution.rs`, `run/`, `script/`
- Acquisition and sandboxing: `crates/xylem-lib/src/acquisition/`, `sandbox/`
- Run state: `crates/xylem-lib/src/journal/`, `workspace/`, `outcome.rs`, `projection/`
- CLI: `crates/xylem-cli/src/cli.rs`, `commands/`
- Dashboard: `crates/xylem-tui/src/`
- User docs: `docs/src/`; workflow contract: `MODULE_SPEC.md`
- Active implementation plans: `.wiki/`, with entry point `.wiki/index.md`

## Planning Wiki

The authoritative entry point for implementation plans is [.wiki/index.md](.wiki/index.md). Treat `.wiki` as a queue of remaining work, not a historical ledger. Before implementing planned work, verify plan evidence against current code, tests, and documentation. When changing plans, update metadata, remaining phases, dependencies, indexes, and coordination records together. Remove completed sections and plans after preserving durable evidence elsewhere. Follow the planning skill's authoring, lifecycle, and validation rules.

## Documentation

- Update `docs/src/configuration.md` and `MODULE_SPEC.md` when configuration or module contracts change.
- Update examples when user-visible setup or execution behavior changes.
- Update `ARCHITECTURE.md` for structural changes.
- Regenerate CLI help files with `make help` after changing CLI syntax or help descriptions.
- Keep public Rust documentation aligned with actual behavior and error conditions.

## Repository Information

- Repository: https://code.ornl.gov/research-enablement/xylem
- Documentation: https://xylem.ornl.gov
- License: MIT
- Version: `[workspace.package].version` in `Cargo.toml`
- Rust edition: 2021
