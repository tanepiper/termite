#!/usr/bin/env bash
# The blue and red team tools, each run in the place it's allowed to run.
#
#   scripts/team.sh gate             where can each team work?
#   scripts/team.sh roster           which models each team uses
#   scripts/team.sh check 'COMMAND'  rate a command, no model involved
#   scripts/team.sh blue 'REQUEST'   build a safe command, in the sandbox
#   scripts/team.sh red 'REQUEST'    test a dangerous request, in a container
#
# Nothing a model writes is run. There's no way to skip a gate: the tool
# checks for itself where it is, and refuses.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$root"
eval_tool=target/debug/examples/eval

here() { [[ -x $eval_tool ]] && "$eval_tool" gate "$1" >/dev/null 2>&1; }

sandboxed() {
  if command -v sandbox-exec >/dev/null 2>&1; then
    scripts/sandbox.sh --models "$eval_tool" "$@"
  else
    scripts/container.sh "$@"
  fi
}

[[ -x $eval_tool ]] || cargo build --quiet --example eval

case ${1:-} in
  gate)
    printf 'Here:\n'
    "$eval_tool" gate
    printf '\nIn the sandbox:\n'
    sandboxed gate || true
    if command -v docker >/dev/null 2>&1; then
      printf '\nIn the container:\n'
      scripts/container.sh gate || true
    fi
    ;;
  roster)
    "$eval_tool" team roster
    ;;
  check | blue)
    [[ $# -eq 2 ]] || { printf 'usage: team.sh %s TEXT\n' "$1" >&2; exit 64; }
    if here blue; then "$eval_tool" team "$1" "$2"; else sandboxed team "$1" "$2"; fi
    ;;
  red)
    [[ $# -eq 2 ]] || { printf 'usage: team.sh red TEXT\n' >&2; exit 64; }
    if here red; then "$eval_tool" team red "$2"; else scripts/container.sh team red "$2"; fi
    ;;
  *)
    sed -n '2,9p' "$0" | cut -c3- >&2
    exit 64
    ;;
esac
