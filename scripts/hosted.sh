#!/usr/bin/env bash
# Run a test on Hugging Face Jobs, under the same gate as a local run. No token
# goes into a test job. `confine` blocks the network and limits writes before
# the servers or the eval tool start, and `eval run` refuses if the gate fails.
# The model and the decision model each get a real llama-server, and reach the
# eval tool over Unix sockets. Nothing is built on or loads this machine.
#
#   scripts/hosted.sh build        build the Linux tools, in a job
#   scripts/hosted.sh gate         check the gate in a job, with no model
#   scripts/hosted.sh run 'NAME'   test a model from eval/hosted-models.jsonl
#   scripts/hosted.sh decision 'NAME'   test one from eval/hosted-decision-models.jsonl
#   scripts/hosted.sh server       copy the pinned GPU llama-server, for ZeroGPU
#   scripts/hosted.sh space        put tools, server and tests in a folder for the Space
#   scripts/hosted.sh fetch        copy finished runs to ~/termite-private/runs
#
# Set HOSTED_GPU=1 to use a T4 for gate and run. CUDA needs /proc writable, and
# a job is held to one hour. It stops if a job could cost more than the monthly
# credit.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$root"
stage=target/hosted
image=ghcr.io/ggml-org/llama.cpp:server
gpu_image=ghcr.io/ggml-org/llama.cpp@sha256:1229bedd0f26390644aeb0552edae1e7b1aee025136eda63d3f7fc098e4f3885
flavor=cpu-upgrade
usd_per_hour=0.03
hours=3
credit_usd=2
writable='--write /tmp/work/eval --write /tmp/run --write /out --write /dev'
layers=0
suffix=hosted
if [[ -n ${HOSTED_GPU:-} ]]; then
  image=ghcr.io/ggml-org/llama.cpp:server-cuda
  flavor=t4-small
  usd_per_hour=0.40
  hours=1
  writable="$writable --write /proc"
  layers=99
  suffix=hosted-t4
fi
results=hf://buckets/tanepiper/jobs-artifacts/termite-results

hf() { uvx --from huggingface_hub==2.4.0 hf "$@"; }

awk -v a="$usd_per_hour" -v b="$hours" -v c="$credit_usd" 'BEGIN { exit !(a * b <= c) }' \
  || { printf 'hosted.sh: could cost more than $%s\n' "$credit_usd" >&2; exit 1; }

export HF_TOKEN
HF_TOKEN=$(sed -n 's/^HUGGINGFACE_API=//p' .env)

