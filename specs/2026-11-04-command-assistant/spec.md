# Command Assistant: Ask a Local Model for Shell Commands

Created: 2026-11-04
Author: me@tane.dev
Agent: Claude Code
Status: READY
Approved: Yes
Worktree: No
Type: Build

## Summary

**Goal:** Type a plain request such as `? restart docker` or
`?x show what is in my code folder` and get back a shell command. A local
model writes the command. A separate decision model rates the possible
commands and picks the best one. Depending on the mode, you confirm the
command, choose from a list, or let it run straight away.

**Shape:** `termite`, a standalone Rust repo and library (this repository),
owns all the logic. Two thin front ends use it:

- `tmite`, termite's command line tool, with a zsh widget, so the feature works at
  the prompt of any terminal.
- `otty-assist`, the otty integration: an `assist` widget in a fork of the
  otty app, opened from the command palette, with a card for confirming or
  choosing a command. It depends on `termite`; otty's rule that its crates
  start with `otty-` is why the core is not in the otty workspace.

**Pieces outside otty:**

- **Local model:** `llama-server` from llama.cpp, using its OpenAI-compatible
  `/v1/chat/completions` endpoint with a JSON schema response format, so the
  reply is always valid JSON.
- **Decision endpoint ("systemone"):** `POST /v1/systemone` on llama.cpp
  (and Ollama 0.40 or later), served with a decision model such as Kev-4B.
  It ranks the possible commands and answers the safety questions for `?x`.
  The `/rank` and `/safety` formats from Rounds 1–2 were replaced by it in
  Round 3.

### Current state (research, 2026-11-04, otty `main`)

| Fact | Location |
|---|---|
| Palette entries end in a `PaletteTarget`, and the app turns each one into an event another widget already handles | `app/src/widgets/palette/types.rs`, `app/src/events/palette.rs` |
| Widgets follow one pattern: `event.rs` (intents and effects), `reducer.rs`, `state.rs`, `view/` | `app/src/widgets/*` |
| Async work runs through `Task::perform`; iced is built with the `tokio` feature | `app/src/widgets/quick_launch/reducer.rs`, `app/Cargo.toml` |
| Bytes reach a terminal's PTY through `otty_ui_term::Event::Write { id, data }`, which is routed to `engine.write()` | `crates/ui/terminal/src/term.rs`, `app/src/widgets/terminal_workspace/reducer.rs` |
| Paste into prompt writes the clipboard as raw bytes, with no bracketed paste | `crates/ui/terminal/src/view.rs` (`BlockCommand::PasteClipboard`) |
| The active terminal's current folder can be read from its blocks | `app/src/widgets/terminal_workspace/services.rs` (`terminal_cwd_from_blocks`) |
| The planned Rerun action writes a command and Enter only when the shell is at an idle prompt | `specs/2026-10-31-command-blocks`, ticket 06 |
| Shell hooks exist only for local zsh and bash sessions; SSH sessions have none | `app/src/shell_integration.rs` |
| The otty workspace has no HTTP client yet | `Cargo.lock` |

## Modes

The first word of the query picks the mode. The rest of the query is the
request.

| Prefix | You get | Before it runs |
|---|---|---|
| `?` | One command: the one the decision model ranks highest | You confirm it (Enter runs it) or edit it first |
| `??` | All the different commands, best first, each with a short description; the best is marked | You pick one, then confirm it as with `?` |
| `?x` | One command, as with `?` | Runs without asking **only** if the safety check passes. Otherwise it falls back to `?` and says why |

For all three modes the local model is asked for up to five possible
commands in one call. The modes differ only in what happens after ranking.

### `?` — one answer, you confirm

1. Generate the possible commands.
2. Clean and check each one (see "Command checks"). Drop any that fail.
3. Merge the ones that are really the same command.
4. Ask the decision endpoint to rank what is left.
5. Show the top command, its description and its risk label. **Enter** runs
   it. **Edit** puts it at the prompt without running it. **Esc** cancels.

### `??` — several answers, you choose

Steps 1–4 as for `?`. Show every remaining command, ranked, with its
description, risk label and score, and mark the best one. If only one is left
after merging, show just that one. This is the "there is only one way to do
it" case, so a single answer is a correct result and not an error. Choosing a
command leads to the same confirm step as `?`.

### `?x` — one answer, run it if it is safe

Steps 1–4 as for `?`. Then the top command goes through the **safety gate**
below. If it passes, it runs right away and the card shows "Ran without
asking: <reason>". If it fails, the result is exactly the `?` confirm step,
with a line saying why it was not run automatically. For example: "Not run
automatically: `rm` deletes files."

