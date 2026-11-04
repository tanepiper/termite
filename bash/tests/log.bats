#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  script=$(make_script talk 'args_parse "$@"
log_info "info"
log_verbose "verbose"
log_debug "debug"
log_warn "warn"
log_error "error"
printf "data\n"')
}

@test "messages go to stderr and data to stdout" {
  run --separate-stderr bash "$script"
  [ "$status" -eq 0 ]
  [ "$output" = data ]
  [ "$stderr" = $'info\ntalk: warning: warn\ntalk: error: error' ]
}

@test "--verbose adds verbose messages, -vv adds debug" {
  run --separate-stderr bash "$script" --verbose
  [ "$stderr" = $'info\nverbose\ntalk: warning: warn\ntalk: error: error' ]
  run --separate-stderr bash "$script" -vv
  [ "$stderr" = $'info\nverbose\ndebug: debug\ntalk: warning: warn\ntalk: error: error' ]
}

@test "--quiet keeps only warnings and errors" {
  run --separate-stderr bash "$script" -q
  [ "$stderr" = $'talk: warning: warn\ntalk: error: error' ]
}

@test "control characters in messages are replaced" {
  noisy=$(make_script noisy 'log_info "$1"
log_printable "$1"')
  run --separate-stderr bash "$noisy" $'file\e]0;owned\a\rname'
  [ "$stderr" = 'file?]0;owned??name' ]
  [ "$output" = 'file?]0;owned??name' ]
}

@test "messages keep spaces, globs and leading dashes as given" {
  odd=$(make_script odd 'log_info "$@"')
  run --separate-stderr bash "$odd" '-n' 'a  b' '*'
  [ "$stderr" = '-n a  b *' ]
}

@test "no colour without a terminal" {
  unset NO_COLOR
  run --separate-stderr bash "$script"
  [[ $stderr != *$'\e['* ]]
}
