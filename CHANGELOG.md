# Changelog

## Unreleased

### Check

- Stop a suggestion when a loop or a branch never closes, or when a pipe has nothing in it.
- Raise commands that replace an existing file, lose uncommitted work or show settings.
- Let an agent do ordinary work inside the project, up to level 3. Before, a hook stopped most git, build and edit commands.
- Read a command as one text: here-documents, variables the command sets, functions, groups and `case`.
- Ask a person about a level 4 command where the harness can ask. Only level 5 and critical hazards are refused with nobody asked.
- Stop a change to a file that runs later by itself: a shell startup file or a git hook.
- Search code written in a command, such as `python3 -c`, the way a script file is searched. Before, all such code stopped an agent.
- Let `gh api` and `curl` pass when they only read.

### Test

- Add version 0.2 of the command and decision models, trained only on data with clear licences.

## 0.1.0 - 2026-11-06

The first release of termite. It covers everything built so far, at a high level.

### Ask

- Turn a plain request into a shell command with a local model, with `?`, `??` and `?x` in your shell.
- Choose between several commands, each with a rating and a reason.
- Write a script as a draft, and check it before you install it.
- Set up harnesses and local models with `tmite setup`. It works with llama.cpp, Ollama and LM Studio.

### Check

- Rate every command at one of six levels, in plain code, with no model involved.
- Rate scripts and piped commands with `tmite check`.
- Use a decision model to rank commands and answer safety questions. It can only make an answer stricter.

### Gate

- Keep root commands from the prompt, and withhold hostile commands. No setting lowers a guard.
- Check an agent's shell commands before they run, with hooks for agent harnesses.
- Turn down suggestions that can't be used as written: cut-off lines, placeholders, and programs that aren't installed.
- Keep a signed log of what termite installed and what its guards stopped.

### Test

- Publish safety ratings for local models, from tests that run in a sandbox.
- Test what commands really do in a throwaway machine with no network.
- Publish two small models for termite: `termite-command-4b` and `termite-decide-0.8b`.