Typing `?x` gives permission for **that one query only**. Nothing is
remembered for later queries.

## Safety gate for `?x`

A command runs without asking only when **all** of these are true. If any
check fails, or cannot get an answer, the command falls back to `?`.

1. **Command checks pass** (they apply to every mode; see below).
2. **The local policy says the command is read-only** (rules below). This is
   code in `termite`, so no model can talk its way past it.
3. **The decision endpoint says `safe_unguarded`** in answer to the safety
   question.
4. **The model's own risk label is `read`.** If the generating model calls
   the command `write` or `destructive`, that is enough to stop it.
5. **The session is local** (otty widget). SSH sessions always fall back to
   `?`, because the policy cannot judge the remote machine's paths and tools.
6. **The shell is at an idle prompt** (otty widget). Nothing is written while
   another command is still running, the same rule as Rerun. The CLI is
   always at a prompt.

Each check can only make the answer stricter. The decision model can turn a
command down that the policy allows, but it can never approve one that the
policy turns down.

### Local policy: what counts as read-only

A command can run unguarded only if it looks at the machine without changing
it, does not need extra privileges, and sends nothing over the network.

**Shell features that are never allowed unguarded:**

- More than one command: `;`, `&&`, `||`, `&`, or a new line
- Writing files: `>`, `>>`, `>|`, `tee`, here-documents
- Running code built at run time: `$(…)`, backticks, `eval`, `source`, `.`,
  `exec`, `xargs`, process substitution `<(…)`
- Raising privileges: `sudo`, `doas`, `su`, `pkexec`
- Setting variables in front of the command (`FOO=bar cmd`), since these can
  change what a program does (`LD_PRELOAD`, `GIT_PAGER`, …)
- Subshells and zsh code in globs: unquoted `(` or `)`
- `!` history expansion (outside single quotes), and `$'…'` / `$"…"` text
  built from escape codes
- A program given by path (`./ls`, `/bin/rm`), which may not be the real
  program

**Pipes** are allowed only when every part of the pipe passes the program
rules below. For example, `ls ~/code | wc -l` passes.

**Programs allowed unguarded, with their limits:**

| Program | Allowed | Turned down |
|---|---|---|
| `ls`, `pwd`, `stat`, `du`, `df`, `wc`, `which`, `whoami`, `uname`, `id`, `uptime` | any options | — |
| `tree`, `file`, `date` | looking | `tree -o` (writes a file), `file -C` (writes a file), `date -s` / `--set` or an operand that is not `+format` (sets the clock) |
| `cat`, `head`, `tail`, `less`, `bat` | reading files | `tail -f` / `-F` / `--follow` (never ends), `less -o` / `-O` / `+command`, `bat --pager`, files matching the secrets list |
| `grep`, `rg`, `fd` | any search | `rg --pre`, `fd -x` / `-X` |
| `find` | searching | `-delete`, `-exec`, `-execdir`, `-ok`, `-okdir`, `-fprint*`, `-fls` |
| `git` | `status`, `log`, `diff`, `show`, `branch` (listing only), `remote -v`, `rev-parse`, `blame`, `ls-files`; global `-C <dir>`, `--no-pager` | everything else, any other global option (`-c`, `--exec-path`), `--output` |
| `docker` | `ps`, `images`, `inspect`, `logs` (without `-f`), `version`, `info` | everything else |
| `kubectl` | `get`, `describe`, `logs` (without `-f`), `version`, `config current-context`; `-n` / `--namespace` / `--context` before the subcommand | everything else, `get secret(s)` (also `secret/x` and in lists), `get -w` / `--watch` |
| `brew`, `npm`, `cargo` | `list`, `info`, `--version`, `outdated` | everything else |

**Secret files** are never read unguarded, even with `cat`: `~/.ssh/*`,
`*.pem`, `*.key`, `.env*`, `~/.aws/*`, `~/.config/gh/*`, `~/.netrc`,
`~/.docker/config.json`, `*.kdbx`. Reading them is still possible through
`?`.

Anything not on this list falls back to `?`. The list is meant to grow slowly,
one program at a time, with a table test for each one.

### Safety questions to the decision endpoint

One `/v1/systemone` request with three `noul` (true/false) questions. The
`state` holds the request, shell, OS, folder, session kind and the command.
Each question gives a description for both answers (Ollama requires them,
and Kev-4B separates more sharply with them):

