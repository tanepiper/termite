#!/usr/bin/env bash
# Run a command in the sandbox: loopback network only, writes only to the
# build, results and temporary folders, secrets unreadable, and a throwaway
# HOME. Used for the tests and the model evaluation. It holds nothing hostile:
# spec levels 8 to 10, which need real attacks, belong in the container.
#
# Usage: scripts/sandbox.sh [--termite-config|--models] COMMAND [ARGUMENT...]
#        scripts/sandbox.sh --check     prove every rule holds
#
# --models is for anything that talks to a model: of everything on this
# machine, it can only reach the model ports and no local sockets.
# --termite-config does the same, and lets the command read your termite
# config, for the evaluation, which needs to know where the decision model is.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
real_home=$(cd -- "$HOME" && pwd -P)
cargo_home=${CARGO_HOME:-$real_home/.cargo}
rustup_home=${RUSTUP_HOME:-$real_home/.rustup}

command -v sandbox-exec >/dev/null 2>&1 || {
  printf 'sandbox.sh: needs sandbox-exec (macOS)\n' >&2
  exit 1
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/termite-sandbox.XXXXXX")
scratch=$(cd -- "$scratch" && pwd -P)
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p -- "$root/target" "$root/eval/results"

profile=$root/scripts/sandbox.sb

# The same rules, with this machine narrowed down to the model ports. Tests
# can't use this: they start their own fake servers on any free port.
models_only() {
  local port
  profile=$scratch/models.sb
  {
    printf '(version 1)\n(import "%s")\n' "$root/scripts/sandbox.sb"
    printf '(deny network-outbound (remote ip "localhost:*"))\n'
    printf '(deny network-outbound (remote unix-socket))\n'
    while read -r port; do
      printf '(allow network-outbound (remote ip "localhost:%s"))\n' "$port"
    done < <("$root/scripts/model-ports.sh")
  } >"$profile"
}

run() {
  sandbox-exec -f "$profile" \
    -D "TARGET=$root/target" -D "RESULTS=$root/eval/results" \
    -D "SCRATCH=$scratch" -D "CARGO=$cargo_home" \
    -D "REAL_HOME=$real_home" -D "ENV_FILE=$root/.env" \
    /usr/bin/env HOME="$scratch" CARGO_HOME="$cargo_home" \
    RUSTUP_HOME="$rustup_home" CARGO_NET_OFFLINE=true \
    SEMGREP_SEND_METRICS=off ${termite_home:+TERMITE_HOME="$termite_home"} \
    "$@"
}

# Each of these must fail inside the sandbox. Output is thrown away, so a
# broken rule shows up as a failed check and never as a secret on screen.
check() {
  local failed=0 name
  blocked() {
    name=$1
    shift
    if run "$@" >/dev/null 2>&1; then
      printf 'NOT BLOCKED: %s\n' "$name" >&2
      failed=1
    else
      printf 'blocked: %s\n' "$name"
    fi
  }
  allowed() {
    name=$1
    shift
    if run "$@" >/dev/null 2>&1; then
      printf 'allowed: %s\n' "$name"
    else
      printf 'WRONGLY BLOCKED: %s\n' "$name" >&2
      failed=1
    fi
  }

  blocked 'reaching the internet' /usr/bin/nc -z -G 3 1.1.1.1 443
  blocked 'listing ~/.ssh' /bin/ls "$real_home/.ssh"
  blocked 'reading the .env file' /bin/cat "$root/.env"
  blocked 'writing to the real home' /usr/bin/touch "$real_home/.termite-sandbox-check"
  blocked 'writing to the source' /usr/bin/touch "$root/src/.termite-sandbox-check"
  blocked 'writing to ~/.zshrc' /bin/sh -c "echo x >> '$real_home/.zshrc'"
  allowed 'writing to the build folder' /usr/bin/touch "$root/target/.sandbox-check"
  allowed 'reading the source' /bin/cat "$root/Cargo.toml"
  rm -f -- "$root/target/.sandbox-check"

  # A local service that isn't a model, like a password manager or Docker.
  local port=18765 listener
  /usr/bin/nc -l 127.0.0.1 "$port" >/dev/null 2>&1 &
  listener=$!
  sleep 0.2
  allowed 'tests reaching a local port' /usr/bin/nc -z -G 3 127.0.0.1 "$port"
  /usr/bin/nc -l 127.0.0.1 "$port" >/dev/null 2>&1 &
  listener="$listener $!"
  sleep 0.2
  models_only
  blocked 'model tests reaching another local port' /usr/bin/nc -z -G 3 127.0.0.1 "$port"
  # shellcheck disable=SC2086
  kill $listener 2>/dev/null || true

  # A real socket, so a block can't just mean there was nothing there.
  local socket=$scratch/check.sock
  /usr/bin/nc -lU "$socket" >/dev/null 2>&1 &
  listener=$!
  sleep 0.2
  blocked 'model tests reaching a local socket' /usr/bin/nc -U "$socket" </dev/null
  profile=$root/scripts/sandbox.sb
  allowed 'tests reaching a local socket' /usr/bin/nc -U "$socket" </dev/null
  kill "$listener" 2>/dev/null || true
  return "$failed"
}

termite_home=
case ${1:-} in
  --check)
    check
    exit
    ;;
  --termite-config)
    termite_home=${TERMITE_HOME:-$real_home/.config/termite}
    models_only
    shift
    ;;
  --models)
    models_only
    shift
    ;;
  '')
    printf 'usage: sandbox.sh [--termite-config] COMMAND [ARGUMENT...]\n' >&2
    exit 64
    ;;
esac

run "$@"
