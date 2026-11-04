#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  script=$(make_script ask 'args_parse "$@"
[[ -z ${ANSWER_FILE-} ]] || _confirm_input=$ANSWER_FILE
if confirm "Delete $1?"; then printf yes; else printf no; fi')
}

@test "without a terminal the answer is no, with a warning" {
  ANSWER_FILE=/nonexistent/tty run --separate-stderr bash "$script" things
  [ "$output" = no ]
  [[ $stderr == *"no terminal to ask"* ]]
}

@test "with no controlling terminal at all, the answer is no" {
  run --separate-stderr no_tty bash "$script" things </dev/null
  [ "$output" = no ]
}

@test "--yes answers yes without asking" {
  ANSWER_FILE=/nonexistent/tty run --separate-stderr bash "$script" things --yes
  [ "$output" = yes ]
  [ -z "$stderr" ]
}

@test "y and yes are yes, in any case; anything else is no" {
  for answer in y Y yes YES; do
    printf '%s\n' "$answer" >"$BATS_TEST_TMPDIR/answer"
    ANSWER_FILE=$BATS_TEST_TMPDIR/answer run --separate-stderr bash "$script" things
    [ "$output" = yes ]
  done
  for answer in n no "" yess " y" "y es"; do
    printf '%s\n' "$answer" >"$BATS_TEST_TMPDIR/answer"
    ANSWER_FILE=$BATS_TEST_TMPDIR/answer run --separate-stderr bash "$script" things
    [ "$output" = no ]
  done
}

@test "the question is shown with control characters replaced" {
  printf 'n\n' >"$BATS_TEST_TMPDIR/answer"
  ANSWER_FILE=$BATS_TEST_TMPDIR/answer run --separate-stderr bash "$script" $'x\e[2J'
  # bats trims the trailing space after the prompt.
  [ "$stderr" = 'Delete x?[2J? [y/N]' ]
}
