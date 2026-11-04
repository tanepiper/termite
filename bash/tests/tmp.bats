#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  export TMPDIR=$BATS_TEST_TMPDIR/tmp/
  mkdir -p -- "$TMPDIR"
}

leftovers() { find "$BATS_TEST_TMPDIR/tmp" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' '; }

@test "tmp_dir makes a private folder and removes it on success" {
  script=$(make_script work 'tmp_dir work
[[ -d $work ]] && printf "%s\n" "$work"
touch -- "$work/file with spaces" "$work/-dash"
mkdir -- "$work/nested"')
  run --separate-stderr bash "$script"
  [ "$status" -eq 0 ]
  [[ $output == "$BATS_TEST_TMPDIR/tmp/work."* ]]
  [ "$(leftovers)" = 0 ]
}

@test "tmp_dir folders are only readable by the owner" {
  script=$(make_script mode 'tmp_dir work; stat -f %Lp "$work" 2>/dev/null || stat -c %a "$work"')
  run --separate-stderr bash "$script"
  [ "$output" = 700 ]
}

@test "tmp_dir folders are removed after a failure" {
  script=$(make_script failing 'tmp_dir one; tmp_dir two; false')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [ "$(leftovers)" = 0 ]
}

@test "tmp_dir folders are removed after SIGINT" {
  script=$(make_script waits 'tmp_dir work; printf "%s" "$work" >"$1"; while :; do sleep 0.1; done')
  marker=$BATS_TEST_TMPDIR/where
  start_in_background "$script" "$marker"
  wait_for "$marker"
  kill -INT "$pid"
  status=0
  wait "$pid" || status=$?
  [ "$status" -eq 130 ]
  [ ! -e "$(cat "$marker")" ]
}

@test "tmp_dir refuses bad variable names and \$( )" {
  for body in 'tmp_dir "a b"' 'tmp_dir _tmp_x' 'x=$(tmp_dir y)'; do
    script=$(make_script bad "$body")
    run --separate-stderr bash "$script"
    [ "$status" -ne 0 ]
  done
}

@test "clean-up does not follow a folder replaced by a symlink" {
  keep=$BATS_TEST_TMPDIR/keep
  mkdir -p -- "$keep"
  touch -- "$keep/precious"
  script=$(make_script swap 'tmp_dir work; rmdir -- "$work"; ln -s "$1" "$work"')
  run --separate-stderr bash "$script" "$keep"
  [ -e "$keep/precious" ]
}
