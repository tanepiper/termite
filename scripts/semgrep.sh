#!/usr/bin/env bash
# Run Semgrep on termite: first the tests for termite's own rules
# (semgrep/*.yaml with their cases), then a scan with those rules plus
# Semgrep's Rust registry pack. Metrics are always off.
#
# Usage: scripts/semgrep.sh [--local] [--json FILE]
#   --local      only termite's own rules: no registry download
#   --json FILE  also write the findings in SonarQube's generic issue format
#
# Exit status: 0 nothing found, 1 findings, 2 a rule test or the scan failed.
# Needs: brew install semgrep jq
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fail() { printf 'semgrep.sh: %s\n' "$*" >&2; exit 2; }

local_only=false
json=
while (($#)); do
  case $1 in
    --local) local_only=true ;;
    --json) json=${2:?--json needs a file}; shift ;;
    *) fail "unknown option $1 (usage: scripts/semgrep.sh [--local] [--json FILE])" ;;
  esac
  shift
done
for tool in semgrep jq; do
  command -v "$tool" >/dev/null 2>&1 || fail "needs $tool (brew install $tool)"
done
cd -- "$root"

semgrep --test --metrics=off --quiet semgrep/ >/dev/null 2>&1 ||
  fail 'a rule test failed; run: semgrep --test --metrics=off semgrep/'

tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
scan=(semgrep scan --metrics=off --disable-version-check --quiet --json)

configs=(--config semgrep/)
if ! $local_only; then
  # Semgrep's own "unsafe" rule is replaced by termite-unsafe-only-in-home.
  configs+=(--config p/rust --exclude-rule rust.lang.security.unsafe-usage.unsafe-usage)
fi
"${scan[@]}" "${configs[@]}" --output "$tmp/code.json" \
  src tests bash/lib bash/preamble.sh bash/tests scripts docs/tapes/record.sh
# Scripts without a .sh extension are only scanned when named, and only
# with the bash rules (the Rust rules cannot parse them).
"${scan[@]}" --config semgrep/termite-bash.yaml --scan-unknown-extensions \
  --output "$tmp/scripts.json" bash/examples/*

jq -s '{results: [.[].results[]], errors: [.[].errors[]]}' \
  "$tmp/code.json" "$tmp/scripts.json" >"$tmp/all.json"

errors=$(jq '.errors | length' "$tmp/all.json")
if ((errors > 0)); then
  jq -r '.errors[] | "  \(.type | tostring | .[0:60]) \(.path // "")"' "$tmp/all.json" >&2
  fail "the scan hit $errors errors"
fi

if [[ -n $json ]]; then
  jq '
    def severity: {ERROR: "HIGH", WARNING: "MEDIUM", INFO: "LOW"}[.extra.severity] // "MEDIUM";
    {
      rules: (.results | unique_by(.check_id) | map({
        id: .check_id, name: .check_id, engineId: "semgrep",
        description: .extra.message, cleanCodeAttribute: "TRUSTWORTHY",
        impacts: [{softwareQuality: "SECURITY", severity: severity}]
      })),
      issues: (.results | map({
        ruleId: .check_id,
        primaryLocation: {
          message: .extra.message, filePath: .path,
          textRange: {startLine: .start.line, endLine: .end.line}
        }
      }))
    }' "$tmp/all.json" >"$json"
fi

found=$(jq '.results | length' "$tmp/all.json")
if ((found > 0)); then
  jq -r '.results[] | "\(.path):\(.start.line): \(.check_id | split(".") | last): \(.extra.message)"' \
    "$tmp/all.json" >&2
  printf 'semgrep.sh: %s findings\n' "$found" >&2
  exit 1
fi
printf 'semgrep.sh: no findings\n' >&2
