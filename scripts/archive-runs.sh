#!/usr/bin/env bash
# Keep the raw runs safe. They hold withheld commands, so they never go in the
# repository: they are copied to ~/termite-private/runs, and to a private
# dataset on Hugging Face, along with the observations. Existing copies are never overwritten. The private
# reports stay on this machine. The upload stops unless the dataset says it is
# private.
#
# Usage: scripts/archive-runs.sh [--local]
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
home=$HOME/termite-private
dataset=termite-eval-runs
install -d -m 700 "$home" "$home/runs" "$home/observations"
cp -n "$root"/eval/results/*.json "$root"/eval/results/team/*.json "$home/runs/" 2>/dev/null || true
cp -n "$root"/eval/observations/*.jsonl "$home/observations/" 2>/dev/null || true
chmod 600 "$home"/runs/*.json "$home"/observations/*.jsonl
printf 'archive-runs.sh: %s raw runs in %s\n' "$(find "$home/runs" -type f | wc -l | tr -d ' ')" "$home/runs" >&2
[[ ${1:-} == --local ]] && exit 0

export HF_TOKEN_VALUE
HF_TOKEN_VALUE=$(sed -n 's/^HUGGINGFACE_API=//p' "$root/.env")
[[ -n $HF_TOKEN_VALUE ]] || { printf 'archive-runs.sh: no key\n' >&2; exit 2; }
exec python3 -I "$root/scripts/archive_upload.py" "$dataset" "$home" "$root/eval/dataset-card.md"
