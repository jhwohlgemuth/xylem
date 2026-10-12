# Module contract

A module is either a script project or an executable binary. Each declaration
has a name, type, URI, and optional variables and command template.

Local modules use `file:` URIs. Remote scripts use supported Git transports and
must name a full immutable commit. Remote binaries normally require an
algorithm-qualified SHA-256 checksum. The operator-only
`--skip-verify-checksum` emergency escape hatch permits an unpinned binary,
emits a warning, and records its observed digest. Cleartext HTTP additionally
requires `--allow-insecure-http`.

Every remote host must appear in `options.allowed` or the CLI `--allowed`
override. Verified remote content is stored in an immutable cache, copied into
the owned run workspace, and recorded in the run summary. Offline runs may
reuse verified cache entries but never contact the source.

Command templates declare a command and its argument list. Prefer the array
form for arguments because every item remains one process argument after
variable interpolation. Templates may also select a Pixi or Conda environment
and a supported container runtime.

The authoritative, versioned contract is
[MODULE_SPEC.md](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md).
It defines module fields, URI handling, variable precedence, environment
managers, binary verification, and the conformance checklist.
