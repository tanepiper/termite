# shellcheck shell=bash
# Shared set-up for the termite library tests.
#
# Each test gets its own TERMITE_HOME with a private copy of the library,
# and make_script writes a script made of the real preamble plus a body.

bats_require_minimum_version 1.5.0

BASH_DIR=$(cd -- "$BATS_TEST_DIRNAME/.." && pwd)

setup_home() {
  export TERMITE_HOME=$BATS_TEST_TMPDIR/home
  mkdir -p -- "$TERMITE_HOME/lib/termite"
  cp -- "$BASH_DIR"/lib/termite/* "$TERMITE_HOME/lib/termite/"
  chmod 700 "$TERMITE_HOME" "$TERMITE_HOME/lib" "$TERMITE_HOME/lib/termite"
  chmod 600 "$TERMITE_HOME"/lib/termite/*
  export NO_COLOR=1
}

# make_script NAME BODY: write $BATS_TEST_TMPDIR/NAME and print its path.
make_script() {
  local path=$BATS_TEST_TMPDIR/$1
  { cat -- "$BASH_DIR/preamble.sh"; printf '%s\n' "$2"; } >"$path"
  chmod 700 "$path"
  printf '%s' "$path"
}

# no_tty COMMAND...: run COMMAND in a new session, with no controlling
# terminal, the way cron or a CI job runs it.
no_tty() {
  perl -MPOSIX -e 'POSIX::setsid() or die "setsid: $!"; exec @ARGV or die "exec: $!"' -- "$@"
}

# start_in_background SCRIPT ARGS...: start a script that can receive
# SIGINT (non-interactive shells start background jobs with SIGINT
# ignored, unless job control is on). Sets $pid.
start_in_background() {
  set -m
  bash "$@" &
  # shellcheck disable=SC2034 # read by the calling test
  pid=$!
  set +m
}

# wait_for FILE: wait up to 5 s for FILE to exist and not be empty.
wait_for() {
  local _
  for _ in $(seq 50); do
    [ -s "$1" ] && return 0
    sleep 0.1
  done
  return 1
}
