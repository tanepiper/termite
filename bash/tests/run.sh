#!/usr/bin/env bash
# Lint and test the termite bash library: shellcheck, then bats.
# Usage: bash/tests/run.sh
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

shellcheck --external-sources --source-path=SCRIPTDIR \
  preamble.sh lib/termite/*.sh examples/* tests/run.sh tests/test_helper.bash tests/*.bats
bats --print-output-on-failure tests
