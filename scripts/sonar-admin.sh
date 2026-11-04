#!/usr/bin/env bash
# One-off SonarQube set-up for termite: a strict quality profile and gate.
# Safe to run again; it brings both back to the state below.
#
# Profiles "termite strict" (Rust, Secrets): every rule the server has for
# the language, not a subset.
#
# Gate "termite strict":
#   overall:  no issues at all, A ratings for security, reliability and
#             maintainability, every security hotspot reviewed, coverage >= 70%
#   new code: no new issues, coverage >= 80%, duplication <= 3%, every hotspot
#             reviewed
#
# Needs an administrator's user token (an analysis token cannot change
# profiles or gates), from SONARQUBE_ADMIN_TOKEN in .env or the Keychain
# service "termite-sonarqube-admin".
#
# Usage: scripts/sonar-admin.sh [SONAR_HOST_URL]   default http://localhost:9000
set -euo pipefail

host=${1:-http://localhost:9000}
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
project=termite
name='termite strict'

fail() { printf 'sonar-admin.sh: %s\n' "$*" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || fail 'needs jq'

token=
if [[ -f $root/.env ]]; then
  token=$(sed -n 's/^SONARQUBE_ADMIN_TOKEN=//p' "$root/.env" | head -n 1)
fi
if [[ -z $token ]] && ! token=$(security find-generic-password -a "$USER" -s termite-sonarqube-admin -w 2>/dev/null); then
  fail 'no administrator token; see the comment at the top of this script'
fi

# The token goes to curl on stdin as a config line, never on the command line.
api() {
  local method=$1 path=$2; shift 2
  printf 'user = "%s:"\n' "$token" |
    curl --silent --show-error --fail-with-body --config - -X "$method" "$host/api/$path" "$@"
}

api GET users/current | jq -e '.permissions.global | index("admin")' >/dev/null ||
  fail 'the token is not an administrator token'

# --- the project (so the gate and profiles can be attached before a scan) ---
if [[ $(api GET "projects/search?projects=$project" | jq '.components | length') == 0 ]]; then
  api POST projects/create --data-urlencode "project=$project" --data-urlencode "name=$project" >/dev/null
  printf 'created project %s\n' "$project"
fi

# --- quality profiles: every rule ---------------------------------------
for lang in rust secrets; do
  key=$(api GET "qualityprofiles/search?language=$lang" |
        jq -r --arg n "$name" '.profiles[] | select(.name == $n) | .key')
  if [[ -z $key ]]; then
    key=$(api POST qualityprofiles/create --data-urlencode "language=$lang" --data-urlencode "name=$name" |
          jq -r .profile.key)
  fi
  api POST qualityprofiles/activate_rules --data-urlencode "targetKey=$key" \
    --data-urlencode "languages=$lang" --data-urlencode "is_template=false" >/dev/null
  api POST qualityprofiles/add_project --data-urlencode "language=$lang" \
    --data-urlencode "qualityProfile=$name" --data-urlencode "project=$project" >/dev/null
  count=$(api GET "qualityprofiles/search?language=$lang" |
          jq -r --arg n "$name" '.profiles[] | select(.name == $n) | .activeRuleCount')
  total=$(api GET "rules/search?languages=$lang&is_template=false&ps=1" | jq .total)
  printf 'profile %s (%s): %s of %s rules active\n' "$name" "$lang" "$count" "$total"
  [[ $count == "$total" ]] || fail "not every $lang rule is active"
done

# --- quality gate ---------------------------------------------------------
if ! api GET "qualitygates/show?name=$(jq -rn --arg n "$name" '$n|@uri')" >/dev/null 2>&1; then
  api POST qualitygates/create --data-urlencode "name=$name" >/dev/null
fi
# Start from no conditions, so a re-run gives exactly the list below.
api GET "qualitygates/show?name=$(jq -rn --arg n "$name" '$n|@uri')" | jq -r '.conditions[].id' |
  while read -r id; do api POST qualitygates/delete_condition --data-urlencode "id=$id" >/dev/null; done

conditions=(
  # metric                                   op  error
  'violations                                GT  0'
  'software_quality_security_rating          GT  1'
  'software_quality_reliability_rating       GT  1'
  'software_quality_maintainability_rating   GT  1'
  'security_hotspots_reviewed                LT  100'
  'coverage                                  LT  70'
  'new_violations                            GT  0'
  'new_coverage                              LT  80'
  'new_duplicated_lines_density              GT  3'
  'new_security_hotspots_reviewed            LT  100'
)
for line in "${conditions[@]}"; do
  read -r metric op error <<<"$line"
  api POST qualitygates/create_condition --data-urlencode "gateName=$name" \
    --data-urlencode "metric=$metric" --data-urlencode "op=$op" --data-urlencode "error=$error" >/dev/null
done
api POST qualitygates/select --data-urlencode "gateName=$name" --data-urlencode "projectKey=$project" >/dev/null
printf 'gate %s: %s conditions, attached to %s\n' "$name" "${#conditions[@]}" "$project"
