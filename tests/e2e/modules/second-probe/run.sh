#!/bin/sh
set -eu

test "$1" = "Goodbye, workflow!"
test "$2" = "environment_probe,second_probe"
test "${_XYLEM_MESSAGE}" = "Goodbye, workflow!"
test "${_XYLEM_MODULE_LIST}" = "environment_probe,second_probe"

printf '%s\n' "XYLEM_E2E_TWO_OK"
