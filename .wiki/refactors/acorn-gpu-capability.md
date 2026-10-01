---
type: Plan
title: "Move portable GPU capability into ACORN"
description: "Move provider-neutral GPU inventory and telemetry types into ACORN while leaving Xylem responsible for run monitoring and journal lifecycle."
difficulty: medium
impact: high
package: [acorn-core, acorn-schema, xylem-lib]
tags: [acorn, gpu, hardware, monitoring, portability, telemetry]
status: planned
---

# Goal

Make ACORN the owner of portable GPU identity, capacity, and telemetry modeling. Xylem should retain only the run-scoped monitoring policy, `nvidia-smi` adapter, durable journal transitions, aggregation, and reporting needed to execute workflows.

# Current evidence

- Xylem uses ACORN's `CommandSpec` for bounded process execution, `Milliseconds` for the sampling interval, and `Validate` for inventory validation.
- `acorn_schema::hardware::Resource::GPU` models a requested GPU resource, including architecture, backend, count, memory, name, and vendor. It does not model an observed device's stable provider identifier or enumeration index, both of which Xylem needs to correlate samples.
- `Resource` currently implements `Serialize` but not `Deserialize`, `Eq`, or `PartialEq`. Xylem journal events must deserialize during recovery and compare the initial inventory with later samples.
- Xylem's `GpuDevice` is consequently limited to the replay-safe fields required by monitoring: index, UUID, product name, and total memory. Provider-specific command execution and CSV parsing remain isolated in `xylem-lib::gpu`.
- The ACORN revision pinned by this work contains the portable `Milliseconds` type but does not yet provide an observed-device or GPU-telemetry schema.

# Project code boundaries

- **ACORN:** add provider-neutral observed GPU identity, capacity, utilization, and memory telemetry types with serialization, deserialization, equality, validation, and documented unit semantics. Keep request declarations such as `Resource::GPU` distinct from observed hardware.
- **Xylem:** replace local data-transfer types with ACORN types; retain monitor start/stop behavior, journal event ordering, recovery aggregation, CLI/config activation, and the `nvidia-smi` adapter.
- **Do not move:** run status, degradation policy, journal persistence, report rendering, or job exit-code decisions. These are Xylem execution concerns.

# Recommendation

1. Define the smallest observed GPU model in ACORN with a provider-neutral stable identifier, optional provider-local index, display name, optional total memory, and explicit units.
2. Define one timestamped reading model with optional utilization and used memory. Missing metrics must remain distinguishable from zero.
3. Implement `Serialize`, `Deserialize`, `Eq`, `PartialEq`, and `Validate` for the portable types. Provide collection validation for unique stable identifiers and optional indices.
4. Add adapters or constructors in ACORN without adding command execution or an NVIDIA dependency. Vendor tools remain downstream adapters.
5. Update Xylem's parser to construct the ACORN types and keep topology-change checks based on stable identity.
6. Migrate the versioned plan and run-summary schemas only if the serialized representation changes. Preserve compatibility through an explicit schema-version transition.
7. Remove Xylem's local GPU identity and reading types after journal recovery, fake-`nvidia-smi` tests, and supported-target builds pass with the ACORN model.

# Validation

- ACORN round-trips observed devices and readings through JSON and validates empty identifiers, invalid utilization, duplicate identifiers, and missing optional metrics.
- ACORN's portable crates build with `default-features = false` for native and WebAssembly targets.
- Xylem's parser tests cover quoted NVIDIA names, unavailable metrics, malformed rows, and topology changes while constructing ACORN-owned values.
- Xylem journal tests prove deterministic replay and aggregation with the shared types.
- `cargo tree` shows one ACORN source revision and no vendor SDK dependency.

# Acceptance criteria

- ACORN owns replay-safe, provider-neutral observed GPU and telemetry types.
- `Resource::GPU` remains a resource request unless ACORN explicitly introduces a separate observed-resource representation.
- Xylem contains no duplicate GPU identity or reading model.
- NVIDIA-specific behavior is confined to the Xylem adapter and can be replaced by another provider without changing journal lifecycle code.
- Native, musl, and WebAssembly checks retain their existing support level.

# Related plans

- [Dependency surface and security gates](dependency-surface-and-security-gates.md)
- [Memory-aware adaptive scheduling](../features/memory-aware-scheduling.md)
