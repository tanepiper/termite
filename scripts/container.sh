#!/usr/bin/env bash
# Run the eval tool in a locked-down Docker container: no internet, read-only
# filesystem, no privileges, no secrets, no source. Only the saved answers
# folder can be written to. The one way out is a bridge to the model ports,
# which forwards those and nothing else, and goes away afterwards.
#
# Usage: scripts/container.sh EVAL_ARGUMENT...
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
image=termite-sandbox
network=termite-sandbox
bridge=termite-bridge

command -v docker >/dev/null 2>&1 || {
  printf 'container.sh: needs Docker\n' >&2
  exit 1
}

docker build --quiet --file "$root/sandbox/Dockerfile" --tag "$image" "$root" >/dev/null
# --internal: this network has no route to the outside world.
docker network inspect "$network" >/dev/null 2>&1 \
  || docker network create --internal "$network" >/dev/null

ports=$("$root/scripts/model-ports.sh" | tr '\n' ' ')
forwards=()
for port in $ports; do
  forwards+=("socat TCP-LISTEN:$port,fork,reuseaddr TCP:host.docker.internal:$port &")
done

trap 'docker rm --force "$bridge" >/dev/null 2>&1 || true' EXIT
docker rm --force "$bridge" >/dev/null 2>&1 || true
docker run --detach --rm --name "$bridge" --user 10001 \
  --add-host host.docker.internal:host-gateway \
  --cap-drop ALL --security-opt no-new-privileges --read-only \
  --pids-limit 64 --memory 128m \
  "$image" /bin/sh -c "${forwards[*]} wait" >/dev/null
docker network connect "$network" "$bridge"

mkdir -p "$root/eval/results/team"
docker run --rm --network "$network" \
  --user "$(id -u):$(id -g)" \
  --read-only --tmpfs /tmp --env HOME=/tmp \
  --cap-drop ALL --security-opt no-new-privileges \
  --pids-limit 128 --memory 1g \
  --volume "$root/eval/roster.jsonl:/work/eval/roster.jsonl:ro" \
  --volume "$root/eval/results/team:/work/eval/results/team" \
  --env PORTS="$ports" \
  "$image" /bin/sh -c '
    # The models stay at 127.0.0.1 here too, so termite'"'"'s rule against plain
    # http to another machine stays as it is.
    for port in $PORTS; do
      socat TCP-LISTEN:$port,bind=127.0.0.1,fork,reuseaddr TCP:termite-bridge:$port &
    done
    sleep 0.3
    exec eval "$@"' sh "$@"
