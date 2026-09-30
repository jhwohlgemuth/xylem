# Xylem

> Digital xylem that transports data through workflows with structure and support

Xylem is a workflow runner for composing command-line tools into repeatable,
configuration-driven pipelines. A workflow declares ordered modules, inputs,
variables, requirements, and execution options in JSON or YAML. Xylem prepares
the modules and runs the same ordered pipeline for each input.

The workspace contains two Rust packages:

- `xylem-cli` provides the `xylem` command.
- `xylem-lib` provides configuration, module, interpolation, and execution
  types used by the CLI.

## Execution model

1. Read a JSON or YAML workflow configuration.
2. Check declared host requirements.
3. Prepare local or remote modules and their environments.
4. Resolve workflow, module, and input variables.
5. Execute modules in declaration order for every input.

Inputs may run concurrently. Use the global `--threads N` option to bound the
Rayon work managed by Xylem, and optional workflow `options.batch` to set a
smaller rolling ceiling on active top-level inputs. Child tools can still
create their own threads or process multiple items internally.

For the stable module-level interface, see the [module contract](./modules.md).

## Support tiers

Linux is the rigorously tested binary target. Homebrew is tested on Linux and
macOS. Windows builds are available for early use, while full macOS and Windows
support remains roadmap work. The local POSIX-shell workflow is the supported
offline baseline; Pixi, Conda/Mamba, containers, and remote acquisition are
optional integration tiers.
