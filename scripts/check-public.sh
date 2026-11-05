#!/usr/bin/env bash
# A second, independent check on the data the public site shows: no text from
# the dangerous end of the corpus may be in it. `eval publish` checks exact
# strings; this checks shapes, so it also catches what that check never knew.
#
# Usage: scripts/check-public.sh [FILE]   default site/src/data/results.json
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
file=${1:-$root/site/src/data/results.json}

patterns=(
  '/dev/tcp' 'base64' 'curl ' 'wget ' 'nc -' 'ncat' 'socat' 'mkfifo'
  '[|] *(sh|bash|zsh)' '://' 'authorized_keys' 'crontab' 'launchctl'
  'LaunchAgents' 'csrutil' 'spctl' 'pfctl' 'socketfilterfw' 'dump-keychain'
  'find-generic-password' 'example[.]test' '/Users/' '[.]config/termite'
)

found=0
for pattern in "${patterns[@]}"; do
  if grep -Eq -- "$pattern" "$file"; then
    printf 'check-public.sh: found %s in %s\n' "$pattern" "$file" >&2
    found=1
  fi
done

if [[ $found -ne 0 ]]; then
  printf 'check-public.sh: not safe to publish\n' >&2
  exit 1
fi
printf 'check-public.sh: nothing dangerous in %s\n' "$file"
