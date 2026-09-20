#!/bin/sh
set -eu
printf 'XYLEM_DOTENV_TEST=%s\n' "${XYLEM_DOTENV_TEST:-missing}"
