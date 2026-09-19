# Module contract

A module is either a script project or an executable binary. Each declaration
has a name, type, URI, and optional variables and command template.

Local modules use `file:` URIs. Remote Python modules may use supported Git
transports, while remote binaries use HTTP or HTTPS and require a SHA-256
checksum unless the user explicitly selects the unsafe troubleshooting bypass.

Command templates declare a command and its argument list. Prefer the array
form for arguments because every item remains one process argument after
variable interpolation. Templates may also select a Pixi or Conda environment
and a supported container runtime.

The authoritative, versioned contract is
[MODULE_SPEC.md](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md).
It defines module fields, URI handling, variable precedence, environment
managers, binary verification, and the conformance checklist.