```json
POST {decision_url}/v1/systemone
{
  "model": "{decision_model}",
  "state": "User request: …\nShell: zsh on macos\nFolder: …\nSession: local\nCommand: du -sh .",
  "questions": {
    "only_reads": { "type": "noul", "instructions": "Does this command only read …?",
                    "criteria": { "false": "it creates, changes or deletes something", "true": "it only reads or displays" } },
    "no_secrets": { "type": "noul", "instructions": "…", "criteria": { "false": "…", "true": "…" } },
    "harmful":    { "type": "noul", "instructions": "…", "criteria": { "false": "…", "true": "…" } }
  }
}

200
{ "answers": { "only_reads": { "noul": 0.99 }, "no_secrets": { "noul": 0.94 }, "harmful": { "noul": 0.03 } } }
```

Thresholds (calibrated on Kev-4B, `src/decide.rs`): it runs unguarded only
with `only_reads ≥ 0.85`, `no_secrets ≥ 0.6` and `harmful ≤ 0.3`;
`harmful ≥ 0.9` refuses; anything else asks. A missing answer or a value
outside 0–1 is an error, and errors fail closed.

## Command checks (every mode)

Model output gets written into a real shell, so every command is checked
before it is shown or written:

- **One line only.** Any `\n` or `\r` turns the command down.
- **No control characters.** Bytes below `0x20` (except a space), `0x7F`, and
  any escape sequence (`\x1b…`) turn the command down. They are not quietly
  removed, because removing them changes what the command means.
- **It must parse** as shell words (`shell-words`, already a dependency of the
  app). If it doesn't, it is turned down.
- **It has a length limit** (Question 4). Longer output usually means the
  model has gone wrong.
- **Not empty.**

A turned-down command never reaches the screen as something you can run. If
every command is turned down, show "No usable command" with the model's
explanation.

## Ranking request to the decision endpoint

One `choice` question whose options are the candidates (`c1`, `c2`, …, each
"command (description)"). The answer's `probabilities` are the scores, best
first; ties keep the model's order. One candidate needs no request (systemone
needs at least two options) and scores 1.0.

```json
POST {decision_url}/v1/systemone
{
  "model": "{decision_model}",
  "state": "User request: restart docker\nShell: zsh on macos\n…",
  "questions": {
    "best": { "type": "choice", "instructions": "Which shell command best does what the user asked?",
              "criteria": { "c1": "docker restart web (…)", "c2": "…" } }
  }
}

200
{ "answers": { "best": { "choice": "c1", "probabilities": { "c1": 0.82, "c2": 0.18 } } } }
```

An id that is not a candidate, or no probabilities, is an error.

## Generation request to the local model

The model gets a system prompt with the context (OS, shell, current folder,
session kind) and these rules: prefer standard tools; one command per
candidate; no chained commands unless the request needs them; give each
command a short description and an honest risk label. The response format is
a JSON schema:

```json
{
  "candidates": [
    { "command": "string", "description": "string", "risk": "read | write | destructive" }
  ]
}
```

The schema limits the array to 1–5 items. The model may return one item when
there is only one sensible command.

## Design Decisions

1. **One core library, two front ends.** `termite` (its own repo, crate
   `termite`, binary `tmite`) owns query parsing, the context, the model and decision
   calls, command checks, the safety policy and the mode logic. `tmite` and
   `otty-assist` only show results and write the chosen command. The part that
   touches your shell is tested once, in one place.
2. **The mode is chosen by the prefix, not by the model.** `?`, `??` and `?x`
   are read by a small parser in `termite` and then removed from the query.
   The model never decides how much freedom it has.
3. **One generation call for every mode.** The model is always asked for up to
   five commands, and the modes differ only after ranking. There is one prompt
   to tune, and the decision endpoint always has something to compare.
4. **Merging, not padding.** Commands that differ only in spacing, quoting or
   option order (`ls -la` and `ls -al`) are merged before ranking. `??` never
   invents alternatives to fill the list.
5. **Code first, model second, for `?x`.** The local policy is the real
   safety boundary. The request text, file names in the current folder and
   model output can all carry text meant to fool a model. The policy is plain
   Rust with table tests. The decision model is a second check that can only
   make things stricter.
6. **Fail closed.** If either endpoint times out, returns bad JSON, gives an
   unknown verdict, or the policy can't parse the command, `?x` falls back to
   `?`, and `?`/`??` show the error. Nothing ever runs because of a missing
   answer.
7. **Permission lasts one query.** `?x` gives permission for that request
   only. There is no "always allow" setting.
8. **Confirm means Enter.** In `?` and after a `??` choice, the command is
   shown with the confirm card focused: Enter runs it, Edit puts it at the
   prompt without running it, Esc cancels. In the zsh widget, the command is
   placed in the prompt buffer, so Enter is the confirmation and you can edit
   it first.
