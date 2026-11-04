# The termite bash library

Small, tested pieces that every script written by `?b` uses, so each
script gets the same help, options, output, error handling and clean-up
without copying them.

The library is installed at `$TERMITE_HOME/lib/termite` (default
`~/.config/termite/lib/termite`). It is built into `tmite` and installed
or updated by `tmite setup`. Its source and tests live in the termite repo
under `bash/`.

## Using it

Every script starts with the termite preamble (`bash/preamble.sh`). The
preamble:

1. stops with an install hint on bash older than 5.3, before running
   anything else (macOS `/bin/bash` is 3.2);
2. checks that `TERMITE_HOME`, `lib`, `lib/termite` and every library file
   are real files and folders, owned by you and not writable by anyone
   else, and that `VERSION` is new enough;
3. sources `core.sh`, which sources the rest.

A script then looks like this (see `bash/examples/` for complete ones):

```bash
#!/usr/bin/env bash
# --- termite preamble 1 … (exact text from bash/preamble.sh) ---

usage() {
  cat <<'HELP'
notes-backup: copy the notes folder to the NAS.

Usage: notes-backup [options]
…
HELP
}

main() {
  args_version 1.0.0
  args_flag dry-run n
  args_value keep k 7
  args_parse "$@"
  require_cmd rsync
  log_verbose "copying …"
  …
}

main "$@"
```

## Components

Each file documents its own functions at the top.

| File | Gives |
|---|---|
| `core.sh` | strict mode, `umask 077`, error reports with line and command, `die`, `core_at_exit`, exit 130/143 on Ctrl C / TERM |
| `log.sh` | `log_info`, `log_verbose`, `log_debug`, `log_warn`, `log_error` (all on stderr), `log_printable` |
| `args.sh` | `-h -v -q -y --version --`, `args_flag`, `args_value`, `args_on`, `args_version`, `args_parse`, `args_error` |
| `require.sh` | `require_cmd`, `require_bash`, `require_os` |
| `tmp.sh` | `tmp_dir VARIABLE`: a private folder removed on exit |
| `confirm.sh` | `confirm QUESTION`: no without a terminal, yes with `--yes` |

## Exit statuses

| Status | Meaning |
|---|---|
| 0 | done |
| 1 | failed (`die`, a failing command, a missing requirement) |
| 64 | bad usage (`args_error`, unknown option) |
| 70 | a script bug the library caught (no `usage` function) |
| 130 / 143 | interrupted (Ctrl C / TERM) |

## Rules the library keeps, and scripts should too

- Quote every expansion. Use arrays for lists of arguments.
- Put `--` before user-supplied arguments to commands that take options.
- Check a value with a regex before using it in `(( ))` or `$(( ))`:
  arithmetic on untrusted text can run commands (`a[$(id)]`).
- No `eval`, no backticks, no `source` of anything but the library.
- `printf` for anything that is not a fixed string; `log_printable` for
  file names and other untrusted text shown to a person.
- Temporary files only through `tmp_dir`.
- Ask with `confirm` before deleting or overwriting; offer `--dry-run`.
- Messages on stderr, data on stdout.

## Versions

`VERSION` is a whole number. A new version may add functions but never
changes what an existing one does, so scripts written for version 1 keep
working on version 2. The preamble refuses an older library than the one
the script was written for.

## Tests

`bash/tests/run.sh` runs shellcheck on every file, then the bats tests
(`brew install bats-core shellcheck`). `cargo test` runs it too, and skips
it with a message when either tool is missing.
