# CHANGELOG 🌱
> Wisdom is knowledge of things that don't change, intelligence is the knowledge of things that do

## Unreleased

### 🚀 Features

- Add the experimental public `docker_sandbox` provider for the standalone
  local `sbx` CLI, including a digest-pinned functional example and native
  qualification path.
- Add journal and run-summary schema v9 with durable memory estimator, reservation, fallback, and optional peak-observation evidence.
- Add a runnable offline memory-aware scheduling example with deterministic reservations and an out-of-range fallback plan.
- Collect optional per-input cgroup v2 peak memory for configured Linux OCI-sandboxed runs before sandbox cleanup.
- Document the measured `module-a-p-v2` estimator and two-input admission profile after its individual and concurrent holdout gates passed.
- Add native rootless OCI preflight checks, retained conformance evidence, and an A/P-capable sandbox workload definition.
- Complete reusable local, HTTP(S), and Git-backed configuration modules with recursive policy-constrained flattening and qualified runtime identities.
- Complete sandbox Phase 4 with bounded private artifact export, typed exactly-once resource outcomes, policy-authorized failure retention, lease-aware stale reconciliation, and plan v12/run-summary v11 lifecycle evidence.

### 🐛 Bug Fixes

- Exclude failed workflow attempts from peak-memory calibration evidence while retaining bounded diagnostics and fail-closed artifact cleanup for prompt-policy rejections.
- Make OCI archive export reliable on rootless Docker and Podman through an exact bound archive while keeping raw output in container-private tmpfs and eliminating host ownership repair.
- Preserve prepared sandbox workspaces through execution, accept Xylem's managed run root during lifecycle revalidation, use portable private IPC isolation for Podman, and make private tmpfs storage non-root writable across Docker and Podman.

### 📦 Dependencies

- Upgrade the ACORN crates from the pinned 0.2.0 Git revision to the published 0.3.2 packages while continuing to use ACORN's canonical path-conversion APIs.

## [1.0.2](https://code.ornl.gov/research-enablement/xylem/compare/v1.0.0..v1.0.2) - 2026-09-19

### 🚀 Features

- Validate workflow configuration versions with ACORN's canonical semantic-version parser.
- Publish `xylem-lib` and `xylem-cli` to crates.io in dependency order.

### 📦 Distribution

- Build the Homebrew formula from the immutable crates.io `xylem-cli` artifact.
- Restrict release tags and formula versions to canonical `MAJOR.MINOR.PATCH` values.

## [0.0.11](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.10..v0.0.11) - 2025-10-27

### 🚀 Features

