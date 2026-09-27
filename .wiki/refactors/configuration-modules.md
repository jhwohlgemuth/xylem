---
type: Plan
title: "Add reusable configuration modules"
description: "Allow local and remote configuration modules to group workflow stages and be composed by another workflow."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [configuration, modules, workflow, composition]
status: planned
---

# Goal

Make a `configuration` module execute a referenced workflow as a reusable group of modules while preserving the behavior of a top-level workflow.

# Current evidence

- `ModuleType::Configuration` is parsed but rejected during acquisition and planning.
- Module execution is ordered at the top-level configuration, with variables, inputs, completion, reporting, and journaling scoped to that configuration.
- A nested configuration needs clear identity and path resolution so its modules can be resumed and reported without colliding with the parent workflow.

# Design

- Permit `type: "configuration"` with local `file:` and remote Git/HTTP URIs, and validate the referenced configuration before execution.
- Resolve nested relative module URIs from the referenced configuration's directory, not the parent process directory.
- Expand the nested module list in place, preserving parent order. Parent variables and input values flow into the group; nested variables override inherited values using the existing precedence rules.
- Namespace journal entries, completion markers, plan module identities, and reporter output by the configuration module path and name.
- Reject direct and indirect configuration cycles with a readable chain of module names and paths. Set a small, documented nesting-depth limit.
- Acquire remote configurations through the existing allowlist and bounded-reachability policy; retain source identity for journaling and require a documented pinning contract before treating an unpinned remote configuration as reproducible.

# Validation

- Add unit tests for parsing, local URI resolution, variable inheritance and override, input propagation, completion isolation, ordered expansion, cycle rejection, and depth rejection.
- Add a CLI fixture where a parent workflow uses one grouped configuration before and after an ordinary script module.
- Add a configuration module to Kitchensink that groups its Mamba and Pixi stages, so the example covers local configuration composition alongside script, binary, and container modules.
- Verify plans, dry runs, journal JSONL, resumable execution, and text/JSON reporters show nested identities deterministically.
- Document the configuration-module schema, execution model, limitations, and a runnable example.

# Acceptance criteria

- Local and remote configuration modules can group multiple stages and run inside another workflow.
- Nested workflows behave equivalently to inline modules for inputs, variables, completion, reporting, and resume.
- Cycles, invalid nested configurations, ambiguous output paths, and disallowed or unreachable remote references fail before side effects.
