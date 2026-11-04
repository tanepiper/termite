# 01 — Intents in the CLI: `ask`, `choose`, `run`, `script`, `scripts`

Status: ready-for-agent
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (section 1)

## Details

- `tmite ask|choose|run|script <request…>` and `tmite scripts list|show|install|remove`; `tmite setup` unchanged. The request is every word after the intent (or after `--`), joined with spaces.
- `Mode` becomes `Intent { Ask, Choose, Run, Script }` in the library; `answer` takes an intent and a request instead of a `Query`. `parse_query` and the `?` prefixes leave the library (the plugin, ticket 02, maps them).
- `--json` and `--zle` stay; `--zle` is replaced by the card in ticket 07, so keep its output as is until then.
- Usage text lists the intents; an unknown intent is a usage error (64) that names the valid ones.
- Old `tmite '? …'` calls print a one-line hint ("use `tmite ask …`, or the zsh plugin for `?`") and exit 64.

## Tests first

- AC1: a table of argument lists → intent and request, including `--`, an empty request, an unknown intent and the old `?` form.
- The existing answer tests, moved from `Mode` to `Intent`.
