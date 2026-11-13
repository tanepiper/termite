# How termite is built

termite is a safety tool, so readers must be able to check it. This page
describes the design. `tests/architecture.rs` fails the build when the code
does not match it, so a change to the design must update this page in the same commit.

## Rules

1. **Small files.** No file has more than 400 lines of code, not counting
   tests. A file that grows is split along a boundary a reader would name.
2. **One direction.** A module uses only the modules listed for it below.
   There are no cycles.
3. **Side effects stay in the binary.** The library prints nothing, starts no process
   and spawns no thread. Only `generate` and `decide` talk to a server. The
   `tmite` binary owns the terminal, processes and threads.
4. **Every input is bounded.** Anything read from a model, a server or a
   repository has a limit, listed under Memory.
5. **Errors never loosen a check.** Doubt, an error or a timeout makes the
   answer stricter.
6. **Model text is data.** It is never run by termite and never written to a
   terminal without `to_printable`.

## Modules

Each row may use only the rows above it.

| Module | Job | Contract | Uses |
|---|---|---|---|
| `candidate` | a command, its description and risk label | plain data | |
| `hazard` | the list of known hazards, and how serious each one is | pure; records what it finds and changes nothing | |
| `place` | what separates this process from the machine | reports only; the eval tool decides what to do with the answer | |
| `command` | checks on any command a model wrote | pure; `check_command` and `to_printable` read only their argument | |
| `context` | the OS, shell, folder and session | plain data | |
| `error` | every way an answer can fail | typed; says which service failed | |
| `home` | termite's private folder and script names | refuses folders that others can write to; writes files 0600 | |
| `provenance` | the log of what termite installed | hashes only; termite never adds to a log that fails verification | `home` |
| `progress` | the current stage of an answer | carries only termite's own words and numbers; cannot change the answer | |
| `heartbeat` | checks the server during a long wait | reports only; the time limit decides when to give up | `progress` |
| `query` | reads `?`, `??`, `?x` | pure | |
| `sse` | reads a streamed reply | any split of the bytes gives the same reply; bounded | `error` |
| `merge` | joins commands that do the same thing | pure; the most harmful risk label wins | `candidate` |
| `policy` | what `?x` may inject with no question | an allowlist; anything unknown is refused; reads only git config | `command` |
| `rating` | the rating each command shows | a model's label can raise a rating and never lower it | `candidate`, `hazard`, `policy` |
| `classify` | puts every command at one of six levels | plain code, no model; doubt raises a level, and the level before doubt is kept beside it; a model's label can only raise it; reads the disk only to find the project and symlinks, to read a script a command runs, and to read the names of files below a folder a command searches | `candidate`, `hazard`, `policy` |
| `script` | reads a script or a stream and rates each command | reads text only; memory stays bounded; anything it cannot read is risky; a here-document and an open quote stay in one statement | `classify`, `command`, `hazard`, `policy`, `rating` |
| `config` | the config file | checks URLs; holds no token | `error`, `home` |
| `decide` | the decision model's client | can only make an answer stricter than the thresholds allow | `candidate`, `command`, `config`, `context`, `error` |
| `generate` | the chat model's client | 1 to 5 candidates, or a typed error, which includes the model declining | `candidate`, `command`, `config`, `context`, `error`, `heartbeat`, `progress`, `sse` |
| `draft` | writes a script as a draft | saves nothing itself and runs nothing; a critical hazard is withheld with no second try | `command`, `context`, `decide`, `error`, `generate`, `hazard`, `home`, `progress`, `rating`, `script` |
| `answer` | the flow: generate, check, merge, rank, gate | never fails open; local checks run before the network | `candidate`, `classify`, `command`, `context`, `decide`, `error`, `generate`, `hazard`, `merge`, `policy`, `progress`, `query` |

