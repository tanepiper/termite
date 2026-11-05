#!/usr/bin/env bash
# The big analysis step: is termite getting better or worse?
#
# Runs the tests with coverage and the model evaluation inside the sandbox,
# reads SonarQube's numbers if it is running, saves one record per run, and
# builds eval/report.html. Open that file to see the results.
#
# Usage: scripts/analyse.sh [--no-models]
#
# Without --no-models this asks every model in eval/models.jsonl about every
# request, which takes a long time. Nothing a model writes is ever run.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$root"

mode=run
if [[ ${1:-} == --no-models ]]; then
  mode=policy
fi

for tool in cargo-llvm-cov jq; do
  command -v "$tool" >/dev/null 2>&1 || { printf 'analyse.sh: needs %s\n' "$tool" >&2; exit 1; }
done

commit=$(git rev-parse --short HEAD)
if [[ -n $(git status --porcelain) ]]; then
  commit=$commit-dirty
fi
started=$(date +%s)
out=target/analyse
mkdir -p "$out"

printf 'analyse.sh: proving the sandbox holds\n' >&2
sandbox=failed
if scripts/sandbox.sh --check >"$out/sandbox.txt" 2>&1; then
  sandbox=holds
else
  cat "$out/sandbox.txt" >&2
  exit 1
fi

printf 'analyse.sh: tests with coverage, in the sandbox\n' >&2
scripts/sandbox.sh cargo llvm-cov --all-targets --json --summary-only \
  --output-path "$out/coverage.json" 2>&1 | tee "$out/tests.txt" >&2
coverage=$(jq '.data[0].totals.lines.percent' "$out/coverage.json")
tests=$(sed -nE 's/^test result: ok\. ([0-9]+) passed.*/\1/p' "$out/tests.txt" | paste -sd+ - | bc)

printf 'analyse.sh: model evaluation, in the sandbox\n' >&2
cargo build --example eval
scripts/sandbox.sh --termite-config target/debug/examples/eval "$mode" --commit "$commit"

# SonarQube's last analysis, if it is up. The token is read, never printed.
issues=null gate=unknown
if [[ -f .env ]]; then
  token=$(sed -n 's/^SONARQUBE_ADMIN_TOKEN=//p' .env | head -n 1)
  host=http://localhost:9000
  if measures=$(curl -fsS -m 5 -u "$token:" "$host/api/issues/search?componentKeys=termite&resolved=false&ps=1" 2>/dev/null); then
    issues=$(jq '.total' <<<"$measures")
    status=$(curl -fsS -m 5 -u "$token:" "$host/api/qualitygates/project_status?projectKey=termite")
    gate=$(jq -r '.projectStatus.status' <<<"$status")
  fi
fi

jq -n --argjson started "$started" --arg commit "$commit" --argjson tests "$tests" \
  --argjson coverage "$coverage" --argjson issues "$issues" --arg gate "$gate" --arg sandbox "$sandbox" \
  '{started: $started, commit: $commit, tests: $tests, coverage: $coverage,
    sonar_issues: $issues, gate: $gate, sandbox: $sandbox}' \
  >"eval/results/quality-$started-$commit.json"

target/debug/examples/eval report
if [[ $mode == run ]]; then
  target/debug/examples/eval publish
  target/debug/examples/eval private
  scripts/check-public.sh
  scripts/archive-runs.sh
fi
printf 'analyse.sh: open eval/report.html; private reports are in ~/termite-private\n' >&2
