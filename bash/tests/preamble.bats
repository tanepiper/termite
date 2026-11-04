#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  script=$(make_script loads 'printf loaded')
}

@test "a script loads the library from TERMITE_HOME" {
  run --separate-stderr bash "$script"
  [ "$status" -eq 0 ]
  [ "$output" = loaded ]
}

@test "the library is found under HOME when TERMITE_HOME is unset" {
  mkdir -p -- "$BATS_TEST_TMPDIR/user/.config"
  mv -- "$TERMITE_HOME" "$BATS_TEST_TMPDIR/user/.config/termite"
  HOME=$BATS_TEST_TMPDIR/user TERMITE_HOME= run bash "$script"
  [ "$output" = loaded ]
}

refused() {
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [ -z "$output" ]
  [[ $stderr == *"$1"*"Fix: run tmite setup" ]]
}

@test "a missing library stops the script" {
  rm -- "$TERMITE_HOME/lib/termite/core.sh"
  refused "core.sh is missing"
}

@test "a library file writable by others stops the script" {
  chmod o+w "$TERMITE_HOME/lib/termite/args.sh"
  refused "args.sh is writable by other users"
}

@test "a library folder writable by the group stops the script" {
  chmod g+w "$TERMITE_HOME/lib"
  refused "lib is writable by other users"
}

@test "a home folder writable by others stops the script" {
  chmod o+w "$TERMITE_HOME"
  refused "home is writable by other users"
}

@test "a symlinked library file stops the script" {
  mv -- "$TERMITE_HOME/lib/termite/log.sh" "$BATS_TEST_TMPDIR/log.sh"
  ln -s -- "$BATS_TEST_TMPDIR/log.sh" "$TERMITE_HOME/lib/termite/log.sh"
  refused "log.sh is a symlink"
}

@test "an older or unreadable library version stops the script" {
  printf '0\n' >"$TERMITE_HOME/lib/termite/VERSION"
  refused "library is version 0"
  printf 'one\n' >"$TERMITE_HOME/lib/termite/VERSION"
  refused "library is version one"
}

@test "a relative TERMITE_HOME stops the script" {
  TERMITE_HOME=relative run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [[ $stderr == *"must be an absolute path"* ]]
}

@test "bash older than 5.3 stops at the version guard" {
  old=/bin/bash
  if "$old" -c '((BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 3)))'; then
    skip "/bin/bash is already 5.3 or later"
  fi
  run --separate-stderr "$old" "$script"
  [ "$status" -eq 1 ]
  [ -z "$output" ]
  [[ $stderr == "loads needs bash 5.3 or later; this is "* ]]
  [[ $stderr == *"Fix: "* ]]
}

@test "sh stops at the version guard too" {
  run --separate-stderr sh "$script"
  [ "$status" -eq 1 ]
  [[ $stderr == *"needs bash 5.3 or later"* ]]
}
