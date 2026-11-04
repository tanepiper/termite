#!/usr/bin/env bats
load test_helper

setup() { setup_home; }

@test "require_cmd passes for installed commands" {
  script=$(make_script needs 'require_cmd ls cat; printf ok')
  run bash "$script"
  [ "$status" -eq 0 ]
  [ "$output" = ok ]
}

@test "require_cmd names every missing command with an install hint" {
  script=$(make_script needs 'require_cmd ls no-such-tool-a rg-is-fine-maybe no-such-tool-b')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [[ $stderr == *"needs no-such-tool-a"*"no-such-tool-b, not found on PATH."* ]]
  [[ $stderr == *"Install"* ]]
}

@test "require_cmd maps commands to package names" {
  script=$(make_script needs 'PATH=/nowhere require_cmd rg')
  run --separate-stderr bash "$script"
  [[ $stderr == *ripgrep* ]]
}

@test "require_bash compares major and minor versions" {
  script=$(make_script bash-version 'require_bash 5 3; require_bash 4; require_bash 99 0')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [[ $stderr == *"needs bash 99.0 or later"* ]]
}

@test "require_bash rejects a bad version" {
  script=$(make_script bash-version 'require_bash five')
  run bash "$script"
  [ "$status" -eq 64 ]
}

@test "require_os accepts this system and refuses others" {
  script=$(make_script os 'require_os macos linux; printf ok; require_os plan9')
  run --separate-stderr bash "$script"
  [ "$status" -eq 1 ]
  [ "$output" = ok ]
  [[ $stderr == *"runs only on plan9"* ]]
}
