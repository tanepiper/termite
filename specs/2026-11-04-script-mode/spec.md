# Script Mode: `?b` Writes a Bash Script

Created: 2026-11-04
Author: me@tane.dev
Agent: Claude Code
Status: READY
Approved: Yes
Worktree: No
Type: Build

## Summary

**Goal:** `?b back up my notes folder to the NAS, keeping 7 days` writes a
complete, well-made bash script instead of a one-line command. The script
has help text, `--verbose`, proper error handling and reporting, and uses
a shared, tested library of bash components. It is installed into
termite's own folder, which is on your `PATH`, so afterwards you run it by
name like any other command.

**Minimum safety requirement (owner):** installed scripts can be read and
run only by you, no other user.

**Shape:**

```text
$TERMITE_HOME                 default ~/.config/termite, mode 0700
├── config.json               already exists
├── bin/                      installed scripts, on PATH, 0700; scripts 0700
├── lib/termite/              shared bash components, 0700; files 0600
│   ├── VERSION
│   ├── core.sh  log.sh  args.sh  require.sh  tmp.sh  confirm.sh
│   └── README.md
└── drafts/                   scripts waiting for review, 0700; files 0600
```

The `lib/termite` components ship inside the `tmite` binary and are
installed from it. Their source, docs and tests live in this repo under
`bash/`.

## How it works

1. `?b <request>` asks the chat model for one script, as JSON:
   `{ "name": "notes-backup", "summary": "…", "script": "…" }`. The system
   prompt is the bash style guide below.
2. termite checks the script (see **Script checks**). A failed check is
   sent back to the model once with the error; if it fails again, `?b`
   shows the errors and saves nothing.
3. The decision model rates it (`harmful`, `no_secrets`). These are shown
   as notes; they never approve anything, because nothing runs.
4. The script is saved as a **draft** in `drafts/notes-backup` (0600, not
   executable, not on `PATH`).
5. You review it: the zsh widget opens it in `$PAGER` (default
   `less`), then asks `Install as notes-backup? [y/N]`. From the command
   line, `tmite script show notes-backup` and
   `tmite script install notes-backup`.
6. Install copies the draft to `bin/notes-backup` with mode 0700 and puts
   `notes-backup --help` at your prompt.

`?b` never runs the script. Running it is always something you type.

## Design decisions

1. **Draft, review, then install.** A script is code from a model. It is
   only made executable after you have seen all of it and said yes, the
   same rule as Enter for `?`. The draft stage also means a half-written
   or rejected script is never on `PATH`.
2. **Owner-only by construction, not by `chmod` afterwards.** Every folder
   is created 0700 and every file is created with its final mode in the
   same `open` call (`O_CREAT | O_EXCL | O_NOFOLLOW`, mode 0700 or 0600),
   whatever the umask is. Writes go to a temporary name in the same folder,
   are synced, then renamed into place, so a script is never seen half
   written. There is no moment where the file is readable by others.
3. **Check the folders before every write and every run of `?b`.** Each
   folder from `$TERMITE_HOME` down must be a real folder (not a symlink),
   owned by you, and not writable by group or others. If not, termite
   refuses and says which folder and what is wrong. It does not silently
   `chmod` your files. `tmite setup` offers to fix the existing
   `~/.config/termite` (now 0755) once, after asking.
4. **Append to `PATH`, never prepend.** `bin/` goes at the end of `PATH`,
   so a script can never take the place of a system command, even one
   installed later.
5. **Names cannot shadow anything.** A script name is 2–40 characters of
   `a-z`, `0-9` and `-`, starting with a letter. It is turned down if it is
   a shell keyword or builtin, an alias or function in your shell, or a
   program already on `PATH`. Replacing one of your own termite scripts
   asks first, and keeps the old one in `drafts/<name>.previous`.
6. **Target bash 5.3 (owner).** The library and generated scripts may use
   bash 5 features (associative arrays, `mapfile`, `${x,,}`, `${x@Q}`,
   `wait -n`, `EPOCHSECONDS`, `local -n`). macOS still ships bash 3.2 as
   `/bin/bash`, so:
   - Scripts use `#!/usr/bin/env bash`, which finds Homebrew's bash 5.3 in
     a normal terminal.
   - The first lines of every script are a version guard, written so that
     bash 3.2 can run it. On an older bash it stops with
     `notes-backup needs bash 5.3 or later; this is 3.2.57 (/bin/bash).
     Install it with: brew install bash`.
   - cron and launchd use `PATH=/usr/bin:/bin`, so they find bash 3.2 and
     hit the guard. Scheduling is out of scope; when it comes, it needs a
     `PATH` or an absolute bash path.
   - `?b` and `tmite setup` check that bash 5.3 or later is on `PATH`, and
     setup offers `brew install bash` (Linux: the package manager) when it
     isn't.
7. **One shared library, sourced, with its source checked.** Each script
   starts with a short preamble that finds `lib/termite/core.sh` from
   `$TERMITE_HOME` (default `~/.config/termite`), checks that the file is
   owned by you and not writable by others, checks the library `VERSION`,
   then sources it. If any check fails, the script stops with a one-line
   message saying how to fix it (`tmite setup`). This makes the shared
   components possible without trusting a file someone else could edit.