9. **Write through the terminal's own input path.** The otty widget sends
   `Event::Write` to the active terminal: the command alone for Edit, and the
   command followed by `\r` for Run. Because of the command checks, the bytes
   are always a single printable line.
10. **Nothing is written while a command is running.** The otty widget uses the
    same idle-prompt check as Rerun (command blocks ticket 06). If the shell is
    busy, Run is disabled and only Copy is offered.
11. **No separate audit log.** Every command that runs, guarded or not, shows
    up as a normal command block in otty and in shell history. The card for a
    `?x` run says it ran without asking and why. A second log would repeat
    this.
12. **The two outside services sit behind traits.** `CommandSource` (the local
    model) and `DecisionClient` (systemone) are traits because they cross an
    infrastructure boundary and need `mockall` mocks in tests. Each has exactly
    one HTTP implementation. No other extension points.
13. **One config file shared by both front ends.** The file is
    `~/.config/termite/config.json`. It holds the model URL, the model name, the
    decision URL and the time limits. A decision endpoint token, if needed, is
    read from the `TERMITE_DECISION_TOKEN` environment variable and is
    never written to the file.
14. **New dependency: `reqwest`** 0.13 (async; features `json` and `rustls`,
    default features off; approved 2026-11-04) for both HTTP calls. It runs on the tokio runtime that iced already uses. It needs
    the owner's approval before it is added. `otty-assist` pulls it in
    through `termite`, so the otty fork must also pass `cargo deny check`.

## Setup (`tmite setup`)

`tmite setup` finds local servers and writes the config:

- **Finds** llama.cpp (including the Llama app's router, read from `ps` for
  its port and `--models-max`), Ollama and LM Studio on their default ports,
  plus any URL already in the config. llama.cpp names itself in
  `/v1/models`; Ollama answers `/api/version`; LM Studio answers
  `/api/v0/models`. Decision models are told apart by the router's
  `decision` tag or Ollama's `decision` capability.
- **Lists** installed servers that are not running, with how to start them.
- **Asks** for a chat model and a decision model (Enter takes the default:
  loaded first, then llama.cpp).
- **When one is missing,** it prints the exact command and the download size
  (from the Hugging Face API) and runs it only after the user types `y`:
  `ollama pull`, `hf download` into the Hugging Face cache for the Llama app,
  or a detached `llama-server -hf <repo>:<quant>` on 127.0.0.1 (log in
  `~/.cache/termite/`), which setup waits for.
- **Warns** when both models are on one router that keeps one model loaded.
- **Writes** `~/.config/termite/config.json` after showing it and asking,
  with 120 s / 60 s timeouts for model loading. The decision token is never
  written; it stays in `TERMITE_DECISION_TOKEN`.
- Without a terminal, choices take their default and every yes/no is no:
  nothing is downloaded, started or written.

## Front ends

### `tmite` (CLI and zsh widget)

- `tmite '<query with prefix>'` prints a result. `--json` prints the
  structured result for scripts.
- A zsh widget, `shell/tmite.zsh` in this repo, takes over Enter. When the prompt buffer
  starts with `?`, it does not run the line. In zsh, a bare `?` would also be
  read as a glob pattern. Instead it calls `tmite --zle` (a plain line format; `--json` is for scripts) and then:
  - `?`: replaces the buffer with the command and shows the description under
    the prompt (`zle -M`). Enter runs it.
  - `??`: shows a numbered list. Picking a number puts that command in the
    buffer.
  - `?x`: if the gate passed, sets the buffer and runs it (`zle accept-line`).
    Otherwise it behaves like `?` and shows the reason.
- Bash support is out of scope for this spec.

### `otty-assist` (otty widget)

- `app/src/widgets/assist/` follows the usual widget layout.
- **Palette:** when the palette input starts with `?`, the list shows one
  entry, "Ask: <request>". Choosing it sends `PaletteTarget::Ask { query }`.
- **Intents:** `Submit { query }`, `Answered(Result)`, `Choose { index }`,
  `Run`, `Edit`, `Copy`, `Cancel`.
- **Effect:** `WriteToTerminal { tab_id, terminal_id, bytes }`. The app turns
  it into `TerminalWorkspaceIntent::Widget(Event::Write { … })` in
  `app/src/events/assist.rs`.
- **Card:** command (monospace), description, risk label as text and color
  (never color alone), score. For `??`, a list. For a `?x` fallback, the
  reason line. A spinner shows while waiting, and Esc cancels the request.
- **Context comes from the active terminal:** the current folder from its
  blocks, and the session kind (local or SSH) from its settings.
- All new text goes through `i18n` (English first, other languages fall back to
  English).

## Owner Answers

Round 1, 2026-11-04:

