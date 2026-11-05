#!/usr/bin/env bash
# Print the localhost ports the model servers listen on, one per line. These
# are the only local ports a model test may reach. They come from the model
# lists and your termite config, and only ever digits come out.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
config=${TERMITE_HOME:-$HOME/.config/termite}/config.json

{
  cat "$root/eval/models.jsonl" "$root/eval/decision-models.jsonl"
  if [[ -r $config ]]; then cat "$config"; fi
} | grep -Eo 'https?://(127\.0\.0\.1|localhost):[0-9]{1,5}' \
  | grep -Eo '[0-9]+$' | sort -un