8. **The library is versioned and only grows.** Scripts declare the
   library version they were written for. New versions may add functions
   but never change what an existing one does, so installed scripts keep
   working after `tmite` updates the library.
9. **Its own model setting.** Scripts need a stronger model than one-liners.
   The config gains optional `script_model` (defaults to `model`) and
   `script_timeout_secs` (default 300). Reasoning stays off.
10. **The style guide is a file, not a string in Rust.**
    `prompts/bash-script.md` is the system prompt, included at build time,
    so it can be reviewed and improved like code. Its examples are taken
    from the library's tested examples.

## No `?bd` (owner)

`?bd` is not added. Owner-only scripts do not affect worktrees: a worktree
is just a folder, and an installed script runs as you from any folder with
mode 0700. Mode 0700 only stops other users (another login,
`sudo -u someone`, a container running as a different user). Nothing
termite makes is ever group- or world-writable.

## Shell profile

`tmite setup` gains one step. After showing exactly what it will add, and
only after you type `y`, it adds this block to the profile for your shell
(`~/.zshrc` for zsh; `~/.bashrc`, plus a line in `~/.bash_profile` that
sources it, for bash, since macOS bash login shells read only
`~/.bash_profile`):

```sh
# >>> termite >>>
export TERMITE_HOME="${TERMITE_HOME:-$HOME/.config/termite}"
case ":$PATH:" in
  *":$TERMITE_HOME/bin:"*) ;;
  *) export PATH="$PATH:$TERMITE_HOME/bin" ;;
esac
# <<< termite <<<
```

The block is the same for bash and zsh. It does nothing if `bin` is
already on `PATH`, and setup replaces an existing block rather than adding
a second. The old file is kept as `<profile>.termite-backup`. The zsh
widget line (`source …/shell/tmite.zsh`) goes inside the same block when
you ask for the widget.

## The shared library (`bash/lib/termite/`)

Each component is small, documented at the top of its file and in
`README.md`, and tested. Every function name starts with its component, so
nothing clashes with the script's own names.

