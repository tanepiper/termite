# 02 — Shared bash library with docs, bats and shellcheck tests

Status: resolved
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (The shared library, decisions 6–8)

## Details

- `bash/lib/termite/`: `VERSION` (`1`), `core.sh`, `log.sh`, `args.sh`, `require.sh`, `tmp.sh`, `confirm.sh`, `README.md`. The behaviour of each is in the spec's table. Each file starts with a comment block: what it gives, each function with its arguments, output stream and exit status, and an example.
- Target bash 5.3. Every file is meant to be sourced, never run: each starts with a guard that returns if it was already sourced, and fails if it is run directly.
- `bash/preamble.sh`: the exact lines every generated script starts with. It must run under bash 3.2 up to the version check: no bash 4+ syntax before it. It then finds `lib/termite/core.sh` from `${TERMITE_HOME:-$HOME/.config/termite}`, checks that the file and its folder are owned by the user and not group/other-writable (`stat` differs: `-f` on macOS, `-c` on Linux), checks `VERSION`, and sources it. Each failure prints one line with the fix (`run: tmite setup`) and exits 1.
- `bash/examples/`: two complete example scripts written to the style guide (one read-only, one that changes files with `--dry-run` and `confirm`). The prompt in ticket 03 quotes them.
- `bash/tests/*.bats` and `bash/tests/run.sh`: shellcheck on every `.sh` and example (no disabled checks without a reason on the line), then bats.
- `tests/bash_library.rs`: a cargo integration test that runs `bash/tests/run.sh`. It prints `skipped: install bats-core and shellcheck` and passes when either tool is missing, so `cargo test` still works on a bare machine.
- bats-core is approved (owner, Round 1): `brew install bats-core`.

## Tests first

- AC6: for every function: normal use; bad or missing arguments; file names with spaces, new lines, a leading `-` and glob characters; `log_*` going to stderr only, colour only on a terminal and only without `NO_COLOR`; `--quiet` and `--verbose`; the `ERR` trap reporting the right command, line and status; `tmp_dir` removed after success, after an error and after `SIGINT` (status 130); `confirm` answering no without a terminal and yes with `--yes`.
- AC7: the preamble stops before sourcing when the library is missing, is a symlink, is writable by others, is in a folder writable by others, or has a different `VERSION`; each with its own message.
- The preamble under `/bin/bash` (3.2 on macOS) stops with the install hint and runs nothing else; the test is skipped where `/bin/bash` is already 5.3.


## Comments

**2026-11-04, Claude Code — done.** `bash/preamble.sh`, `bash/lib/termite/` (six components, `VERSION`, `README.md`), `bash/examples/disk-report` and `prune-backups`, `bash/tests/` (63 bats tests, `run.sh`), `tests/bash_library.rs`. shellcheck is clean on every file with `--external-sources`; bats-core 1.14 installed with Homebrew (owner approved).

Changes from the ticket:

- The preamble checks permissions with `find -perm` instead of `stat`, which avoids the macOS/Linux `stat` difference, and with `[[ -O ]]` for ownership.
- `args_version` and `args_on` were added. shellcheck cannot see a `SCRIPT_VERSION` variable used inside the library, and reads `ARGS[dry-run]` in `(( ))` as a subtraction.
- Option names are checked before they are used as array keys, so `-z`, `--=x` or `--a]b` give a usage error (64), not a bash error.
- Clean-up handlers take a function name only (`core_at_exit fn`), so there is no `eval` anywhere.
- Found while testing: bash starts background jobs with SIGINT ignored unless job control is on, so the Ctrl C tests start scripts with `set -m`. They run under a new session (`perl POSIX::setsid`) to check "no terminal" behaviour, because `/dev/tty` was openable in the test shell.
- macOS `chmod` does not accept `--`; the tests pass absolute paths instead.
