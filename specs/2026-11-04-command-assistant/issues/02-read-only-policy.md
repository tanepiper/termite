# 02 — Read-only policy and its table tests

Status: resolved
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 01
Spec: [../spec.md](../spec.md)

## Details

- `policy.rs`: `fn read_only(command: &str) -> Result<(), PolicyRefusal>`. The refusal carries the one-sentence reason shown in the `?x` fallback ("`rm` deletes files").
- Split the command into pipe stages on unquoted `|`. Turn down every banned shell feature in the spec: `;`, `&&`, `||`, `&`, redirects, `tee`, here-documents, `$(…)`, backticks, `<(…)`, `eval`, `source`, `.`, `exec`, `xargs`, `sudo`, `doas`, `su`, `pkexec`, and leading `NAME=value`.
- Check every stage against the spec's program table, including the turned-down options per program (`find -delete`/`-exec`…, `tail -f`, `docker logs -f`, `git -c`, `kubectl get secret(s)`, `rg --pre`, `fd -x`/`-X`).
- Turn down any argument that matches the secret files list, after expanding a leading `~`.
- The allowlist is plain data in the file (a `match` per program), not configuration.

## Tests first

- AC4: one table with at least one allowed and one turned-down row for every program in the spec, every banned shell feature and every secret path pattern. Include quoting traps: `ls ';rm -rf ~'` is allowed (the `;` is quoted); `ls ;rm` is not.

## Comments

**2026-11-04, Claude Code — done.** `src/policy/` with `mod.rs` (`read_only`, `PolicyRefusal`), `shell.rs` (shell features, pipe split), `programs.rs` (the program table), `secrets.rs` (secret paths) and `tests.rs` (5 table tests, about 190 rows). All 16 tests pass; fmt and clippy `-D warnings` are clean.

Stricter than the approved table (the spec's table is updated to match):

- Options the table missed that write files, run programs or never end: `tree -o`, `file -C`, `date -s` / clock operands, `less -o` / `-O` / `+command`, `bat --pager`, `git --output`, `kubectl get -w`, and any `git` global option except `-C` and `--no-pager`.
- `git branch` takes listing options only (no names), and `git remote` takes `-v` only.
- History expansion (`!`), `$'…'` / `$"…"`, unquoted `(` `)` (subshells, zsh `=(…)`, zsh glob qualifiers such as `*(e:…:)`), and programs given by path are refused.
- Short options are matched inside clusters (`tail -nF`), and long options when shortened the way GNU tools accept (`tail --fol`).

Known limits, all safe to leave because a miss here still needs `/safety` to approve, and a refusal only falls back to `?`:

- **Aliases and shell functions.** If the user has `alias ls='…'`, the shell runs the alias, which the policy cannot see. Added to ticket 05: before a `RunNow`, the zsh widget checks each program with `whence -w` and falls back to `?` unless it is a plain command. Ticket 06 has the same gap for otty's terminal; it is noted there.
- **Recursive search through secret folders.** `grep -r token ~` reads `~/.ssh` and is allowed, because no argument names a secret folder. `rg` skips hidden folders by default, `grep -r` does not.
- **Option values attached to short options** (`grep -f~/.ssh/config`) are not checked against the secrets list; `--file=…` is.
- Package tools (`brew info`, `npm outdated`) contact their registries. They send no local data, and the owner approved them.