| File | Gives |
|---|---|
| `core.sh` | strict mode (`set -Eeuo pipefail`, `IFS=$'\n\t'`, `umask 077`); an `ERR` trap that reports the failing command, line and exit status; `die "message" [status]`; sources the others |
| `log.sh` | `log_info`, `log_verbose` (only with `--verbose`), `log_debug` (`--verbose` twice), `log_warn`, `log_error`. All go to stderr, so stdout stays clean for data. Colour only on a terminal and only when `NO_COLOR` is unset. `--quiet` keeps only warnings and errors |
| `args.sh` | the standard options (`-h/--help`, `-v/--verbose`, `-q/--quiet`, `--version`, `--`), combined short options, `--name=value`, a clear error and status 64 for an unknown option |
| `require.sh` | `require_cmd docker rsync` (says which is missing and how it's usually installed), `require_bash 5 3`, `require_os macos|linux` |
| `tmp.sh` | `tmp_dir`: a private `mktemp -d` folder removed by an `EXIT` trap, even after an error or Ctrl C |
| `confirm.sh` | `confirm "Delete 3 old backups?"`: asks on the terminal and answers no without one; `--yes` skips it. Scripts must use it before anything destructive |

The library's own rules, enforced by its tests:

- No `eval`, no backticks, no `source` of anything but the library itself,
  no unquoted expansions (shellcheck clean with no disabled checks unless
  the line says why).
- `printf` instead of `echo` for anything that is not a fixed string.
- `--` before user-supplied arguments to commands that take options.
- Temporary files only through `tmp.sh`. No predictable `/tmp` names.
- Exit statuses: 0 success, 1 failure, 64 usage, 130 interrupted.

**Tests** (in `bash/tests/`, run by `cargo test` through a test that skips
with a message when the tools are missing):

- `shellcheck` on every library file and every example.
- `bats-core` tests for every function: normal use, bad input, file names
  with spaces, new lines, leading dashes and globs; errors reported with the
  right line; traps cleaning up after failure and after `SIGINT`.
- The tests run on bash 5.3 or later. One more test runs a script under
  `/bin/bash` 3.2 and checks that the version guard stops it with the
  install hint, before anything else runs.

## The bash style guide (system prompt)

`prompts/bash-script.md` tells the model to write scripts that:

- Start with the standard preamble and `main "$@"` at the end; put all
  logic in functions.
- Have `usage()` with a name, one-line summary, usage line, every option,
  at least two examples, and exit statuses. `--help` prints it and exits
  0.
- Report through `log_*`: short output by default, every step with
  `--verbose`, nothing but warnings and errors with `--quiet`.
- Check what they need up front (`require_cmd`, arguments, that paths
  exist) and fail early with a message that says what to do.
- Quote every expansion; use arrays for argument lists; `--` before user
  input; `printf '%s'` for data; no `eval`; no parsing `ls`; `find -print0`
  with `while IFS= read -r -d ''` for file lists.
- Confirm before deleting or overwriting, support `--dry-run` when they
  change things, and never use `sudo` unless the request asks for it.
- Use bash 5.3 features where they make the script clearer, and handle
  the macOS/Linux differences in the outside tools they call (`sed -i`,
  `stat`, `date`, `readlink -f`), using `require_os` when one platform is
  enough.

It includes one complete, tested example script (from the library's
examples), plus short right/wrong pairs for the common traps.

## Script checks (before a draft is saved)

| Check | Turned down when |
|---|---|
| name | not a valid, unshadowing name (decision 5) |
| size | over 64 KB, or empty |
| text | any control character other than tab and new line, or invalid UTF-8 |
| shape | missing the preamble, `usage`, `main "$@"`, or `--help`/`--verbose` handling |
| syntax | `bash -n` fails (run with the bash 5.3 found on `PATH`) |
| lint | shellcheck reports a warning or error. shellcheck is required: without it `?b` stops and says how to install it |
| banned | `eval`, backticks, `source`/`.` of anything but the library, `curl`/`wget` piped to a shell, writes to shell profiles, `chmod` making anything group- or world-writable |

The checks read the script; they never run it. `bash -n` and shellcheck
only parse.

## Modes and output

- `?b <request>`: draft, review, install. `tmite --zle` prints
  `script` + the draft path + the decision notes; the widget opens the pager
  and asks.
- `tmite script list | show <name> | install <name> | remove <name>`.
  `remove` moves the script to `drafts/<name>.removed`; it does not delete
  it.
- `--json` gives the name, paths, check results and decision notes.

## Acceptance Criteria

- [ ] AC1: `?b` is parsed as its own mode; `?bash` and `?bd` are not
      (`?bd` is a `?` query for "d …", like `?xargs`) (table test).
- [ ] AC2: Folders and files are created with exactly 0700 / 0600 under
      umask 000 and 022; a symlinked, foreign-owned, or group/other-writable
      folder is refused with a message naming it (tests in a temp home).
- [ ] AC3: Writes are atomic: a failed write leaves no partial file and no
      temporary file behind (test).
- [ ] AC4: Names that are keywords, builtins, or programs on `PATH`, or
      malformed, are turned down (table test).
- [ ] AC5: Every check in **Script checks** turns down a script that breaks
      only that check (table test).
- [ ] AC6: Each library function has bats tests that pass on bash 5.3;
      shellcheck is clean; under `/bin/bash` 3.2 a script stops at the
      version guard with the install hint.
- [ ] AC7: A script whose library file is foreign-owned, writable by
      others, or the wrong version stops before sourcing it, with the fix in
      the message (bats test).
- [ ] AC8: The profile block is added once, replaced not duplicated, and
      works in both zsh and bash (test against temp profiles).
- [ ] AC9: Nothing is installed without a `y`, and `?b` never runs the
      script (test with a mocked model).
- [ ] AC10: Manual: three real `?b` requests against Qwen3.8-27B produce
      scripts that pass every check and do what was asked, written down in
      the ticket.

## Out of scope

- Running scripts, scheduling them (cron/launchd), or editing them with the
  model (`?b edit notes-backup` is a natural next step).
- Sharing scripts between users or machines.
- Shells other than bash for the scripts themselves (the profile step
  supports zsh and bash).
- `?bd` (owner: not wanted).

## Owner Answers

Round 1, 2026-11-04:

1. **`?bd`:** not added.
2. **Where scripts live:** `~/.config/termite`, moved with `TERMITE_HOME`.
3. **Review flow:** always a draft, reviewed in the pager, installed on `y`.
4. **shellcheck:** required for `?b`.
5. **bats-core:** approved as a dev tool for the library tests.
6. **Bash version:** target bash 5.3, with a version guard that bash 3.2
   can run (decision 6).

## Tickets

| # | Ticket | Model | Blocked by |
|---|---|---|---|
| 01 | `TERMITE_HOME` layout, folder checks, atomic owner-only writes, name rules | Opus | — |
| 02 | Shared bash library with docs, bats and shellcheck tests | Opus | — |
| 03 | `?b` generation: mode, style-guide prompt, schema, script checks, decision notes, drafts | Opus | 01, 02 |
| 04 | `tmite script` commands, `?b` output formats and zsh widget review | Sonnet | 03 |
| 05 | `tmite setup`: profile block for zsh and bash, library install, fixing folder modes | Sonnet | 01, 02 |

01 and 02 touch different files and can be built in parallel.

## Round Log

- Draft (2026-11-04): written from the owner's request (`?b` scripts,
  owner-only permissions, maybe `?bd`, an authoritative bash style prompt,
  `--verbose` and help in every script, a tested shared library,
  `TERMITE_HOME` and `PATH` in the shell profile).
- Round 1 (2026-11-04, owner): no `?bd`; `~/.config/termite`; always
  review a draft; shellcheck required; bats-core approved; target bash 5.3.
  Spec approved; tickets 01–05 written.
- 2026-11-04: the intents spec (`specs/2026-11-04-intents-and-confirm-card`)
  supersedes the `?b` prefix (now `tmite script`) and the `tmite script`
  management commands (now `tmite scripts`). Tickets 03–05 are blocked
  until they are rebased on it.