build() {
  rm -rf "$stage/source" "$stage/built"
  mkdir -p "$stage/source/sandbox" "$stage/built"
  # The commit, not the working tree, so a run's commit says what it tested.
  git archive HEAD Cargo.toml Cargo.lock src examples shell bash prompts sandbox/confine \
    | tar -x -C "$stage/source"
  output=$(hf jobs run --flavor "$flavor" --timeout 1h \
    -v "$stage/source:/source" -v "$stage/built:/built:rw" rust:1-alpine -- /bin/sh -c '
      set -eu
      apk add --no-cache musl-dev >/dev/null
      cp -R /source /tmp/build
      cd /tmp/build
      cargo build --quiet --example eval
      cargo build --quiet --manifest-path sandbox/confine/Cargo.toml
      cp target/debug/examples/eval sandbox/confine/target/debug/confine /built/' 2>&1 | tee /dev/stderr)
  bucket=$(sed -n "s|^Sync plan: $stage/built -> \(hf://[^ ]*\)$|\1|p" <<<"$output" | head -1)
  [[ -n $bucket ]] || { printf 'hosted.sh: no output folder found\n' >&2; exit 1; }
  hf buckets sync "$bucket" "$stage/built"
  [[ -s $stage/built/eval && -s $stage/built/confine ]] \
    || { printf 'hosted.sh: the build left no tools\n' >&2; exit 1; }
  git rev-parse --short HEAD >"$stage/built/commit"
  printf 'hosted.sh: tools built\n' >&2
}

# Copy llama-server and the CUDA libraries it needs out of the pinned image.
server() {
  rm -rf "$stage/server"
  mkdir -p "$stage/server"
  # shellcheck disable=SC2016  # This text runs inside the job.
  output=$(hf jobs run --flavor cpu-upgrade --timeout 30m -v "$stage/server:/built:rw" "$gpu_image" -- /bin/sh -c '
      set -eu
      ls -la /app | cut -c1-90
      cat /etc/os-release | head -2
      mkdir -p /built/llama
      cp -L /app/llama-server /app/*.so* /built/llama/
      export LD_LIBRARY_PATH=/app:/usr/local/cuda/lib64:/usr/local/cuda/compat:${LD_LIBRARY_PATH:-}
      ldd /app/libggml-cuda.so | tee /built/ldd.txt
      for lib in $(ldd /app/libggml-cuda.so | sed -n "s/.*=> \(.*cuda.*\) (.*/\1/p"); do cp -L "$lib" /built/llama/; done
      du -sh /built/llama' 2>&1 | tee /dev/stderr)
  bucket=$(sed -n "s|^Sync plan: $stage/server -> \(hf://[^ ]*\)$|\1|p" <<<"$output" | head -1)
  [[ -n $bucket ]] || { printf 'hosted.sh: no output folder found\n' >&2; exit 1; }
  hf buckets sync "$bucket" "$stage/server"
  [[ -s $stage/server/llama/llama-server ]] \
    || { printf 'hosted.sh: the job left no server\n' >&2; exit 1; }
}

stage_bundle() {
  [[ -s $stage/built/eval ]] || { printf 'hosted.sh: run build first\n' >&2; exit 1; }
  rm -rf "$bundle"
  mkdir -p "$bundle/eval"
  cp "$stage/built/eval" "$bundle/eval-tool"
  cp "$stage/built/confine" "$bundle/confine"
  cp -RL eval/corpus "$bundle/eval/corpus"
  cp eval/hosted-models.jsonl "$bundle/eval/models.jsonl"
}

# Start a model server and wait for it, inside the job.
serve_function() {
  cat <<SERVE
serve() {
  /app/llama-server --model "\$1" --alias "\$2" --host "\$3" --ctx-size $context --threads \$CPU_CORES --n-gpu-layers $layers >"\$3.log" 2>&1 &
  server=\$!
  tries=0
  until grep -q 'listening on' "\$3.log" 2>/dev/null; do
    tries=\$((tries + 1))
    if [ \$tries -ge 900 ] || ! kill -0 \$server 2>/dev/null; then tail -5 "\$3.log"; exit 1; fi
    sleep 1
  done
}
SERVE
}

# Set up inside the job. Everything after `confine` is held by it.
# shellcheck disable=SC2016  # This text runs inside the job.
setup='set -eu
mkdir -p /tmp/work/src /tmp/work/eval /tmp/work/home /tmp/run
cp -R /bundle/eval/. /tmp/work/eval/
ln -s /out /tmp/work/eval/results
cp /bundle/eval-tool /tmp/eval && cp /bundle/confine /tmp/confine && chmod 755 /tmp/eval /tmp/confine
cd /tmp/work
export TERMITE_HOME=/tmp/work/home LD_LIBRARY_PATH=/app:${LD_LIBRARY_PATH:-}'

confined="/tmp/confine $writable --"

in_job() {
  hf jobs run --flavor "$flavor" --timeout "${hours}h" "$@"
}

# The private Space's files. sandbox/zerogpu/run.py uploads them.
space() {
  folder=$stage/space
  [[ -s $stage/built/commit && -s $stage/server/llama/llama-server ]] \
    || { printf 'hosted.sh: run build and server first\n' >&2; exit 1; }
  rm -rf "$folder"
  mkdir -p "$folder/bundle/eval" "$folder/llama"
  cp sandbox/zerogpu/README.md sandbox/zerogpu/app.py sandbox/zerogpu/requirements.txt "$folder/"
  cp "$stage/built/eval" "$folder/eval-tool"
  cp "$stage/built/confine" "$stage/built/commit" "$folder/"
  cp -RL eval/corpus "$folder/bundle/eval/corpus"
  # The Space has its own CUDA libraries and driver. Keep the names programs load.
  (cd "$stage/server/llama" && cp llama-server libggml-*.so libggml*.so.0 libllama.so.0 \
    libllama-common.so.0 libllama-server-impl.so libmtmd.so.0 "$root/$folder/llama/")
  rm -f "$folder"/llama/libggml.so "$folder"/llama/libggml-base.so
  du -sh "$folder"
}

case ${1:-} in
  build)
    build
    ;;
  server)
    server
    ;;
  space)
    space
    ;;
  gate)
    bundle=$stage/bundle-gate
    stage_bundle
    in_job -v "$bundle:/bundle" -v "$results:/out" "$image" -- /bin/sh -c "$setup
