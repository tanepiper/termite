# 02 — zsh plugin with a configurable trigger

Status: ready-for-agent
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: 01
Spec: [../spec.md](../spec.md) (section 1)

## Details

- `plugins/tmite/tmite.plugin.zsh` (oh-my-zsh layout; a plain `source` works too), replacing `shell/tmite.zsh`.
- `TMITE_TRIGGER` (default `?`) and `TMITE_INTENTS` (default `'' ask '?' choose 'x' run 'b' script`). A line is a request only when it starts with the trigger, then an intent's letters (longest match first), then whitespace, then text.
- Calls `tmite <intent> -- "$request"`; never `eval`s; any other line goes to the original `accept-line` untouched.
- Until ticket 07 lands, it reads the existing `--zle` output; afterwards the card handles the interaction and the plugin only passes the buffer.

## Tests first

- AC2 in `zsh -f` with `zpty`: default trigger; a custom trigger (`,` and `tm`); custom letters; `?bash`, `?xargs`, `? ` with no text, a quoted `?` inside a command; a request containing `$(…)`, backticks and `;` reaches `tmite` as plain text and runs nothing.
