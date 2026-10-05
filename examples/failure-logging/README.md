# Failure logging

This intentionally failing workflow shows how logs from concurrent inputs and
ordered modules appear. It uses only a local POSIX shell and runs these stages
for `alpha`, `bravo`, `charlie`, and `delta`:

```text
Prepare → Validate → Publish
```

Prepare includes a short delay so all four inputs are admitted before a
failure can stop further admission. Validate rejects `bravo` and `delta` with
exit status 7. The other two inputs continue through Publish.

Run from any directory:

```shell
cargo run --example failure-logging
```

The command exits with status 70 because the failures are intentional. Output
from concurrent workers can appear in a different order, and run IDs and paths
change on every invocation. An abridged run looks like this:

```text
 INFO xylem-input-0 => USING Workflow input value="alpha" source="Configuration"
 INFO xylem-input-1 => USING Workflow input value="bravo" source="Configuration"
 INFO xylem-input-2 => USING Workflow input value="charlie" source="Configuration"
 INFO xylem-input-3 => USING Workflow input value="delta" source="Configuration"
 WARN xylem-input-0 => RUN ▶ script="sh run.sh prepare alpha"
TRACE xylem-input-0 => OUTPUT ["prepare: accepted alpha"]
 WARN xylem-input-1 => RUN ▶ script="sh run.sh validate bravo"
ERROR xylem-input-1 => ✗ FAIL script="sh run.sh validate bravo" status=exit status: 7 details="validate: intentionally rejected bravo"
 WARN xylem-input-2 => RUN ▶ script="sh run.sh publish charlie"
TRACE xylem-input-2 => OUTPUT ["publish: completed charlie"]
ERROR xylem-input-3 => ✗ FAIL script="sh run.sh validate delta" status=exit status: 7 details="validate: intentionally rejected delta"

Run <run-id>: failed
Inputs: 2 succeeded, 0 cancelled, 2 failed, 0 skipped, 0 skipped existing, 0 unfinished
Containers: 0 removed, 0 retained, 0 external, 0 failed
Input alpha: succeeded
  Module Prepare: succeeded
  Module Validate: succeeded
  Module Publish: succeeded
Input bravo: failed: ... process exited with status exit status: 7: validate: intentionally rejected bravo
  Module Prepare: succeeded
  Module Validate: failed: ... validate: intentionally rejected bravo
  Module Publish: not_started
Input charlie: succeeded
  Module Prepare: succeeded
  Module Validate: succeeded
  Module Publish: succeeded
Input delta: failed: ... process exited with status exit status: 7: validate: intentionally rejected delta
  Module Prepare: succeeded
  Module Validate: failed: ... validate: intentionally rejected delta
  Module Publish: not_started
Journal: <temporary-run-directory>/<run-id>.journal.jsonl
```

The module summary is grouped by input in configuration order: `alpha`,
`bravo`, `charlie`, then `delta`.
