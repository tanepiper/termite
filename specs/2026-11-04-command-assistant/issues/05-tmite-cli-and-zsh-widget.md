# 05 — `tmite` CLI and the zsh widget

Status: resolved
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: 04
Spec: [../spec.md](../spec.md)

## Details

- Binary target `tmite` in the `termite` crate: `tmite '<query>'` prints a readable result; `--json` prints `Answer` as JSON for scripts, and `--zle` prints the line format below for the zsh widget. Exit codes: 0 answer, 2 no usable command or refused, 1 error.
- The context comes from the environment: OS, `$SHELL`, current folder; `is_local` and `is_idle` are true.
- `shell/tmite.zsh`: a ZLE widget bound to Enter. When the buffer starts with `?`, it calls `tmite --zle` instead of running the line:
  - `Confirm`: set `BUFFER` to the command and show the description with `zle -M`; the next Enter runs it.
  - `Choose`: show a numbered list; the chosen number puts that command in `BUFFER`.
  - `RunNow`: first check every program in the command with `whence -w`; if any is an alias, function or builtin other than the one named, behave like `ConfirmWithReason` ("`ls` is an alias here"). Otherwise set `BUFFER`, show the reason, then `zle accept-line`.
  - `ConfirmWithReason` / `Refused` / errors: show the reason; `Refused` leaves the buffer empty.
- Any other line is passed to the original `accept-line` untouched.
- The widget reads `tmite --zle` output instead of JSON: line 1 is the variant, line 2 is the command, line 3 is the message. For `Choose`, it prints one `command<TAB>description` line per item. Because the command checks guarantee one printable line, plain `read` is enough. Descriptions and messages have tabs, new lines and control characters replaced with spaces before printing. No `jq`, and the widget never `eval`s anything.

## Tests first

- `--json` and `--zle` output for each `Answer` variant (unit tests).
- AC8, manual, written down in this ticket under `## Comments`: each mode at a zsh prompt, a normal command, a line starting with `?` inside quotes, and Ctrl C while waiting.

## Comments

**2026-11-04, Claude Code — done.** `src/bin/tmite/main.rs` (arguments, context, runtime), `src/bin/tmite/output.rs` (human, `--json`, `--zle`, exit codes), `shell/tmite.zsh`. 8 binary tests pass, and all 51 tests in the crate pass; fmt and clippy `-D warnings` are clean.

- `tmite` only prints. It never runs a command itself; the zsh widget runs it with the shell's own `accept-line`. Usage errors exit 64.
- New direct dependency: `tokio` with the `rt` feature only, for the runtime `reqwest` needs. It was already in the tree through `reqwest`.
- Library additions: `termite::to_printable` turns model-written text into one printable line, used for every description, reason and message. `Ranked::new` is now public, so front ends can build answers in their own tests.
- `--zle` format, as built: `confirm` / `run` / `confirm_reason` / `refused` print the command on line 2 and a ready-to-show message on line 3 (`Not run automatically: …`, `Refused: …`, `Ran without asking: …`). `choose` prints the count on line 2, then `command<TAB>details` lines. `no_command` / `failed` leave line 2 empty.
- The widget keeps the `accept-line` that was there before (`zle -A`), once, so other plugins that wrap Enter keep working and sourcing the file twice does not loop. Source it after those plugins.
- `??` shows up to 9 commands; a number picks one, any other key cancels and leaves the query in the prompt.

**End to end (2026-11-04)**, against a stub server playing `llama-server` and systemone, with a throwaway `HOME`:

- `?` returns the top command; `??` lists the merged commands, best first. `?x list my code` reports safe; `?x restart docker` falls back with "the model labelled it `write`". A multi-line command gives "No usable command" with the bad command escaped, and exit 2.
- The bearer token goes to systemone only, never to the model. A missing config file or a server that is down gives a one-line error, with exit 1.

**AC8, zsh widget**, driven in a real interactive zsh through `zpty` with a stand-in `tmite`:

- Pass: `?` fills the prompt and only the second Enter runs it. `?x` runs at once. `?x` with `alias uname=echo` puts the command at the prompt without running it. `??` then `2` picks the second command, and another key cancels. Refused empties the prompt. Failed keeps the query. Lines not starting with `?` run as normal. Leading spaces before `?` work. Sourcing twice does not loop. A missing `tmite` shows the shell's error.
- The `zpty` harness was flaky: keys sent while the shell was still busy were sometimes lost. Each scenario above passed in at least one clean run. Not checked: Ctrl C while waiting for `tmite`, and running with a real model.
