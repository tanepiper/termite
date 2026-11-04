#!/usr/bin/env bash
# Record the README GIFs with VHS (brew install vhs).
#
# Uses the real tmite and real local models, in a throwaway demo folder
# with its own TERMITE_HOME, so your own config is never touched.
# Usage: docs/tapes/record.sh [CONFIG_JSON]
#   CONFIG_JSON  a termite config to record with (default: ./demo-config.json)
set -euo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd -- "$here/../.." && pwd)
config=${1:-$here/demo-config.json}
demo=$(mktemp -d "${TMPDIR:-/tmp}/tmite-demo.XXXXXXXX")
trap 'rm -rf -- "$demo"' EXIT

cargo build --release --manifest-path "$repo/Cargo.toml" >/dev/null

mkdir -p "$demo/home" "$demo/project/photos" "$demo/project/build" "$demo/project/notes"
chmod 700 "$demo/home"
cp -- "$config" "$demo/home/config.json"
head -c 3000000 /dev/zero >"$demo/project/photos/holiday.raw"
head -c 800000 /dev/zero >"$demo/project/build/app.o"
printf '# notes\n' >"$demo/project/notes/todo.md"

cat >"$demo/zshrc" <<ZSHRC
export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
PROMPT='%F{208}demo%f %F{244}❯%f '
export PATH="$repo/target/release:\$PATH"
export TERMITE_HOME="$demo/home"
cd "$demo/project"
source "$repo/shell/tmite.zsh"
ZSHRC

for tape in "$here"/*.tape; do
  sed -e "s|DEMO_DIR|$demo|g" -e "s|OUT_DIR|$repo/docs/assets|g" \
    "$tape" >"$demo/$(basename -- "$tape")"
  (cd "$demo" && vhs "$(basename -- "$tape")")
done
