# 01 — `termite` crate: query parser, command checks, merging

Status: resolved
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md)

## Details

- Create the `termite` crate at the root of this repo (library now; the `tmite` binary comes in ticket 05). Dependencies: `shell-words` and `thiserror` only, the same ones otty already uses.
- `query.rs`: parse `?`, `??`, `?x` into `Mode` plus the request text. Input that does not start with one of these is not a query.
- `command.rs`: the command checks from the spec (one line, no control characters or escape sequences, parses as shell words, not empty, at most 500 characters). A failed check returns a typed reason; nothing is silently removed.
- `merge.rs`: merge commands that differ only in spacing, quoting or option order, keeping the best description.
- Public types: `Mode`, `Query`, `Candidate { command, description, risk }`, `Risk { Read, Write, Destructive }`, `CommandRejected`.

## Tests first

- AC1: parser table — each prefix with and without a space after it, `?` alone, `?xyz`, plain text, empty input.
- AC2: checks table — `\n`, `\r`, `\x1b[31m`, `\x07`, `\x7f`, unbalanced quote, empty, 501 characters, and a valid command.
- AC3: merging table — `ls -la` / `ls -al`, `ls  -la` / `ls -la`, `cat 'a b'` / `cat "a b"`, and two really different commands that stay separate.

## Comments

**2026-11-04, Claude Code — done.** `src/query.rs`, `src/command.rs`, `src/merge.rs`, `src/candidate.rs`; 6 unit tests and 4 doc tests pass; `cargo fmt --check` and clippy `-D warnings` are clean.

Decisions made while building:

- `?x` is the prefix only when a space or the end follows it, so `?xargs usage` is a `?` query. `?X` is not `?x`.
- "Hidden characters" covers control characters (C0 including tab, DEL, C1 such as `\u{9b}`), zero-width characters and bidirectional overrides, because all of them can make what you see differ from what runs. `\u{85}`, `\u{2028}` and `\u{2029}` count as line breaks.
- Merging is conservative: a word with `$`, backticks, globs, `~`, braces or `!` keeps its quotes in the key (`echo "$HOME"` ≠ `echo '$HOME'`). Pipes and other operators split the command, so options never mix between stages. A word that mixes text and operators (`ls|wc`) only merges when every word matches exactly. Whitespace inside quotes still counts.
- A merged candidate keeps the first command text, the longer description and the **more harmful** risk label, so merging can never lower a risk.
- `Risk` has no serde support yet; ticket 03 adds it together with the model reply parsing.
