#!/usr/bin/env bash
# Run the policy on each build and compare the results. No model is involved,
# and nothing that the policy reads is run. Linux builds compile with network,
# for their crates, and then run with no network and a read-only corpus.
#
# Usage: scripts/agree.sh [--no-x86]
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$root"
commit=$(git rev-parse --short HEAD)
[[ -z $(git status --porcelain -- src examples Cargo.lock) ]] || commit+=-dirty
corpus=$(cd eval/corpus && pwd -P)

cargo build --example eval
target/debug/examples/eval agree macos "$commit"

# name, platform, image, packages to add
builds=(
  "linux-musl-arm64 linux/arm64 rust:1-alpine musl-dev"
  "linux-glibc-arm64 linux/arm64 rust:1-bookworm -"
)
if [[ ${1-} != --no-x86 ]]; then
  builds+=(
    "linux-musl-x86 linux/amd64 rust:1-alpine musl-dev"
    "linux-glibc-x86 linux/amd64 rust:1-bookworm -"
  )
fi

for build in "${builds[@]}"; do
  read -r name platform image packages <<<"$build"
  target=$root/target/agree/$name
  mkdir -p "$target"
  install=:
  [[ $packages == - ]] || install="apk add --no-cache $packages"

  printf 'agree.sh: building %s\n' "$name" >&2
  docker run --rm --platform "$platform" \
    -v "$root:/src:ro" -v "$target:/target" -e CARGO_TARGET_DIR=/target \
    -w /src "$image" \
    sh -c "$install && cargo build --locked --example eval"

  printf 'agree.sh: running %s\n' "$name" >&2
  docker run --rm --platform "$platform" --network none \
    -v "$target/debug/examples/eval:/bin/eval:ro" \
    -v "$corpus:/work/eval/corpus:ro" \
    -v "$root/eval/downloads:/work/eval/downloads:ro" \
    -v "$root/eval/results:/work/eval/results" \
    -w /work "$image" /bin/eval agree "$name" "$commit"
done

target/debug/examples/eval agree --compare
