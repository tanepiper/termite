#!/usr/bin/env bats
load test_helper

setup() { setup_home; }

@test "a failing command is reported with its line and status" {
  script=$(make_script fails 'main() {
  ls -- /no/such/file
}
main')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [[ $stderr == *"fails: error: line "*": 'ls -- /no/such/file' failed with status 1"* ]]
}

@test "die prints the message and exits with the status given" {
  script=$(make_script dies 'die "the disk is full" 3')
  run --separate-stderr bash "$script"
  [ "$status" -eq 3 ]
  [ "$stderr" = "dies: error: the disk is full" ]
}

@test "commands in conditions do not count as failures" {
  script=$(make_script quiet-fail 'if ! false; then printf "ok\n"; fi
grep -q x /dev/null || printf "still ok\n"')
  run --separate-stderr bash "$script"
  [ "$status" -eq 0 ]
  [ "$output" = $'ok\nstill ok' ]
  [ -z "$stderr" ]
}

@test "a failing pipeline stage fails the script" {
  script=$(make_script pipe 'false | cat
printf "not reached\n"')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [[ $output != *"not reached"* ]]
}

@test "unset variables are errors" {
  script=$(make_script unset 'printf "%s\n" "$no_such_variable"')
  run bash "$script"
  [ "$status" -ne 0 ]
  [[ $output == *"no_such_variable"* ]]
}

@test "exit handlers run last first, even after a failure" {
  script=$(make_script handlers 'one() { printf "one\n"; }
two() { printf "two\n"; false; }
core_at_exit one
core_at_exit two
die "stop"')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [ "$output" = $'two\none' ]
}

@test "core_at_exit takes only a function" {
  script=$(make_script not-function 'core_at_exit "rm -rf /"')
  run --separate-stderr bash "$script"
  [ "$status" -eq 64 ]
  [[ $stderr == *"is not a function"* ]]
}

@test "new files are private to the owner" {
  script=$(make_script umask 'printf x >"$1"')
  run bash "$script" "$BATS_TEST_TMPDIR/made"
  [ "$status" -eq 0 ]
  [ -z "$(find "$BATS_TEST_TMPDIR/made" -perm -004 -o -perm -040)" ]
}

@test "SIGINT exits 130 and runs exit handlers" {
  script=$(make_script interrupted 'marker=$1
bye() { printf "cleaned\n" >"$marker.done"; }
core_at_exit bye
printf ready >"$1.ready"
while :; do sleep 0.1; done')
  marker=$BATS_TEST_TMPDIR/int
  start_in_background "$script" "$marker"
  wait_for "$marker.ready"
  kill -INT "$pid"
  status=0
  wait "$pid" || status=$?
  [ "$status" -eq 130 ]
  [ "$(cat "$marker.done")" = cleaned ]
}

@test "library files refuse to run directly" {
  for file in "$BASH_DIR"/lib/termite/*.sh; do
    run bash "$file"
    [ "$status" -eq 64 ]
    [[ $output == *"is a library"* ]]
  done
}