Inside a module, `policy` is split into `shell` (reading a command line),
`programs` (the allowlist), `options` (shared option checks), `services`
(git, docker, kubectl, package tools), `secrets` (sensitive paths) and
`git_config` (a repository's own config).

Inside `classify`, the parser (`parse`, with `parse/heredoc`, `parse/vars`
and `parse/groups`) splits a command into stages. It keeps the text of each
here-document apart from the command, puts the value of a variable where the
same command uses it, and reads the body of a function, a `( … )` group and
a `case` branch as commands where they stand. `walk` rates each stage, and
`files` judges here-document text by the program that receives it. `reads`
looks below a searched folder for the name of a secret. `verdict` adds what
only the whole line shows. An agent is judged on the level before doubt, and
on three flags: it may show private data, it can carry data off the machine,
or it runs code that can't be read.

The binary adds `check` (`tmite check`), `scripts` (`tmite script` and
`tmite scripts`), `script_tools` (the only code that calls `bash -n` and
shellcheck, which parse and never run), `library` (installs the bash library),
`provenance` (`tmite provenance`, `tmite alerts`) and `provenance_key` (the only
code that talks to the system key store).

The binary has `main` (arguments and wiring), `output` (the three output
formats), `progress` (the progress line), `shell` (prints the widgets), `hooks`
(see [agent hooks](#agent-hooks)) and `setup` (`discover`, `machine`, `fetch`,
`installed`, `prompt`, `fix`, `hub`, `profile`, `widget`).
`hub` reads `catalog.json`, the list of models that setup can fetch. Each entry
is one GGUF file with a pinned revision, size and hash, and `fetch` won't give a
file its name until all of them match.

## Flow

```text
request ─▶ query ─▶ generate ─▶ command checks ─▶ merge ─▶ decide.rank
                                                              │
                                  ?  ──▶ Confirm  ◀───────────┤
                                  ?? ──▶ Choose   ◀───────────┤
                                  ?x ──▶ policy ─▶ decide.safety ─▶ StraightIn
                                                                   Confirm
                                                                   Refused
```

`tmite` only prints. The widgets in `shell/`, one each for zsh, bash and fish, place
the command at the prompt. `tmite shell` prints them, so nothing has to be copied
into place.

`scripts/team.sh` is the only route the [agent tools](agents.md) use. `place.rs` says
what separates this process from the machine, and the eval tool's `gate.rs` decides
whether a model test may start. Keep those two checks in code, never in a prompt.

## Agent hooks

```text
harness ─▶ tmite hook NAME ─▶ adapter ─▶ check ─▶ answer in the harness's shape
             person ─▶ tmite hooks ─▶ plan ─▶ shown ─▶ y ─▶ settings file changes
```

`hooks` has these parts:

| Part | Job | Rule |
|---|---|---|
| `harness` | the table of harnesses: where each keeps its hooks, where its message holds the command, and how it is told to stop | a new harness is mostly a new row |
| `adapter` | `tmite hook NAME`: reads the message, rates the command as `TERMITE_AGENT=1 tmite check` does, and answers pass, needs approval or stop; only level 5 and critical hazards are refused with nobody asked | a pass never approves anything; a question goes out only to a part of a harness that was tried, shown by the message itself; anything else is a stop |
| `plan` | works out what installing or removing changes | writes nothing; the result is a list of files with their old and new text |
| `entries` | builds the text of each hook | only builds text; the shell line stops the command if `tmite` is missing |
| `json` | keeps the key order of a person's settings file | a change shows as a small change |

The adapter gives up and stops the command itself after 20 seconds, before the
harness's own timeout. It answers before it writes to the log, because writing
can wait on the Keychain. `plugins/claude-code` holds the same hook as a Claude
Code plugin, and `tests/hooks/` holds recorded messages from each harness.
[Agent tools](agents.md#what-a-hook-does) explains the hooks for people who use them.

## Memory

| Input | Limit | Where |
|---|---|---|
| A model's reply | 256 KiB | `sse::MAX_REPLY_BYTES` |
| A decision server's reply | 64 KiB | `decide::MAX_REPLY_BYTES` |
| One line of a stream | 64 KiB | `sse` |
| Candidates | 5 | `generate::MAX_CANDIDATES` |
| One command | `MAX_COMMAND_CHARS` | `command` |
| A git config file | 1 MiB; larger counts as unsafe | `policy::git_config` |
| A message from a harness | 4 MiB; larger stops the command | `hooks::adapter` |
| Any request | the configured time limit | `generate`, `decide` |

A stream keeps only the reply text and one partial line. The progress thread
keeps the current stage, and ends before the answer is printed.

## Checks

| Check | Proves |
|---|---|
| `cargo test` | behaviour, including every refusal table |
| `tests/architecture.rs` | the rules on this page |
| `tests/widget.rs` | the real zsh, bash and fish widgets in a pseudo-terminal |
| `tests/cli.rs` | the real binary against a hostile fake server |
| `tests/hooks.rs` | recorded messages from each harness, through the real hook |
| `tests/gates.rs` | the list of commands and the level each must have |
| `src/adversarial_tests.rs` | hostile commands in every disguise, and random input |
| `scripts/sonar.sh` | Clippy, coverage, ShellCheck, Semgrep, cargo-audit, and the quality gate |
| `scripts/analyse.sh` | the tests plus the model evaluation, over time; see [Evaluating models](evaluation.md) |

Tests and evaluations run through `scripts/sandbox.sh`: loopback network only,
writes only to build and result folders, secrets unreadable, a throwaway `HOME`.
