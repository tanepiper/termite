# 13 — The classifier: six levels, contained and sensitive paths

Status: built. `tmite check` keeps its three ratings, and the widgets' questions stay until ticket 07
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (section 2, Round 3)

## Details

- `src/classify/` (library): `Level { Read, SensitiveRead, Change, ContainedDestructive, SystemDangerous, Never }`, ordered so the larger is the stricter. `classify(command, &Context) -> Classification { level, reasons }`, plain code, no model.
- Per stage of a pipe or chain; the highest level wins. Unknown programs start at `Change`. Doubt (parse failure, glob, brace expansion, `$VAR`, substitution, decode) raises one level, at least to `Change`.
- Paths: expand `~`, `$HOME` and braces; resolve against the current folder; match sensitive paths ignoring case. Project root: the git work tree, else the current folder when it is not `~` or `/`.
- Contained: every delete target inside the project and not its root. Otherwise a delete is `SystemDangerous`.
- `Never`: exfiltration shapes (a sensitive read piped or passed to a network tool), download or decode into a shell, reverse shells, fork bombs, security switches (`csrutil`, `spctl --master-disable`), keychain dumps, termite's own files.
- The models' labels and the decision verdict map onto levels and can only raise one (`max`).
- Gate `?` and `??` now, before the card (ticket 07) exists: in the zsh widget, levels 0 and 2 insert; 1 and 3 ask with `read -q` first; 4 shows the command as text with `zle -M` and inserts nothing; 5 shows only the reason. `--json` carries `level` and `insert`.
- Agent and CI detection (from ticket 06's list) makes levels 1, 3, 4 and 5 exit non-zero with no command.
- The current read-only policy becomes the level 0 test inside the classifier.

## Tests first

- AC13: a table of every example in spec section 2, plus the review's bypasses: `cat ~/{.ssh,x}/id_rsa`, `cat ~/.SSH/id_rsa`, `cat .ENV`, `kubectl get --raw …/secrets`, `git diff --ext-diff`, `grep -r token ~`, `rm -rf node_modules` (3) against `rm -rf ~/node_modules` from the project (4), `rm -rf .` (4).
- Property: `max(level, model_level) == classify` result; no input crashes; adding a pipe stage never lowers the level.
- AC14: each agent and CI variable, and no terminal, against each level.

## Decisions made while building

- Level 1 asks and level 2 doesn't, so a plain `max` would lose the question. A sensitive read that also changes something, or that has doubt, goes to level 3.
- Doubt stops at level 4. Only a hostile shape reaches level 5.
- A hazard signature sets a floor, so a shape the parser misses still counts. Critical hazards floor at 4, because the signatures over-match (`crontab -l`). Level 5 comes from the exact shapes in the ticket.
- `gate` runs after `answer`, in `tmite`. The evaluation calls `answer`, so it still scores what a model wrote.
- A quote, backtick or bracket that never closes is level 4.
