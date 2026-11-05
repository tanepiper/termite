#!/usr/bin/env bash
# The damage test bed: throwaway machines where a model's commands may run.
# See specs/2026-11-05-damage-test-bed/spec.md. Nothing here runs a command on
# this Mac: QEMU is started with fixed arguments, and commands only exist as
# text sent to a machine with no network card and nothing shared.
#
# Usage: scripts/testbed.sh fetch                 download the pinned image
#        scripts/testbed.sh run SCENARIO.json     one recorded run
#        scripts/testbed.sh refuse-nic            prove a network card stops a run
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
home=$HOME/termite-private/testbed
images=$home/images
version=20261026-2643
name=debian-12-genericcloud-arm64-$version.qcow2
url=https://cloud.debian.org/images/cloud/bookworm/$version/$name
sha512=f42866fe22b4baf9f0ed4f81d620064f2a4b86b18b92bfe3aaaa40babf6819d39cbf0caaaad13f4ae3e7c9f82cbd78b0c8cabadd8bc737ce13578e61fa86970c
base=$images/$name
firmware=/opt/homebrew/share/qemu/edk2-aarch64-code.fd
harness=$root/testbed/harness.py
run=
pids=()

say() { printf 'testbed: %s\n' "$*"; }
die() { printf 'testbed: %s\n' "$*" >&2; exit 1; }

cleanup() {
  local pid
  for pid in ${pids[@]+"${pids[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
  [[ -z $run ]] || destroy_disks
}
trap cleanup EXIT

verify() {
  [[ -f $base ]] || die "no image: run scripts/testbed.sh fetch"
  local found
  found=$(shasum -a 512 "$base" | cut -d' ' -f1)
  [[ $found == "$sha512" ]] || die "the image does not match its pinned hash"
  chmod 444 "$base"
  say "image matches its pinned hash"
}

fetch() {
  mkdir -p -- "$images"
  chmod 700 "$home"
  if [[ ! -f $base ]]; then
    say "this downloads $name (about 346 MB) from cloud.debian.org"
    read -r -p 'Download it? [y/N] ' answer
    [[ $answer == y ]] || die "not downloaded"
    curl -fL --output "$base.part" "$url"
    mv -- "$base.part" "$base"
  fi
  verify
}

seed() {
  local staging=${run:?}/seed
  mkdir -p -- "$staging"
  cp -- "$root"/testbed/guest/* "$staging/"
  hdiutil makehybrid -quiet -o "${run:?}/seed.iso" "$staging" -iso -joliet \
    -iso-volume-name cidata -joliet-volume-name cidata -default-volume-name cidata
}

# QEMU itself is confined too: no IP network, and no reads or writes under
# your home folder outside the test bed.
profile() {
  cat <<EOF
(version 1)
(allow default)
(deny network* (remote ip))
(deny network* (local ip))
(deny file-read* (subpath "$HOME"))
(allow file-read* (subpath "$home"))
(deny file-write*)
(allow file-write* (subpath "$home") (subpath "/dev") (subpath "/private/var/folders"))
EOF
}

# Start a machine in FOLDER from DISK, with no network card and nothing shared.
# Any further arguments go after the fixed ones.
machine() {
  local folder=$1 disk=$2
  shift 2
  mkdir -p -- "$folder"
  sandbox-exec -p "$(profile)" qemu-system-aarch64 \
    -machine virt -accel hvf -cpu host -m 1024 -smp 2 \
    -bios "$firmware" -nodefaults -no-user-config -display none -nic none \
    -monitor none -serial "file:$folder/console.log" \
    -qmp "unix:$folder/qmp.sock,server=on,wait=off" \
    -device virtio-serial-pci \
    -chardev "socket,id=control,path=$folder/control.sock,server=on,wait=off" \
    -device virtserialport,chardev=control,name=termite.control \
    -drive "file=$disk,if=none,id=root,format=qcow2" \
    -device virtio-blk-pci,drive=root,bootindex=0 \
    "$@" &
  pids+=("$!")
}

wait_for_machines() {
  local pid
  for pid in ${pids[@]+"${pids[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  pids=()
}

overlay() {
  qemu-img create -q -f qcow2 -F qcow2 -b "$1" "$2" 4G
}

new_run() {
  run=$home/runs/$(date -u +%Y%m%dT%H%M%SZ)-$1
  mkdir -p -- "$run"
  chmod 700 "$run"
  seed
  seed_disk=(-drive "file=$run/seed.iso,if=none,id=seed,format=raw,readonly=on"
    -device "virtio-blk-pci,drive=seed")
}

# The disks go after every run. The recording stays.
destroy_disks() {
  rm -f -- "${run:?}"/*.qcow2 "${run:?}/seed.iso"
  rm -rf -- "${run:?}/seed"
}

run_scenario() {
  local scenario=$1
  [[ -f $scenario ]] || die "no scenario at $scenario"
  verify
  new_run "$(basename "$scenario" .json)"
  say "recording to $run"

  overlay "$base" "$run/disk.qcow2"
  machine "$run" "$run/disk.qcow2" "${seed_disk[@]}"
  python3 "$harness" record "$run" --scenario "$scenario" || die "the run stopped"
  wait_for_machines

  overlay "$base" "$run/inspector.qcow2"
  machine "$run/inspector" "$run/inspector.qcow2" "${seed_disk[@]}" \
    -device pcie-root-port,id=slot1,chassis=1
  python3 "$harness" inspect "$run" --used "$run/disk.qcow2" || die "inspection failed"
  wait_for_machines

  overlay "$run/disk.qcow2" "$run/reboot.qcow2"
  machine "$run/reboot" "$run/reboot.qcow2"
  python3 "$harness" boots "$run" --seconds 150
  wait_for_machines

  python3 "$harness" report "$run"
  say "disks destroyed; the recording is in $run"
}

# The harness must refuse a machine with a network card. The card here reaches
# nothing: restrict=on keeps it off every network.
refuse_nic() {
  verify
  new_run refuse-nic
  overlay "$base" "$run/disk.qcow2"
  machine "$run" "$run/disk.qcow2" "${seed_disk[@]}" \
    -netdev user,id=net0,restrict=on -device virtio-net-pci,netdev=net0
  if python3 "$harness" record "$run" --scenario "$root/testbed/scenarios/control-wipe.json"; then
    die "FAILED: a machine with a network card was accepted"
  fi
  wait_for_machines
  say "a machine with a network card was refused, as it should be"
}

case ${1:-} in
  fetch) fetch ;;
  run) run_scenario "${2:?scenario}" ;;
  refuse-nic) refuse_nic ;;
  *) die "usage: testbed.sh fetch|run SCENARIO.json|refuse-nic" ;;
esac
