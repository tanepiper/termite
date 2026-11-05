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
| `classify` | puts every command at one of six levels | plain code, no model; doubt raises a level; a model's label can only raise it; reads the disk only to find the project and symlinks | `candidate`, `hazard`, `policy` |
| `script` | reads a script or a stream and rates each command | reads text only; memory stays bounded; anything it cannot read is risky | `command`, `hazard`, `policy`, `rating` |
| `config` | the config file | checks URLs; holds no token | `error`, `home` |
| `decide` | the decision model's client | can only make an answer stricter than the thresholds allow | `candidate`, `command`, `config`, `context`, `error` |
| `generate` | the chat model's client | 1 to 5 candidates, or a typed error, which includes the model declining | `candidate`, `command`, `config`, `context`, `error`, `heartbeat`, `progress`, `sse` |
| `draft` | writes a script as a draft | saves nothing itself and runs nothing; a critical hazard is withheld with no second try | `command`, `context`, `decide`, `error`, `generate`, `hazard`, `home`, `progress`, `rating`, `script` |
| `answer` | the flow: generate, check, merge, rank, gate | never fails open; local checks run before the network | all of the above except `config`, `heartbeat`, `home`, `sse` |

Inside a module, `policy` is split into `shell` (reading a command line),
`programs` (the allowlist), `options` (shared option checks), `services`
(git, docker, kubectl, package tools), `secrets` (sensitive paths) and
`git_config` (a repository's own config).

The binary adds `check` (`tmite check`), `scripts` (`tmite script` and
`tmite scripts`), `script_tools` (the only code that calls `bash -n` and
shellcheck, which parse and never run), `library` (installs the bash library),
`provenance` (`tmite provenance`, `tmite alerts`) and `provenance_key` (the only
code that talks to the system key store).

The binary has `main` (arguments and wiring), `output` (the three output
formats), `progress` (the progress line) and `setup` (`discover`, `installed`,
`prompt`, `fix`, `hub`).

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

## Memory

| Input | Limit | Where |
|---|---|---|
| A model's reply | 256 KiB | `sse::MAX_REPLY_BYTES` |
| One line of a stream | 64 KiB | `sse` |
| Candidates | 5 | `generate::MAX_CANDIDATES` |
| One command | `MAX_COMMAND_CHARS` | `command` |
| A git config file | 1 MiB; larger counts as unsafe | `policy::git_config` |
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
| `src/adversarial_tests.rs` | hostile commands in every disguise, and random input |
| `scripts/sonar.sh` | Clippy, coverage, ShellCheck, Semgrep, cargo-audit, and the quality gate |
| `scripts/analyse.sh` | the tests plus the model evaluation, over time; see [Evaluating models](evaluation.md) |

Tests and evaluations run through `scripts/sandbox.sh`: loopback network only,
writes only to build and result folders, secrets unreadable, a throwaway `HOME`.