1. **Decision endpoint format:** use the `/rank` + `/safety` formats in this
   spec as the contract. systemone is fitted to them. Auth is the optional
   bearer token from `TERMITE_DECISION_TOKEN`.
2. **Local model:** a 7–8B instruct model (for example Qwen2.5-Coder-7B or
   Llama 3.1 8B Instruct). The model name stays in termite's `config.json`. The prompt
   is tuned for this size: short rules, the schema does the formatting work.
3. **`?x` in SSH sessions:** always falls back to `?`.
4. **Command length limit:** 500 characters (proposed default, not
   challenged).
5. **Time limits:** 20 s for generation, 5 s for each decision call; a
   timeout fails closed (proposed default, not challenged). Both can be
   changed in termite's `config.json`.
6. **Allowlist:** keep the table as written. It grows one program at a time,
   each with a table test.

## Acceptance Criteria

- [x] AC1: The query parser returns the mode and the request for `?`, `??` and
      `?x`, with and without spaces after the prefix. Other input is not a
      query (table test).
- [x] AC2: The command checks turn down new lines, control characters, escape
      sequences, unparseable input, empty input and too-long input (table
      test).
- [x] AC3: Merging joins commands that differ only in spacing, quoting or
      option order, and keeps the best description (table test).
- [x] AC4: The policy returns read-only for every allowed example and not
      read-only for every turned-down example in "Local policy", including each
      banned shell feature and each secret path (table test, at least one row
      per rule).
- [x] AC5: `?x` runs without asking only when all six gate checks pass. With
      mocks, each failing check alone, each endpoint error, a timeout and bad
      JSON all give the `?` result with a reason.
- [x] AC6: `??` with one remaining command returns one command and no error.
- [ ] AC7: The otty widget writes the command alone for Edit and the command
      plus `\r` for Run, to the terminal that was active when the query was
      sent. It writes nothing while that terminal is running a command.
- [x] AC8: The zsh widget handles all three modes and leaves lines that don't
      start with `?` untouched (manual check, written down in the ticket).
- [ ] AC9: in termite, `cargo fmt --check`, clippy with `-D warnings` and
      `cargo test` pass. In the otty fork, ticket 06 also meets otty's own
      checks: `cargo +nightly fmt`, clippy, `cargo test --workspace
      --all-features`, `cargo deny check`, and line coverage strictly greater
      than on the merge base.

## Out of Scope

- Bash and fish widgets.
- Using the last command's output as context ("fix that error").
- Commands that span several lines, and scripts.
- Streaming the model's answer as it is written.
- Any "always allow" or remembered permission.
- Other model backends (Ollama, hosted APIs). The model endpoint is
  OpenAI-compatible, so they may work, but they are not tested.

## Progress Tracking

- [x] 01 — `termite` crate: query parser, command checks, merging · Sonnet
- [x] 02 — Read-only policy and its table tests · Opus
- [x] 03 — `CommandSource` (llama-server) and `DecisionClient` (systemone), with mocks; needs approval for `reqwest` · Sonnet
- [x] 04 — Mode logic and the `?x` safety gate · Opus
- [x] 05 — `tmite` CLI and the zsh widget · Sonnet
- [x] 07 — `tmite setup`: discovery, model choice, downloads, local inference · Opus
- [ ] 06 — `otty-assist`: otty widget, palette entry and terminal write; needs command blocks ticket 06 (idle prompt check) · Opus

Order: 01 → 02 → 03 → 04, then 05 and 06 (they touch different files). Each
ticket names its model on its `Model:` line; one subagent and one commit per
ticket.

## Round Log

- Draft (2026-11-04): written from the otty review and the owner's mode
  requirements (`?`, `??`, `?x`; core crate + CLI + otty widget).
- Round 1 (2026-11-04, owner): the proposed decision formats, a 7–8B model, no
  `?x` over SSH, and the allowlist as written. Length and time limits kept at
  the proposed defaults. Spec approved; tickets 01–06 written.
- Round 2 (2026-11-04, owner): the core is `termite`, its own repo, with the
  CLI `tmite`; `otty-assist` is only the otty integration (ticket 06).
  Tickets 01–05 are built in termite; ticket 06 is built in an otty fork.
- Round 3 (2026-11-04, owner): the decision client uses llama.cpp's
  `/v1/systemone` instead of `/rank` and `/safety` (config gains
  `decision_model`); thresholds calibrated on Kev-4B. Added `tmite setup`
  (ticket 07): detect popular local servers, suggest Hugging Face downloads
  and run them only after `y`, and start local llama-server inference.
  Training our own small models is drafted in
  `docs/training-tiny-models.md`.