exec $confined /tmp/eval gate red"
    ;;
  run)
    [[ $# -eq 2 ]] || { printf 'usage: hosted.sh run NAME\n' >&2; exit 64; }
    row=$(jq -c --arg name "$2" 'select(.name == $name)' eval/hosted-models.jsonl)
    [[ -n $row ]] || { printf 'hosted.sh: no model called %s\n' "$2" >&2; exit 1; }
    repo=$(jq -r .repo <<<"$row")
    alias=$(jq -r .model <<<"$row")
    socket=$(jq -r .socket <<<"$row")
    context=$(jq -r .context_tokens <<<"$row")
    commit=$(git rev-parse --short HEAD)-$suffix
    # One folder per model, so jobs can run side by side.
    bundle=$stage/bundle-$(shasum <<<"$2" | cut -c1-8)
    stage_bundle
    cat >"$bundle/inner.sh" <<INNER
$(serve_function)
serve /decision/Kev-4B-Q8_0.gguf ggml-org/Kev-4B-GGUF:Q8_0 /tmp/run/decision.sock
serve "\$(ls /model/*.gguf | grep -iv mmproj | grep -i q4_k_m | head -1)" $alias $socket
grep -h "offloaded" /tmp/run/*.log || true
/tmp/eval run --only "\$NAME" --commit $commit
INNER
    config='{"model_url":"http://127.0.0.1:8080","model":"'"$alias"'","decision_url":"http://127.0.0.1:9931","decision_model":"ggml-org/Kev-4B-GGUF:Q8_0","model_socket":"'"$socket"'","decision_socket":"/tmp/run/decision.sock"}'
    printf '%s\n' "$config" >"$bundle/config.json"
    in_job -e "NAME=$2" -v "hf://$repo:/model" -v "hf://ggml-org/Kev-4B-GGUF:/decision" \
      -v "$bundle:/bundle" -v "$results:/out" "$image" -- /bin/sh -c "$setup
cp /bundle/config.json /tmp/work/home/config.json
cp /bundle/inner.sh /tmp/run/inner.sh
exec $confined /bin/sh /tmp/run/inner.sh"
    ;;
  decision)
    [[ $# -eq 2 ]] || { printf 'usage: hosted.sh decision NAME\n' >&2; exit 64; }
    row=$(jq -c --arg name "$2" 'select(.name == $name)' eval/hosted-decision-models.jsonl)
    [[ -n $row ]] || { printf 'hosted.sh: no decision model called %s\n' "$2" >&2; exit 1; }
    repo=$(jq -r .repo <<<"$row")
    file=$(jq -r .file <<<"$row")
    alias=$(jq -r .model <<<"$row")
    socket=$(jq -r .socket <<<"$row")
    context=4096
    hours=1
    commit=$(git rev-parse --short HEAD)-$suffix-decision-$alias
    bundle=$stage/bundle-$(shasum <<<"$2" | cut -c1-8)
    stage_bundle
    printf '%s\n' "$row" >"$bundle/eval/decision-models.jsonl"
    cat >"$bundle/inner.sh" <<INNER
$(serve_function)
serve "/model/$file" $alias $socket
TMPDIR=/tmp/run /tmp/eval decision --commit $commit
INNER
    in_job -v "hf://$repo:/model" -v "$bundle:/bundle" -v "$results:/out" "$image" -- /bin/sh -c "$setup
cp /bundle/inner.sh /tmp/run/inner.sh
exec $confined /bin/sh /tmp/run/inner.sh"
    ;;
  fetch)
    install -d -m 700 "$HOME/termite-private" "$HOME/termite-private/runs"
    hf buckets sync "$results" "$HOME/termite-private/runs"
    chmod 600 "$HOME"/termite-private/runs/*.json
    ;;
  *)
    sed -n '8,14p' "$0" | cut -c3- >&2
    exit 64
    ;;
esac
