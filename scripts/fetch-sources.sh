#!/usr/bin/env bash
# Fetch the outside data and model weights the evaluation uses, as listed in
# eval/sources.jsonl: pinned to one revision, each checked against its
# SHA-256. These repositories are public, so no key is sent. Weights are GGUF
# only, and no code from a repository is ever run.
#
# Usage: scripts/fetch-sources.sh [--models]   datasets, or datasets and weights
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
models=${1:-}
downloads=$root/eval/downloads

while IFS= read -r line; do
  kind=$(jq -r .kind <<<"$line")
  [[ $kind == datasets || $models == --models ]] || continue
  repo=$(jq -r .repo <<<"$line")
  revision=$(jq -r .revision <<<"$line")
  file=$(jq -r .file <<<"$line")
  want=$(jq -r .sha256 <<<"$line")
  size=$(jq -r .bytes <<<"$line")
  [[ $kind == datasets ]] && prefix=datasets/ || prefix=

  target=$downloads/$repo/$file
  mkdir -p -- "$(dirname -- "$target")"
  if [[ ! -f $target || $(stat -f %z "$target") -ne $size ]]; then
    printf 'fetch-sources.sh: %s %s\n' "$repo" "$file" >&2
    curl --fail --location --silent --show-error --continue-at - \
      --output "$target" \
      "https://huggingface.co/${prefix}${repo}/resolve/${revision}/${file}"
  fi

  got=$(shasum -a 256 "$target" | cut -d' ' -f1)
  if [[ -n $want && $got != "$want" ]]; then
    rm -f -- "$target"
    printf 'fetch-sources.sh: %s does not match its checksum; removed\n' "$file" >&2
    exit 1
  fi
  [[ -n $want ]] || printf 'fetch-sources.sh: %s sha256 %s (not yet pinned)\n' "$file" "$got" >&2
done <"$root/eval/sources.jsonl"
printf 'fetch-sources.sh: done\n' >&2
