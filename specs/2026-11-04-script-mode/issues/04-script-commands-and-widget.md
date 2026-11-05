# 04 — `tmite script` commands, `?b` output and zsh widget review

Status: done as `tmite script` and `tmite scripts`; no `?b` in the widgets
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: 03
Spec: [../spec.md](../spec.md) (Modes and output)

> **Superseded in part (2026-11-04):** the intents spec
> (`specs/2026-11-04-intents-and-confirm-card`) replaces `?b` with
> `tmite script` and the management commands with `tmite scripts …`, and
> adds the input and output guards and the provenance log. Rebase this
> ticket on intents tickets 01, 03, 04 and 09 before starting it.

## Details

- `tmite script list` (installed and drafts, with summaries), `show <name>` (the draft, or the installed script), `install <name>`, `remove <name>` (moves it to `drafts/<name>.removed`).
- `install`: `Home::check()`; the name check again (something with that name may have been installed since); if `bin/<name>` exists, ask, then keep the old one as `drafts/<name>.previous`; copy with `write_private(bin, name, script, 0o700)`. The prompt is read from the terminal; without one, install refuses (no default yes).
- Output for `?b`: human; `--json`; `--zle` lines `script`, the draft path, the name, then the notes.
- `shell/tmite.zsh`: for `script`, check the name against aliases and functions (`whence -w`); open the draft in `${PAGER:-less}`; ask `Install as <name>? [y/N]` with `read -q`; on `y` run `tmite script install <name>` and put `<name> --help` in the buffer.

## Tests first

- `--json` and `--zle` output for `Script` and `ScriptRejected`.
- `install` with no terminal refuses; with an existing script asks and keeps `.previous`; the installed file is 0700 (temp home).
- AC9 and the widget, manual, written down here: `?b` at a zsh prompt, reviewing, `n` (nothing installed), `y` (installed, `--help` in the buffer).
- AC10, manual: three real `?b` requests against Qwen3.8-27B, written down here with how each did.
