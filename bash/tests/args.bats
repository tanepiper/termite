#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  script=$(make_script opts 'usage() { printf "usage: opts [options]\n"; }
args_version 2.1.0
args_flag dry-run n
args_value days d 7
args_parse "$@"
printf "dry=%s days=%s\n" "${ARGS[dry-run]}" "${ARGS[days]}"
printf "<%s>\n" "${ARGS_REST[@]}"
if args_on dry-run; then printf "dry run\n"; fi')
}

@test "defaults apply when nothing is given" {
  run --separate-stderr bash "$script"
  [ "$status" -eq 0 ]
  [ "$output" = $'dry=0 days=7\n<>' ]
}

@test "long, short, joined and = forms all set values" {
  for form in "--days 3" "--days=3" "-d 3" "-d3"; do
    # shellcheck disable=SC2086 # the form is meant to split into words
    run --separate-stderr bash "$script" $form
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "dry=0 days=3" ]
  done
}

@test "short flags combine and count" {
  run --separate-stderr bash "$script" -nnd 5
  [ "${lines[0]}" = "dry=2 days=5" ]
  [ "${lines[-1]}" = "dry run" ]
}

@test "arguments keep their order, spaces, globs and new lines" {
  run --separate-stderr bash "$script" one 'two words' --dry-run '*' $'new\nline'
  [ "$status" -eq 0 ]
  [ "$output" = $'dry=1 days=7\n<one>\n<two words>\n<*>\n<new\nline>\ndry run' ]
}

@test "after -- everything is an argument" {
  run --separate-stderr bash "$script" -- --dry-run -x
  [ "$output" = $'dry=0 days=7\n<--dry-run>\n<-x>' ]
}

@test "a value may start with a dash" {
  run --separate-stderr bash "$script" --days -1
  [ "${lines[0]}" = "dry=0 days=-1" ]
}

@test "usage errors exit 64 and point to --help" {
  for bad in --nope -z -é "--days" "--dry-run=yes" "-d" "--=x" "--a]b" "--x[1]=2"; do
    run --separate-stderr bash "$script" $bad
    [ "$status" -eq 64 ]
    [[ ${stderr_lines[-1]} == "Try 'opts --help'." ]]
  done
}

@test "--help prints usage and exits 0 without running the rest" {
  run --separate-stderr bash "$script" --days 3 --help
  [ "$status" -eq 0 ]
  [ "$output" = "usage: opts [options]" ]
}

@test "--version prints the name and version" {
  run --separate-stderr bash "$script" --version
  [ "$output" = "opts 2.1.0" ]
}

@test "option names are checked when declared" {
  for bad in "args_flag Bad" "args_flag help" "args_value x xx" "args_flag a v" "args_flag 'a;b'"; do
    broken=$(make_script broken "$bad")
    run --separate-stderr bash "$broken"
    [ "$status" -eq 64 ]
  done
}

@test "option values are never run as code" {
  run --separate-stderr bash "$script" --days '$(touch '"$BATS_TEST_TMPDIR"'/ran)' -- '`touch x`'
  [ "$status" -eq 0 ]
  [ ! -e "$BATS_TEST_TMPDIR/ran" ]
}
