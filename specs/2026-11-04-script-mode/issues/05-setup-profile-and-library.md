# 05 — `tmite setup`: profile block, library install, folder modes, bash 5.3

Status: done
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: 01, 02
Spec: [../spec.md](../spec.md) (Shell profile, decision 3, decision 6)

> **Superseded in part (2026-11-04):** the intents spec
> (`specs/2026-11-04-intents-and-confirm-card`) replaces `?b` with
> `tmite script` and the management commands with `tmite scripts …`, and
> adds the input and output guards and the provenance log. Rebase this
> ticket on intents tickets 01, 03, 04 and 09 before starting it.

## Details

- **Folder modes:** if `Home::check()` fails because a termite folder is too open (today `~/.config/termite` is 0755), show which and offer to set 0700, on `y` only. Foreign-owned or symlinked folders are reported, never changed.
- **Library:** install or update `lib/termite/` from the copy built into `tmite` (`include_str!` of `bash/lib/termite/*`), files 0600, with `write_private`. Only replace it when the built-in `VERSION` is the same or newer.
- **bash 5.3 and shellcheck:** check the bash on `PATH` (`bash --version`) and `shellcheck`; offer `brew install bash` / `brew install shellcheck` (or say which package on Linux), on `y` only.
- **Profile:** pick the profile from `$SHELL`: `~/.zshrc` for zsh; `~/.bashrc`, plus a line in `~/.bash_profile` that sources it, for bash. Show the block from the spec; on `y`, replace an existing `# >>> termite >>>` block or append one, keeping the old file as `<profile>.termite-backup`. Write through a temporary file and rename, keeping the profile's existing mode. Offer to add the widget's `source` line inside the block.
- A profile that is a symlink (dotfile managers) is edited at its target, after saying so.

## Tests first

- AC8: against temp profiles: appended once; run twice gives one block; an old block is replaced; the rest of the file is unchanged byte for byte; the backup holds the old file; a symlinked profile edits the target.
- The block, sourced in zsh and in bash with `TERMITE_HOME` set and unset, gives the right `PATH` and does not add `bin` twice.
- Folder-mode fix: 0755 becomes 0700 only after `y`; nothing changes without a terminal.
