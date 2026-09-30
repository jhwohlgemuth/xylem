# Roadmap

This roadmap contains only forward-looking milestones. Detailed implementation
plans and their dependency order live in the [planning wiki](./.wiki/index.md).

Current 1.x support is intentionally narrower: Linux is the rigorously tested
release target, Homebrew is tested on Linux and macOS, and the local POSIX-shell
workflow is the baseline offline path. Items below are aspirations, not current
support claims.

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
