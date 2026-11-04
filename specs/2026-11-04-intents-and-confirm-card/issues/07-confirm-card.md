# 07 — The confirm card: three treatments, alternate screen, `--json` rules

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 04
Spec: [../spec.md](../spec.md) (section 2)

## Details

- `src/card.rs` (library): `Treatment { Insert, DisplayOnly, Withheld }` decided from flags, output flags and the verdict, in code only.
- `src/bin/tmite/card.rs`: draws on `/dev/tty` with plain ANSI, on the alternate screen, raw mode via `libc::termios`, restoring the terminal on every exit path (including Ctrl C and panics). Keys per the spec. Everything printed goes through `to_printable`.
- Insert: hand the command to the shell. The plugin reads it from a private temporary file (0600, removed after reading) or from a pipe, and sets `BUFFER`; the command is never run by the card. Copy: `pbcopy` / `wl-copy` / `xclip`, for Insert only.
- Display only: no insert, no copy, and the command is cleared with the alternate screen. Withheld: only the description of the effect.
- `--json`: withheld commands are left out with a reason; display-only ones carry `"insert": false`.

## Tests first

- AC6 on the treatment function: a table of flags, output flags and verdicts → treatment.
- Rendering tests on a fake terminal (an in-memory writer, fixed width): each treatment, long commands wrapping, escape sequences in a description shown as `?`.
- Manual check, written down here: each treatment at a real zsh prompt with the plugin; Ctrl C restores the terminal.
