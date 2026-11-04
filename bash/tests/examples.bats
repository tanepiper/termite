#!/usr/bin/env bats
load test_helper

setup() {
  setup_home
  examples=$BASH_DIR/examples
  tree=$BATS_TEST_TMPDIR/tree
  mkdir -p -- "$tree/small" "$tree/big" "$tree/with space" "$tree/-dash"
  head -c 300000 /dev/zero >"$tree/big/file"
  head -c 2000 /dev/zero >"$tree/small/file"
}

@test "every example starts with the exact preamble" {
  lines=$(wc -l <"$BASH_DIR/preamble.sh")
  for example in "$examples"/*; do
    diff <(head -n "$lines" -- "$example") "$BASH_DIR/preamble.sh"
  done
}

@test "every example has help with usage, options and examples" {
  for example in "$examples"/*; do
    run --separate-stderr bash "$example" --help
    [ "$status" -eq 0 ]
    [[ $output == *"Usage: "* && $output == *"Options:"* && $output == *"Examples:"* ]]
  done
}

@test "disk-report lists the biggest folders first, with odd names" {
  run --separate-stderr bash "$examples/disk-report" --top 2 "$tree"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 2 ]
  [[ ${lines[0]} == *"/big" ]]
}

@test "disk-report refuses a bad --top" {
  for top in 0 -1 abc '1;ls' 'a[$(id)]'; do
    run --separate-stderr bash "$examples/disk-report" --top "$top" "$tree"
    [ "$status" -eq 64 ]
  done
}

@test "prune-backups --dry-run deletes nothing" {
  touch -t 202001010000 -- "$tree/old.tar" "$tree/-old" "$tree/old file"
  touch -- "$tree/new.tar"
  run --separate-stderr bash "$examples/prune-backups" --dry-run "$tree"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 3 ]
  [ -e "$tree/old.tar" ] && [ -e "$tree/-old" ] && [ -e "$tree/old file" ]
}

@test "prune-backups asks, and without a terminal deletes nothing" {
  touch -t 202001010000 -- "$tree/old.tar"
  run --separate-stderr no_tty bash "$examples/prune-backups" "$tree" </dev/null
  [[ $stderr == *"no terminal to ask"* ]]
  [ -e "$tree/old.tar" ]
}

@test "prune-backups --yes deletes only old matching files" {
  touch -t 202001010000 -- "$tree/old.tar" "$tree/old.txt" "$tree/-old.tar"
  touch -- "$tree/new.tar"
  run --separate-stderr bash "$examples/prune-backups" --yes --pattern '*.tar' "$tree"
  [ "$status" -eq 0 ]
  [ ! -e "$tree/old.tar" ] && [ ! -e "$tree/-old.tar" ]
  [ -e "$tree/old.txt" ] && [ -e "$tree/new.tar" ] && [ -d "$tree/big" ]
}

@test "prune-backups refuses / and patterns with a slash" {
  run --separate-stderr bash "$examples/prune-backups" --yes /
  [ "$status" -eq 1 ]
  run --separate-stderr bash "$examples/prune-backups" --pattern '../*' "$tree"
  [ "$status" -eq 64 ]
}
