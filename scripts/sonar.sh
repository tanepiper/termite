#!/usr/bin/env bash
# Analyse termite with a local SonarQube.
#
# Steps: coverage (cargo-llvm-cov, which runs every test including the bash
# library's), ShellCheck, Semgrep and cargo-audit findings converted to Sonar's
# generic issue format, then sonar-scanner. Sonar runs Clippy itself. The
# run fails when the quality gate fails.
#
# The token is read from the macOS Keychain (service "termite-sonarqube"),
# so it is never typed into a command line, printed, or stored in a file.
# Create it once in SonarQube (My Account > Security > a "Project Analysis"
# token for the project "termite"), then store it with:
#
#   security add-generic-password -a "$USER" -s termite-sonarqube -w
#
# (security prompts for the value.)
#
# Needs: brew install sonar-scanner cargo-llvm-cov cargo-audit shellcheck semgrep jq
#        rustup component add llvm-tools-preview
#
# Usage: scripts/sonar.sh [SONAR_HOST_URL]   default http://localhost:9000
set -euo pipefail

host=${1:-http://localhost:9000}
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
out=$root/target/sonar

fail() { printf 'sonar.sh: %s\n' "$*" >&2; exit 1; }

for tool in sonar-scanner cargo-llvm-cov cargo-audit shellcheck semgrep jq; do
  command -v "$tool" >/dev/null 2>&1 || fail "needs $tool (see the comment at the top of this script)"
done

token=
# A local .env (git-ignored) may hold SONARQUBE_TOKEN=...; read, never sourced.
if [[ -f $root/.env ]]; then
  token=$(sed -n 's/^SONARQUBE_TOKEN=//p' "$root/.env" | head -n 1)
fi
if [[ -z $token ]] && ! token=$(security find-generic-password -a "$USER" -s termite-sonarqube -w 2>/dev/null); then
  fail 'no token in the Keychain; see the comment at the top of this script'
fi

cd -- "$root"
rm -rf -- "$out"
mkdir -p -- "$out"

# --- coverage -------------------------------------------------------------
printf 'sonar.sh: running the tests with coverage\n' >&2
cargo llvm-cov --all-targets --lcov --output-path "$out/lcov.info"

# --- ShellCheck -----------------------------------------------------------
printf 'sonar.sh: running ShellCheck\n' >&2
shell_files=(bash/preamble.sh bash/lib/termite/*.sh bash/examples/* bash/tests/*.bats
             bash/tests/run.sh scripts/*.sh docs/tapes/record.sh)
# ShellCheck exits 1 when it finds something; that is a finding, not a failure.
shellcheck --format=json1 --external-sources --source-path=SCRIPTDIR --source-path=bash \
  "${shell_files[@]}" >"$out/shellcheck.raw.json" || [[ $? -eq 1 ]]
jq '
  def quality: if .level == "style" or .level == "info" then "MAINTAINABILITY" else "RELIABILITY" end;
  def severity: {error: "HIGH", warning: "MEDIUM", info: "LOW", style: "LOW"}[.level];
  {
    rules: (.comments | unique_by(.code) | map({
      id: "SC\(.code)", name: "ShellCheck SC\(.code)", engineId: "shellcheck",
      description: "https://www.shellcheck.net/wiki/SC\(.code)",
      cleanCodeAttribute: "CONVENTIONAL",
      impacts: [{softwareQuality: quality, severity: severity}]
    })),
    issues: (.comments | map({
      ruleId: "SC\(.code)",
      primaryLocation: {message: .message, filePath: .file, textRange: {startLine: .line}}
    }))
  }' "$out/shellcheck.raw.json" >"$out/shellcheck.json"

# --- Semgrep --------------------------------------------------------------
printf 'sonar.sh: running Semgrep\n' >&2
# semgrep.sh exits 1 when it finds something; that is a finding, not a failure.
scripts/semgrep.sh --json "$out/semgrep.json" || [[ $? -eq 1 ]]

# --- cargo-audit ----------------------------------------------------------
printf 'sonar.sh: running cargo-audit\n' >&2
# cargo-audit exits 1 when it finds a vulnerability; that is a finding too.
cargo audit --json >"$out/cargo-audit.raw.json" || [[ $? -eq 1 ]]
# Line of each package's [[package]] name in Cargo.lock, for the issue location.
lock_lines=$(awk '/^name = "/ { gsub(/"/, "", $3); print $3, NR }' Cargo.lock | jq -Rn '[inputs | split(" ") | {(.[0]): (.[1] | tonumber)}] | add')
jq --argjson lines "$lock_lines" '
  [ (.vulnerabilities.list[]? | {kind: "vulnerability", a: .advisory, p: .package}),
    (.warnings // {} | to_entries[] | .key as $k | .value[]? | {kind: $k, a: .advisory, p: .package}) ]
  | map(select(.a != null)) as $found
  | {
      rules: ($found | unique_by(.a.id) | map({
        id: .a.id, name: .a.title, engineId: "cargo-audit",
        description: (.a.url // "https://rustsec.org/advisories/\(.a.id)"),
        cleanCodeAttribute: "TRUSTWORTHY",
        impacts: [{softwareQuality: "SECURITY",
                   severity: (if .kind == "vulnerability" then "HIGH" else "MEDIUM" end)}]
      })),
      issues: ($found | map({
        ruleId: .a.id,
        primaryLocation: {
          message: "\(.p.name) \(.p.version): \(.a.title) (\(.kind))",
          filePath: "Cargo.lock",
          textRange: {startLine: ($lines[.p.name] // 1)}
        }
      }))
    }' "$out/cargo-audit.raw.json" >"$out/cargo-audit.json"

printf 'sonar.sh: %s ShellCheck, %s Semgrep and %s cargo-audit findings\n' \
  "$(jq '.issues | length' "$out/shellcheck.json")" "$(jq '.issues | length' "$out/semgrep.json")" \
  "$(jq '.issues | length' "$out/cargo-audit.json")" >&2

# --- analysis -------------------------------------------------------------
SONAR_TOKEN=$token sonar-scanner -Dsonar.host.url="$host"