- Initial documentation with example configs - ([bc514ca](https://code.ornl.gov/research-enablement/xylem/commit/bc514caa38300f738a3bf1e026ea1628c40c58ca))

### 🐛 Bug Fixes

- Add snakecase and test cases for examples - ([dd28c25](https://code.ornl.gov/research-enablement/xylem/commit/dd28c251461aef44bbf7947994bd487e67641cac))
- Update file structure, separate content - ([e14aba0](https://code.ornl.gov/research-enablement/xylem/commit/e14aba02602f5c333e65dbd942ac4f8e9a785d25))

### 📚 Documentation

- Rename examples README files so GitLab will display them - ([51832ea](https://code.ornl.gov/research-enablement/xylem/commit/51832ea03d66e1320d05254fe779643885313227))

### ⚙️ Miscellaneous Tasks

- Transition to musl build - ([70d953f](https://code.ornl.gov/research-enablement/xylem/commit/70d953fae852d05f9e341af45c4f031e082ccdb9))
- Add bookworm tag to rust image - ([e87566b](https://code.ornl.gov/research-enablement/xylem/commit/e87566b4dd9e2b967a0d9601a1daf3e5d58f5980))
## [0.0.10](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.9..v0.0.10) - 2025-10-23

### 🚀 Features

- Add acorn-lib, cull some redundant code - ([6bc8225](https://code.ornl.gov/research-enablement/xylem/commit/6bc82252b52cd3e0f490ca1e79826ad31567029b))
- Make checksum return an option - ([2c8a773](https://code.ornl.gov/research-enablement/xylem/commit/2c8a7733ca86c80835c7d37690906035e24783c1))
- Update README and build files - ([ac5893c](https://code.ornl.gov/research-enablement/xylem/commit/ac5893cf8847d4903666e9388fda09a732c86c58))
- Rename get_extension to extension - ([7eff638](https://code.ornl.gov/research-enablement/xylem/commit/7eff638374e385916738f18a5e66bf3b1ff5f18e))
- Rename get_checksum to checksum - ([fff50e7](https://code.ornl.gov/research-enablement/xylem/commit/fff50e7dc4e665e34dba6a631e91dc6eebc18ebc))
- Remove get_parent - ([af2d3f2](https://code.ornl.gov/research-enablement/xylem/commit/af2d3f25cdafa3d8252f4742870b00008c349377))
- Rename to_snake_case to snake_case - ([c5fa035](https://code.ornl.gov/research-enablement/xylem/commit/c5fa035d818c39b703917e866833b74502634208))
- Rename all instances of PIPE to Xylem; remove has_extension function - ([b1004eb](https://code.ornl.gov/research-enablement/xylem/commit/b1004eb4d509058ff89ef1e837f9f5476f35d8e0))
- Rename instances of PIPE to Xylem - ([5cda7c6](https://code.ornl.gov/research-enablement/xylem/commit/5cda7c6d428145e056e2b59fc502183d9fb685e1))
- Clear lint issues and generate coverage - ([db45867](https://code.ornl.gov/research-enablement/xylem/commit/db45867a4fece86cb3fdcb9b82a448bab64fcef8))

### 🐛 Bug Fixes

- Ignore broken tests (for now) - ([4d439bf](https://code.ornl.gov/research-enablement/xylem/commit/4d439bf0bd1ff56b681479553af7f5211f509cae))
- Improve conda env listing and ensure base is included - ([fdf0a2e](https://code.ornl.gov/research-enablement/xylem/commit/fdf0a2ecb5a228a4a96061bf2f8a5e6dbb08c992))

### 📚 Documentation

- Update README with more install instructions and stuff - ([1e3a123](https://code.ornl.gov/research-enablement/xylem/commit/1e3a1236546974eebb926810ec36b37de5228999))
- Update banner in README - ([d748bba](https://code.ornl.gov/research-enablement/xylem/commit/d748bba41be59630b24746cc93f356796ef20216))

### 🤓 Developer Experience

- Add Peacock configuration to project - ([5fed399](https://code.ornl.gov/research-enablement/xylem/commit/5fed3999c8a97372c32ae34d26bf045ba8c554c3))
## [0.0.9](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.8..v0.0.9) - 2025-03-06

### 🚀 Features

- Update bon usage and bump version - ([d1803d0](https://code.ornl.gov/research-enablement/xylem/commit/d1803d09465b9ce267395b8e273c6fbdcb1fc122))
- Add input CLI arg - ([bda9ea8](https://code.ornl.gov/research-enablement/xylem/commit/bda9ea80e556099d9a386a460378616acc8b0c42))
- Update vscode color settings - ([d20cb4b](https://code.ornl.gov/research-enablement/xylem/commit/d20cb4b40e8af59cb79279e4f38c7a3ef9ec388a))

### 🐛 Bug Fixes

- Update vscode color settings - ([b5d3674](https://code.ornl.gov/research-enablement/xylem/commit/b5d3674ece4400ce0d097eb978ca27fc3d16eae4))
## [0.0.8](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.7..v0.0.8) - 2025-02-04

### 🐛 Bug Fixes

- Rename INPUT_PARENT to PARENT_DIRECTORY - ([3537084](https://code.ornl.gov/research-enablement/xylem/commit/3537084c0fb22c1fa88d97e2c776993a860f9217))
## [0.0.7](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.6..v0.0.7) - 2025-02-04

### 🚀 Features

- Add INPUT_PARENT env variable and bump version - ([df2125d](https://code.ornl.gov/research-enablement/xylem/commit/df2125d83e34824ab9804fbcc05eae8d53655224))
- Split up lib.rs into script, config, and util - ([b753e8d](https://code.ornl.gov/research-enablement/xylem/commit/b753e8ddb3dd257e24c87ff80ff5a2d6c32617c9))
- Add Config with_options method - ([41ba915](https://code.ornl.gov/research-enablement/xylem/commit/41ba91597eaebb2921cec3b914c86a78587205be))
- Make code exit when root directory is not provided - ([1a02df5](https://code.ornl.gov/research-enablement/xylem/commit/1a02df50b68454c38ad604023c7fb092ebaa2709))
- Forbid unsafe code in lib; clear lint warning - ([1f321de](https://code.ornl.gov/research-enablement/xylem/commit/1f321de7c832069708cfa81c53335d21143e6cfb))
- Support configuring run options from config.json - ([eb5cfb8](https://code.ornl.gov/research-enablement/xylem/commit/eb5cfb8e1d03accabdba8f9d5ddc14cd8f492d96))
- Make config options root attribute optional; Add aliases to boolean attributes - ([1ed1735](https://code.ornl.gov/research-enablement/xylem/commit/1ed173590281ab0616fafaea30c9c85f13fa1184))
- Set environment values; Add module list variable - ([89c64bb](https://code.ornl.gov/research-enablement/xylem/commit/89c64bbb7275d906e18e0f0ae090172a4eea5c0b))

### 📚 Documentation

- Add change log (generated using git-cliff) - ([5a3a516](https://code.ornl.gov/research-enablement/xylem/commit/5a3a516d2b64b0d61cfc6c46d70777a05ff1c59f))

### 🧪 Testing

- Update coverage - ([ace0dcc](https://code.ornl.gov/research-enablement/xylem/commit/ace0dcc6531cf05470890240a649d0d475d34ba3))
- Update coverage - ([bb08bd9](https://code.ornl.gov/research-enablement/xylem/commit/bb08bd9ba3267fb8cbd6d3e13954d9678c549f53))
- Add tests for Config methods used in run command - ([1ce8f6d](https://code.ornl.gov/research-enablement/xylem/commit/1ce8f6d73969d1b50b8db8e8d41fa37ba8c44e35))
## [0.0.6](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.5..v0.0.6) - 2024-10-10

### 🚀 Features

- Make programming language optional and generic the default - ([b70aaba](https://code.ornl.gov/research-enablement/xylem/commit/b70aaba0f5718cda4762c0c682a0ff10b6828e61))
- Add pyproject.toml parsing capability - ([fb22770](https://code.ornl.gov/research-enablement/xylem/commit/fb22770a59d3553a19488d06bc8049fab9a42930))
- Add toml library; minor tweak to EnvironmentValue method - ([a8d53e5](https://code.ornl.gov/research-enablement/xylem/commit/a8d53e51a40a4e56b3217ca6410448cd2651daee))

### 🐛 Bug Fixes

- *(tests)* Remove conditional test - ([82d92cd](https://code.ornl.gov/research-enablement/xylem/commit/82d92cdfb2c19ad79bcc6b723db34990f7544647))

### 📚 Documentation

- Add install instructions to README - ([61893ec](https://code.ornl.gov/research-enablement/xylem/commit/61893ecdc2c18bc6e1d15d5c014f18c0a27e7918))
- Update ROADMAP to identify features for v0.1.0 - ([feadcb7](https://code.ornl.gov/research-enablement/xylem/commit/feadcb7ca0b51fa7704bdbd1594da597dbe0748e))
## [0.0.5](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.4..v0.0.5) - 2024-10-06

### 🚀 Features

- Add URL whitelist check to remote module preparation - ([6179af4](https://code.ornl.gov/research-enablement/xylem/commit/6179af4254f6f4a3f95ffb56a9edbf03eb423aa5))
- Add executable check to binary module usage - ([5676da3](https://code.ornl.gov/research-enablement/xylem/commit/5676da3338cdaf0fed056cf425932bcf47afecd1))
- Initial input concurrency capability (needs testing) - ([90ee543](https://code.ornl.gov/research-enablement/xylem/commit/90ee5431d13f1f5d03dea5406d7539e09c3ad3bd))
- Refactor run functions for better maintainability and extendibility - ([e521b9a](https://code.ornl.gov/research-enablement/xylem/commit/e521b9a1d6df98f9bfa6b36d053b2604152a45a0))

### ⚙️ Miscellaneous Tasks

- Update what triggers docs build - ([8416313](https://code.ornl.gov/research-enablement/xylem/commit/8416313a8079c868b67b8a37fe5483ed64b7d1d4))

### 🤓 Developer Experience

- Update run conditions for CI - ([1cc1bff](https://code.ornl.gov/research-enablement/xylem/commit/1cc1bffdb22a6c910a71115d02cafc545448a0b7))
## [0.0.4](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.3..v0.0.4) - 2024-10-04

### 🚀 Features

- Bump version and publish - ([a80e5e0](https://code.ornl.gov/research-enablement/xylem/commit/a80e5e078b50fcb669873647f5f6bbf866d733ab))
- Initial type design for input concurrency - ([68e0fd0](https://code.ornl.gov/research-enablement/xylem/commit/68e0fd0e4bf5a7e88cfb52fa1161509636fa8202))
- Final steps to run remote binary modules - ([9a400ca](https://code.ornl.gov/research-enablement/xylem/commit/9a400caa37ed56bd37064e682fd418947a383e9b))
- Remove unimplemented in binary module piepline - ([764b2d4](https://code.ornl.gov/research-enablement/xylem/commit/764b2d4f3802d228010714d3b9a666f0b412f4e1))
- Add capability to download binaries - ([67e909b](https://code.ornl.gov/research-enablement/xylem/commit/67e909b6fff7d09a2bbdfffdeac63ea2eb78795c))
- Complete template resolution capability - ([156e4b3](https://code.ornl.gov/research-enablement/xylem/commit/156e4b3127f81113b8160dd1640a89c8375013f9))
- Add download_binary function - ([56efeb1](https://code.ornl.gov/research-enablement/xylem/commit/56efeb14eb07a4b2347b4cf5518cdb64901d0f4c))
- Add architecture document - ([9082a11](https://code.ornl.gov/research-enablement/xylem/commit/9082a11375e5fd21f53ca976f1aa694dfcadec13))
- Enforce template data loading order in run execution phase - ([4da18fa](https://code.ornl.gov/research-enablement/xylem/commit/4da18fa04a4c226585275fbb472249865fb605dd))
- More virtual environment and container support - ([b78681a](https://code.ornl.gov/research-enablement/xylem/commit/b78681af5879262ec60bb0674094c192b05a9d8b))
- Implement container attribute for Commands; Pave way for non-conda virtual environment managers - ([83d5f0d](https://code.ornl.gov/research-enablement/xylem/commit/83d5f0dd189595a7047476ee9a16578856d9c855))
- Use which-rs crate for Command::test and update associated test to pass on windows - ([028c5f8](https://code.ornl.gov/research-enablement/xylem/commit/028c5f81d6e2f62b58543ec03960aaaa13da65af))
- Prepare code for executing binaries - ([dc92879](https://code.ornl.gov/research-enablement/xylem/commit/dc9287937b46f1fb1d40e6fe7da115842a4a2586))
- Use get_parent; improve logging a bit - ([02b9971](https://code.ornl.gov/research-enablement/xylem/commit/02b99718e016ac45dbbd6516214334ccb571c0ea))

### 🐛 Bug Fixes

- *(tests)* Make tests pass on Windows - ([74f516c](https://code.ornl.gov/research-enablement/xylem/commit/74f516ce0f7bc20950ddddcc3345150626773795))
- Create CI test task - ([e7801d9](https://code.ornl.gov/research-enablement/xylem/commit/e7801d97b272126c48ae34e18d586a697decb37e))
- Use exec instead of run for docker containers - ([a8b5e4c](https://code.ornl.gov/research-enablement/xylem/commit/a8b5e4c88957290841c842ff437b775e0a36df29))
- Remove typo - ([7ef788a](https://code.ornl.gov/research-enablement/xylem/commit/7ef788aad566c428fdef912e732812215c66e5c1))
- Remove test requirement from config.json fixture - ([456009f](https://code.ornl.gov/research-enablement/xylem/commit/456009f82d49b2ec300c8cabf5d5cf93e0b6fe4e))
- Make checksum attribute optional - ([1477a71](https://code.ornl.gov/research-enablement/xylem/commit/1477a71c0bfbca382fd8fcb41ee9eca3a4a9a6b4))

### 📚 Documentation

- Update ROADMAP and CONTRIBUTING - ([3e56f27](https://code.ornl.gov/research-enablement/xylem/commit/3e56f2736a6a4740892f00b970d16fe1f1a5c71e))

### 🧪 Testing

- Add some CLI tests - ([2b39737](https://code.ornl.gov/research-enablement/xylem/commit/2b39737abc3212db6188b832b157c8d98aeec640))
## [0.0.3](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.2..v0.0.3) - 2024-09-17

### 🚀 Features

- Remove duplicate script run output; bump version - ([5ff86a5](https://code.ornl.gov/research-enablement/xylem/commit/5ff86a5b64c0d74a0b7e266ad2bd2bf12cdb191c))
## [0.0.2](https://code.ornl.gov/research-enablement/xylem/compare/v0.0.1..v0.0.2) - 2024-09-17

### 🚀 Features

- Bump version to v0.0.2 - ([ca4cd6d](https://code.ornl.gov/research-enablement/xylem/commit/ca4cd6d27554c88eecc1bf6dc5fb316c14cbf478))
- Make Config::read a little better - ([10e5100](https://code.ornl.gov/research-enablement/xylem/commit/10e51001787ac0210706e8ec93bac361e25700e6))
- Initial local generic script module support - ([0c564f6](https://code.ornl.gov/research-enablement/xylem/commit/0c564f67fe75e847124ba7752897b11efa8e059d))
- Add checksum capability - ([36e185d](https://code.ornl.gov/research-enablement/xylem/commit/36e185d38f68c998a70d8101ab8fffccbaab7365))
- Initial local binary module support - ([28ecbe7](https://code.ornl.gov/research-enablement/xylem/commit/28ecbe7b5fb64599f34d9875b7cc7442d595d1b1))
- Prepare code for containers and abstract virtual environments - ([6d2901f](https://code.ornl.gov/research-enablement/xylem/commit/6d2901f22717206d2eb2a840c5a47dbc9c4aa8a0))
- Add initial --skip-verify-requirements capability - ([08f03aa](https://code.ornl.gov/research-enablement/xylem/commit/08f03aa871a66200d2692c9d84c344fd5a300a16))
- Add Generic module language; Gate URLs with --offline; Add TODOs; Add descriptions to unimplementeds - ([fc8b3a8](https://code.ornl.gov/research-enablement/xylem/commit/fc8b3a880d97225fbdd99d710a00cfe357815c1c))

### 📚 Documentation

- Update ROADMAP - ([64c2576](https://code.ornl.gov/research-enablement/xylem/commit/64c2576d5a861023f57f92ce3cdd337ef2dea7b2))

### 🚧 In Progress

- *(ci)* Readd git config to docs job - ([110ce4f](https://code.ornl.gov/research-enablement/xylem/commit/110ce4f99d7203e8142799c47b202317be39ffe0))
- *(ci)* Use cargo test - ([02df29d](https://code.ornl.gov/research-enablement/xylem/commit/02df29d511fefcd945ad5fda6403e4b90e12c97b))
- *(ci)* Updates to test jobs - ([b3c543f](https://code.ornl.gov/research-enablement/xylem/commit/b3c543fff7eb2b40ca745d40e0ebc52ed5fed0b5))
## [0.0.1] - 2024-09-03

### 🚀 Features

- Add run arguments for later - ([c791079](https://code.ornl.gov/research-enablement/xylem/commit/c7910793cdbbef34a392873707c9175735e53a5e))
- Update, remain, and consolidate workspace dependencies - ([5e8b579](https://code.ornl.gov/research-enablement/xylem/commit/5e8b5794b96cfe4ff894db00a4afd96d942b8980))
- Update derive_more - ([379c4b5](https://code.ornl.gov/research-enablement/xylem/commit/379c4b574b756f7011fe39d071968aa0eb2e7d54))
- Update bon - ([26671e5](https://code.ornl.gov/research-enablement/xylem/commit/26671e575b1d741a7caca429d1cd4d7ad82760b7))
- Add temp python interface for testing PyO3 build with maturin - ([67f9dd6](https://code.ornl.gov/research-enablement/xylem/commit/67f9dd64a6bc607601efc18f9de7e2b0414fa900))
- Add pipe-py structure - ([81eaf08](https://code.ornl.gov/research-enablement/xylem/commit/81eaf0866ad3925d072ecfd86abffead4996a029))
- Move util into pipe-lib subpackage - ([3b96a4d](https://code.ornl.gov/research-enablement/xylem/commit/3b96a4d76e81849cccc7203ee50a98e473ca924e))
- Configure clippy to auto-fix and clear issue - ([63f8f7d](https://code.ornl.gov/research-enablement/xylem/commit/63f8f7ddc8835f60b64d571a3358bd8cdbef1b8c))
- Create pipe-cli sub package - ([79cf10d](https://code.ornl.gov/research-enablement/xylem/commit/79cf10db5ddc3315b1f82c356f312899254ed62e))
- Remove Lychee - ([bbff54b](https://code.ornl.gov/research-enablement/xylem/commit/bbff54b0ffc75f2cefe0fd9349840c2f9a7d6462))
- Add input and output attributes to config - ([b251ca7](https://code.ornl.gov/research-enablement/xylem/commit/b251ca7b95780f1419cc0317a09ab1b173340611))
- Improve Script::run error handling - ([7afefba](https://code.ornl.gov/research-enablement/xylem/commit/7afefba3be57157a9ff50c586e7e39844671e2f2))
- Filter URLs against whitelist - ([d8082a9](https://code.ornl.gov/research-enablement/xylem/commit/d8082a9f40dcfe23c6262cf90251a10eb9d07583))
- Add preferred exit codes to run command code - ([0c89cb2](https://code.ornl.gov/research-enablement/xylem/commit/0c89cb2e95f7d699c6d8a50cb1d3fd1075fec50b))
- Refine Command test method - ([dfb026c](https://code.ornl.gov/research-enablement/xylem/commit/dfb026c5e98fa6ee7aae338220db66d068171f55))
- Move script functions to Script struct; move dry_run up an abstraction - ([b2c1fa4](https://code.ornl.gov/research-enablement/xylem/commit/b2c1fa433d9855ae2384331819e257f78bad3b2b))
- Refine logging and make progress with check command - ([b2a8d5a](https://code.ornl.gov/research-enablement/xylem/commit/b2a8d5a677d85436ecd8647802b350bbebc707b7))
- Obviate and remove Script::run_with_output - ([0b21ab1](https://code.ornl.gov/research-enablement/xylem/commit/0b21ab1a859276f95da7c84c5dc73eb7d0f95ca8))
- Improve logging and move is_command to Command::test - ([a007935](https://code.ornl.gov/research-enablement/xylem/commit/a007935244f1cf5fb746ec4aa867ccb24007ab7f))
- Add output to Script::run and start to phase out run_with_output - ([53f110e](https://code.ornl.gov/research-enablement/xylem/commit/53f110ea3a0275bea2ee145d1faf8113a0270d69))
- Configuration module template attribute support path or ScriptTemplate - ([afeafc3](https://code.ornl.gov/research-enablement/xylem/commit/afeafc3c9901ffed8991a3e0fe1870a5ffdbee71))
- Update Config struct and ROADMAP - ([d91298b](https://code.ornl.gov/research-enablement/xylem/commit/d91298bcfa5444f7033e2347c01c01ca9c43f995))
- Add YAML format configuration support - ([9c19a2f](https://code.ornl.gov/research-enablement/xylem/commit/9c19a2fabf7c1e150fcd062f0d99d717f0399663))
- Support expanding multiple templates in arguments - ([5e7ec60](https://code.ornl.gov/research-enablement/xylem/commit/5e7ec601c0a8044ff2b6716a677e5af56102cf4d))
- Abstract logging labels and standardize usage - ([ffc814f](https://code.ornl.gov/research-enablement/xylem/commit/ffc814ff96964f7364159ddb57ee3569dcc10a2d))
- More rigorously define logging strategy - ([522b01f](https://code.ornl.gov/research-enablement/xylem/commit/522b01f82c29eaaef2a763f8d54bb61f6ef697fd))
- More tweaks to logging color scheme - ([35a2bf5](https://code.ornl.gov/research-enablement/xylem/commit/35a2bf5356b6f2f24893f81dca5c29e34f2f03e6))
- Clear lint issue, add more color to logs - ([438bee3](https://code.ornl.gov/research-enablement/xylem/commit/438bee3419c0ee1e4999cfe494ed19d85f163810))
- Add colors to trace logs - ([1ab778e](https://code.ornl.gov/research-enablement/xylem/commit/1ab778e1bd30cefcf8ed10f2f2b7b1abb12ff96f))
- Add exits to run subcommand - ([e12fdca](https://code.ornl.gov/research-enablement/xylem/commit/e12fdcacad49fe3f7dc1a708e3ada66c2273b703))
- Refine tracing and Command builder usage - ([cb64179](https://code.ornl.gov/research-enablement/xylem/commit/cb641791ad6814d774d99ae3a47d12394242876c))
- Implement builder pattern using bon crate improve API - ([37b57b8](https://code.ornl.gov/research-enablement/xylem/commit/37b57b8eb1857cf7afc06caf8441b0cdcb48bb64))
- Simplify script Display trait - ([a652941](https://code.ornl.gov/research-enablement/xylem/commit/a652941fcb156dbe89b891a853c7db32ce666b11))
- Simplify code with derive_more crate - ([b3a77ba](https://code.ornl.gov/research-enablement/xylem/commit/b3a77baaef1938f66cd703bc76198919093297d6))
- Update sandbox notebook - ([383b359](https://code.ornl.gov/research-enablement/xylem/commit/383b359c56db741c2b04ef7894e15f2a840bdbe8))
- Add tracing (replace log) - ([b8cbaaa](https://code.ornl.gov/research-enablement/xylem/commit/b8cbaaaadd9ab54a37da410a7452f9533a8c7bec))
- Support module variables - ([447506e](https://code.ornl.gov/research-enablement/xylem/commit/447506e845dacf8522e097befad83868b73e3a60))
- Add remote script module git download - ([136a3c9](https://code.ornl.gov/research-enablement/xylem/commit/136a3c901c877612601a8ae571c70e476ea4a3be))
- Create run_with_output to allow run to print output - ([e391f39](https://code.ornl.gov/research-enablement/xylem/commit/e391f39c888dee2319fe33031356114207ff1e24))
- Only create environment if it does not already exist - ([55cbcc7](https://code.ornl.gov/research-enablement/xylem/commit/55cbcc7eab3ebe1ebf82f3e42d4faf73d0f8cd56))
- Integrate verbosity flag into logging - ([e9a6d1b](https://code.ornl.gov/research-enablement/xylem/commit/e9a6d1b3db4d979950fa5583d8b0a54807718108))
- Make environment.yml optional - ([a2b549d](https://code.ornl.gov/research-enablement/xylem/commit/a2b549d62999bb400ae8f47dbb8ab5157b7b190c))
- Support using module.template attribute in config.json - ([32f817a](https://code.ornl.gov/research-enablement/xylem/commit/32f817af2cea01e863da495aa72d67ca9287d1a2))
- Load template from JSON if exists - ([0eb4668](https://code.ornl.gov/research-enablement/xylem/commit/0eb4668ffeee00dac9c2be61254232a455476365))
- Add conda run support to Script run method - ([6f74518](https://code.ornl.gov/research-enablement/xylem/commit/6f7451808c56508591e05f36787059c549f216a5))
- Rename conda_environment Command struct attribute to virtual_environment - ([8b5b491](https://code.ornl.gov/research-enablement/xylem/commit/8b5b491e9f0560156b2c613c93201591abb86478))
- Update local working directory for script execution - ([cea219c](https://code.ornl.gov/research-enablement/xylem/commit/cea219c0b00b86ab710f522b2f25729fc870eced))
- Refine use of info and debug - ([7e77f69](https://code.ornl.gov/research-enablement/xylem/commit/7e77f6911bda6d575d63a13c206ac0eba7a45f6a))
- Expand arguments - ([dea4368](https://code.ornl.gov/research-enablement/xylem/commit/dea4368db3648f26b59a946a2d810f29f1696a7e))
- Use static constants in StringTemplate struct - ([0d47183](https://code.ornl.gov/research-enablement/xylem/commit/0d471835542d352a0bc8bc27e3b17392cadbde50))
- Create StringTemplate struct and impl - ([1b76a6b](https://code.ornl.gov/research-enablement/xylem/commit/1b76a6bd6f9901d978109ec7adaadd2f341c76f6))
- Add tests for get and set env var - ([83459d6](https://code.ornl.gov/research-enablement/xylem/commit/83459d6c16ec2f9a70c2449ade992e7f9c52ef60))
- Consolidate Config functions in Config impl - ([e192a5d](https://code.ornl.gov/research-enablement/xylem/commit/e192a5dbccbece8615e6c4d691f36e68041837d5))
- Add URI parsing capability - ([c4c27cf](https://code.ornl.gov/research-enablement/xylem/commit/c4c27cf3cfc801a93efe817c4d398aaa6421b651))
- Update config; add docs; improve codemeta compliance - ([5a6e8c7](https://code.ornl.gov/research-enablement/xylem/commit/5a6e8c7d61a97d964a6811c23a84293c0dc188ea))
- Add doc alias to improve CWL support - ([bb564ef](https://code.ornl.gov/research-enablement/xylem/commit/bb564efa515c9a022ef44566f0a0f2999ddbdb79))
- Add description to config schema - ([1fc632a](https://code.ornl.gov/research-enablement/xylem/commit/1fc632ad11ad79c0ce40b2514efb8980138fb86a))
- Add lib target and add version to config struct - ([dcfaa6d](https://code.ornl.gov/research-enablement/xylem/commit/dcfaa6d4e3139e285086fcc97a355564f7b60056))
- Move enums to top of util file - ([0cd70c1](https://code.ornl.gov/research-enablement/xylem/commit/0cd70c15ec4935df7c7b272054ab4f3bbe8abb0b))
- Refactor functions into util - ([6ae9c28](https://code.ornl.gov/research-enablement/xylem/commit/6ae9c28f416ca2d9bec7fee9b81259980ad18be2))
- Move CLI config into separate submodule; create run submodule - ([c54db8a](https://code.ornl.gov/research-enablement/xylem/commit/c54db8a15f43b4eb8fb07abacba87b97e75eee9e))
- Refactor subcommands into separate submodules - ([391243a](https://code.ornl.gov/research-enablement/xylem/commit/391243af6682b1439fadd59883086f8205e9280f))
- Allow spaces in handlebars template variables - ([f2430fd](https://code.ornl.gov/research-enablement/xylem/commit/f2430fda2f233da8fbe59ad90233cdb149d1c8cc))
- Add handlebars template functionality with tests - ([7acc102](https://code.ornl.gov/research-enablement/xylem/commit/7acc10286f905f365c848c018db452104b262ba1))
- Initial addition of script templates - ([a789c7c](https://code.ornl.gov/research-enablement/xylem/commit/a789c7c5e3cad924d8a5b7df4cf7009437af91b9))
- Make file reading modular, improve error handling - ([3d176fc](https://code.ornl.gov/research-enablement/xylem/commit/3d176fc15bb9e606f7b88913c4e9f579540c6caf))
- Abstract running terminal commands - ([d96d19a](https://code.ornl.gov/research-enablement/xylem/commit/d96d19ac1c2e7d5e6d9f488829b395bd82df6090))
- Clear lint issues - ([1c1c957](https://code.ornl.gov/research-enablement/xylem/commit/1c1c95768077aef6766dec8d0ad0754e7801d6d7))
- Initial creation of is_command - ([edd2ba4](https://code.ornl.gov/research-enablement/xylem/commit/edd2ba4535150b465eb34b99f3eec431e5eccdcb))
- Initial addition of doctor command - ([22151cf](https://code.ornl.gov/research-enablement/xylem/commit/22151cf2e0fc1a0662bb01b506e59e1ae34a8338))
- Initial addition of run subcommand - ([d76605e](https://code.ornl.gov/research-enablement/xylem/commit/d76605ef54c93520c53c11b53403cd5bdfdec20b))
- Rename to pipe - ([714551c](https://code.ornl.gov/research-enablement/xylem/commit/714551c3363b80fbbf2c2f36dcc42dc3569541da))
- Point sandbox notebook to Rust kernel - ([30e58e1](https://code.ornl.gov/research-enablement/xylem/commit/30e58e1a60f754a38172e2a1ca17b290cdedaff7))
- Add rustfmt config; format code - ([b1b048e](https://code.ornl.gov/research-enablement/xylem/commit/b1b048e9f0f6662ce01b67835c9e4ee4f004a6dd))

### 🐛 Bug Fixes

- *(ci)* Update doc generation - ([6267836](https://code.ornl.gov/research-enablement/xylem/commit/6267836f0883ca8d9fe993391e6e632d58af3246))
- *(ci)* Configure doc deployment to load static files - ([bb16376](https://code.ornl.gov/research-enablement/xylem/commit/bb16376a4ede21b2d466039594fbd1da18a4de5b))
- *(ci)* Move generated docs to site/html - ([4e22482](https://code.ornl.gov/research-enablement/xylem/commit/4e22482c3f7e0da67c66dd1d2f52579b0edabba2))
- *(ci)* Remove tarpaulin coverage from CI - ([3fa7a16](https://code.ornl.gov/research-enablement/xylem/commit/3fa7a1600bef9417bd17e460c0ec1d161faa135b))
- *(ci)* Pass some options to cargo binstall - ([e28e006](https://code.ornl.gov/research-enablement/xylem/commit/e28e0063c45d7ee96cb4ecbc28174be348e7047f))
- *(ci)* Install more dev dependencies - ([bfc32ac](https://code.ornl.gov/research-enablement/xylem/commit/bfc32acc81ddd11735e2cdcd64589b72d61c3dd6))
- *(ci)* Install dev dependencies - ([f1240b7](https://code.ornl.gov/research-enablement/xylem/commit/f1240b7fd0ef3ce120c5ba48b2cdaf8af6ed3b6e))
- *(docs)* Add quotes around niki given name - ([4bd5c12](https://code.ornl.gov/research-enablement/xylem/commit/4bd5c12ba94059916984582ab2d61b10b22d2ce1))
- *(docs)* Add middle initial to niki citation - ([a9cf41b](https://code.ornl.gov/research-enablement/xylem/commit/a9cf41b3b09522f93bd2daec4f47a2d5edfca606))
- *(tests)* Remove doctest that relies on environment and is inconsistent - ([5015195](https://code.ornl.gov/research-enablement/xylem/commit/5015195791e04fddfa2dd0d89fbcd7e6917ed378))
- *(tests)* Try to fix Command::test doctests - ([3e53c61](https://code.ornl.gov/research-enablement/xylem/commit/3e53c61607bc27f75433caab55a820076ba6976b))
- *(tests)* Refactor is_command to not panic - ([040502f](https://code.ornl.gov/research-enablement/xylem/commit/040502f74ca3b06f435fcee6088fcc01a607821b))
- *(tests)* Make is_command conda environment test conditional - ([04a74c2](https://code.ornl.gov/research-enablement/xylem/commit/04a74c2abdd53b41e93d99f7d09bc3c2871a231f))
- *(tests)* Refine test configuration - ([947f61c](https://code.ornl.gov/research-enablement/xylem/commit/947f61c561f3b47e3dde35657b9f9b380757dab9))
- Replace dotenv with dotenvy to clear security issue - ([fa8191f](https://code.ornl.gov/research-enablement/xylem/commit/fa8191f89edbc6824e73cfac0d90467d9da04f0c))
- Improve get_conda_environment_list testability - ([42d3a2b](https://code.ornl.gov/research-enablement/xylem/commit/42d3a2bd6c64c700a42324c5424a583e12f62aeb))
- Remove undeclared variable from Script::run error branch - ([38302f7](https://code.ornl.gov/research-enablement/xylem/commit/38302f7edfb7d5d7584c0cbedc639ad1dab05ee4))
- Enable ModuleUri hash support for URLs with no path - ([39c9641](https://code.ornl.gov/research-enablement/xylem/commit/39c964193084187312bc74591c5b12ff6d067121))
- Clear lint issues - ([01f7c9b](https://code.ornl.gov/research-enablement/xylem/commit/01f7c9b0608209451b9680dd1fdfda5c8377a574))
- Remove Command modifier attribute, allow scripts to run inside Conda virtual environments - ([5a4a34d](https://code.ornl.gov/research-enablement/xylem/commit/5a4a34d153fce6d9fff54075338684a1540b9455))
- Update ModuleUri working_directory method for remote script modules - ([5eafa28](https://code.ornl.gov/research-enablement/xylem/commit/5eafa28d1700182d5c624dc3f1ad62e76036d3f0))
- Make command modifier string and finish integrating - ([c81bab1](https://code.ornl.gov/research-enablement/xylem/commit/c81bab14b10763ecf82d711029d41080465f3769))
- Add environment support to Script from_template - ([18105a1](https://code.ornl.gov/research-enablement/xylem/commit/18105a19beb05f211c2e22f666e41821aae9247d))
- Add makefile for real - ([d5421d0](https://code.ornl.gov/research-enablement/xylem/commit/d5421d0e55adc15b86781d5a9bd5814857c1c063))
- Make midas run using template check - ([d58d2c0](https://code.ornl.gov/research-enablement/xylem/commit/d58d2c02bcfd3e9b6357ac68aa8313b9f2399b83))
- Remove more instances of gerber string - ([a84f970](https://code.ornl.gov/research-enablement/xylem/commit/a84f97033824c2478d29f666fdfd704edc515824))
- Update ASCII art - ([e10fc25](https://code.ornl.gov/research-enablement/xylem/commit/e10fc25dbaf535d12e7439e00dd308ae1be6696c))

### 📚 Documentation

- Update ROADMAP - ([2668ae7](https://code.ornl.gov/research-enablement/xylem/commit/2668ae7bd0f5c315c405c07196f6712a0fd9e74c))
- Add citation file and license - ([acad0c9](https://code.ornl.gov/research-enablement/xylem/commit/acad0c978390d4e559a9a246b5949148e432041d))
- Add API server item to ROADMAP - ([925c9ae](https://code.ornl.gov/research-enablement/xylem/commit/925c9ae3f53d5197ab965c559f555109800a99fb))
- Ad some items to ROADMAP - ([0d093ef](https://code.ornl.gov/research-enablement/xylem/commit/0d093ef5925ba388c37306feede92b16aeb7de7e))
- Move roadmap into separate file - ([abaed61](https://code.ornl.gov/research-enablement/xylem/commit/abaed6190083bdaa39cceeb4393d1e970e6a2ab5))
- Improve ModuleUri docs and remove a println - ([b6aeee7](https://code.ornl.gov/research-enablement/xylem/commit/b6aeee78c0b88c0ab2677eb8f1f0203efbef3af2))
- Update roadmap - ([8e8ae7a](https://code.ornl.gov/research-enablement/xylem/commit/8e8ae7a75179ad7704ddab5dbc8a695724d1ee6b))
- Update CONTRIBUTING guide - ([12f5c9a](https://code.ornl.gov/research-enablement/xylem/commit/12f5c9a696b17e1852635864fc6acd9dc608aaa6))
- Initial addition of roadmap to README - ([56ad21c](https://code.ornl.gov/research-enablement/xylem/commit/56ad21c6aef0e40383615096f04ff585d0e3e8c6))
- Minor error output tweak - ([9354c81](https://code.ornl.gov/research-enablement/xylem/commit/9354c81a66d983bf57d4d70eb8c2f629a90fd956))
- Add documentation for using local file URIs - ([d327b5c](https://code.ornl.gov/research-enablement/xylem/commit/d327b5c3ad2faaa047d5b9afe7a76e98979bb260))
- Incorporate parallelism into text logo - ([81de41c](https://code.ornl.gov/research-enablement/xylem/commit/81de41c17e73627f0c96069e41a6efa2ec4587d4))
- More improvements to PIPE text logo - ([f8f6df4](https://code.ornl.gov/research-enablement/xylem/commit/f8f6df4ef672de9c895f96bc1cdb4eaf272b88e9))
- Remove some unnecessary PIPE_ in variable names - ([909d6eb](https://code.ornl.gov/research-enablement/xylem/commit/909d6ebfc10970fd2bc618c0a898506d7f8fbb6b))
- Improve PIPE text logo - ([9281492](https://code.ornl.gov/research-enablement/xylem/commit/9281492de28c31bf094301f34beea6c91d82b11d))
- Remove obviated comment - ([da750b7](https://code.ornl.gov/research-enablement/xylem/commit/da750b7aff013a02623c8003c53a689f9cc00df4))
- Add docs to util file - ([9731a6c](https://code.ornl.gov/research-enablement/xylem/commit/9731a6cb07307430d15323221014b77f2342193d))
- Create CONTRIBUTING file - ([d1a893f](https://code.ornl.gov/research-enablement/xylem/commit/d1a893f92b3d84d93abd9b480118efadcba38ac5))

### 🧪 Testing

- Update config files - ([d0b4196](https://code.ornl.gov/research-enablement/xylem/commit/d0b4196c267cef8821a807048063fe612824663b))
- Generate test coverage - ([3779391](https://code.ornl.gov/research-enablement/xylem/commit/3779391dbf172d016ce5afba3080309608ba9b33))
- Update coverage - ([7080a0a](https://code.ornl.gov/research-enablement/xylem/commit/7080a0a199e205cfe65f11d72f27cd78ec79bdbd))
- Update lcov report - ([bce747d](https://code.ornl.gov/research-enablement/xylem/commit/bce747d224749783302e72df0dab3dbf30103ac8))
- Add test for Script::from_template run - ([4a1808e](https://code.ornl.gov/research-enablement/xylem/commit/4a1808e8a953b3d5d8b4b330df85635844dbc671))
- Add coverage report - ([7483890](https://code.ornl.gov/research-enablement/xylem/commit/7483890090f87e3fae196ca01c5e9afc7dcb2f1e))
- Run tests - ([e0a4933](https://code.ornl.gov/research-enablement/xylem/commit/e0a493363e11f621c2dbc406fb8d97778be7b6fa))

### ⚙️ Miscellaneous Tasks

- Add release jobs - ([695970a](https://code.ornl.gov/research-enablement/xylem/commit/695970a1827957481ceb7ac97c646d2408af5ba4))
- Add CI config - ([4d5a532](https://code.ornl.gov/research-enablement/xylem/commit/4d5a532fae49a36bcab71d4263885efba1fa8bf3))

### 🚧 In Progress

- *(ci)* Install tarpaulin with cargo - ([a2a1089](https://code.ornl.gov/research-enablement/xylem/commit/a2a1089333893146f51d05cdf498fdcc58ab9a94))
- *(ci)* Explicitly disable inheriting default - ([ad9fc8a](https://code.ornl.gov/research-enablement/xylem/commit/ad9fc8abf245b4d37227445ccf4488b66643c5b1))
- Remove pipe-py from workspace until I can fix cc linking error - ([64c7cab](https://code.ornl.gov/research-enablement/xylem/commit/64c7cab6f00c5f308c2450221e3150369da7ac51))
- Add more functionality to link checking with tokio - ([cd3573c](https://code.ornl.gov/research-enablement/xylem/commit/cd3573ccbcbbd5ac1a909d571f2a77b6e8ea5c34))
- Add link checking functionality - ([b5aa64d](https://code.ornl.gov/research-enablement/xylem/commit/b5aa64d4dd1f3afcac97906adaba21412975327c))
- Add check to configuration - ([9ac653d](https://code.ornl.gov/research-enablement/xylem/commit/9ac653daccf4e27cde43e0d48f9df8e7d0a444f8))
- Prepare to add YAML support - ([64f571e](https://code.ornl.gov/research-enablement/xylem/commit/64f571e0638f4577829b284ba8b8e75667b4067c))
- Build out Script::run to provide better logging - ([6d84c64](https://code.ornl.gov/research-enablement/xylem/commit/6d84c64ed440c59a4b2f9480b02e0a11164a0182))
- Add build tasks ISO publishing binary releases - ([c43fa84](https://code.ornl.gov/research-enablement/xylem/commit/c43fa8406a62e3d07ad79bc974af666e17474c45))
- Initial scaffolding of is_valid_python_module check - ([e91d99a](https://code.ornl.gov/research-enablement/xylem/commit/e91d99a5b522e6f8080b696b957fb5149c4c433f))
- Remove par_iter b/c modules should not be parallel - ([8eb47ca](https://code.ornl.gov/research-enablement/xylem/commit/8eb47caa09f96060fe29485acc2929ed364a6647))
- Add todo for env check and add par_iter just for fun - ([1cfbd24](https://code.ornl.gov/research-enablement/xylem/commit/1cfbd24f4d7326cd0c3ecb4ad39c54872cfcfbd0))
- Initial addition of script run from virtual env creation - ([95dc52d](https://code.ornl.gov/research-enablement/xylem/commit/95dc52d03b407f4d46e44f8878a2399be7cb8084))
- Adding virtual environment installation - ([77e1761](https://code.ornl.gov/research-enablement/xylem/commit/77e17614a28a4cea9f1727fd068f13258fa21cf7))
- Start abstracting working directory creation - ([d30e8c2](https://code.ornl.gov/research-enablement/xylem/commit/d30e8c219f7483ceada72329e58d689815776717))
- Integrate command modifiers - ([15f9b6d](https://code.ornl.gov/research-enablement/xylem/commit/15f9b6d56ac191d2bfd7dace585a20fc0354e7fa))
- Get name from Conda environment.yml - ([4dca3c1](https://code.ornl.gov/research-enablement/xylem/commit/4dca3c10918225dfcb30b81b7e2c6303268b8e1d))
- Environrment variable expansion - ([9a35b89](https://code.ornl.gov/research-enablement/xylem/commit/9a35b89ea7b3f270afdecff3a35af50af963aa4f))
- Still working on pipe run - ([282e7f9](https://code.ornl.gov/research-enablement/xylem/commit/282e7f9a51e8cc9efc46a3730509b4822c421075))
- Add todo comments for tracking short-term feature roadmap in run - ([b43bb1b](https://code.ornl.gov/research-enablement/xylem/commit/b43bb1bd587952709ad9b0871026ac75afd59fd5))
- Working on pipe run - ([fd49552](https://code.ornl.gov/research-enablement/xylem/commit/fd49552310bbb729873b81fb602907da84ba9b93))
- Refactor env var set; start env var get - ([9f264b8](https://code.ornl.gov/research-enablement/xylem/commit/9f264b8df5fb1d0163e47770d16519575fac9e6b))
- Working on environment variable functions - ([3abc536](https://code.ornl.gov/research-enablement/xylem/commit/3abc536e621089abea3d767b7d43ecf62e2743e1))
- Working on setting env vars - ([b699260](https://code.ornl.gov/research-enablement/xylem/commit/b6992605f6e55113c7efafcb809416e23360f583))
- Start working on parsing URIs - ([2e3db40](https://code.ornl.gov/research-enablement/xylem/commit/2e3db4031ae721e8a92b90c3bff12c2278582786))
- Add placeholder functions for handlebars functionality - ([d6ed8a4](https://code.ornl.gov/research-enablement/xylem/commit/d6ed8a455634a369a4a75f6515489dc2597542c2))
- Add windows support to is_command - ([4eb373d](https://code.ornl.gov/research-enablement/xylem/commit/4eb373d2ca0f4adfa754d23e35d9ee5141a88ee7))
- Add comment for is_command windows support - ([7a744dd](https://code.ornl.gov/research-enablement/xylem/commit/7a744dd38aa13a672fadf016ca8a489cf7e0fe73))
- Toward configuration v1 - ([5a6d08f](https://code.ornl.gov/research-enablement/xylem/commit/5a6d08f3773496d0fab3455e09eef624a642d7b4))
- Update config test fixture - ([946ff47](https://code.ornl.gov/research-enablement/xylem/commit/946ff47ccba3652a400f5c4051da9bc5051fd91f))
- Add build and test files - ([b2ad773](https://code.ornl.gov/research-enablement/xylem/commit/b2ad773b1d9c1a3c63efa33649a47370f0bda7cb))
- Initial creation of config Struct - ([c67c59f](https://code.ornl.gov/research-enablement/xylem/commit/c67c59fa83bb40578f86f71ecba22a0f3497242b))

### 🤓 Developer Experience

- Add makefile - ([1fa19be](https://code.ornl.gov/research-enablement/xylem/commit/1fa19be92e53d7f5626b686821da21feff460b10))
