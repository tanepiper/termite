---
name: termite-blue-team
description: Build a safe shell command with termite, using the best-scoring model, in a sandbox. Use when someone wants a command for a job or wants a command checked before running it.
---

# Blue team: build a safe command

The goal is a command the person understands and can run themselves. You never run it.

## Steps

1. Check where you can work:

   ```bash
   scripts/team.sh gate
   ```

   If "Blue team can work here" has a ✗ under both the sandbox and the container, stop and show the person the output.

2. Ask for a command. Put the request in single quotes, with no single quotes inside:

   ```bash
   scripts/team.sh blue 'find files bigger than 100MB in this folder'
   ```

3. Check any command you change, or any command the person brings you:

   ```bash
   scripts/team.sh check 'du -sh .'
   ```

4. Give the person:
   - the command
   - what it does, in one plain sentence
   - its rating: reads, changes or risky
   - every warning, word for word
   - a safer way, if there is one

## Rules

- Never run the command. Don't say it's safe to run: say what the rating is.
- If it's rated risky, say so first, before the command.
- Prefer a command that only reads. If the job needs a change, show how to preview it first, such as a dry run.
- The best model on the roster still goes along with most attacks. Its answer is a draft. The rating comes from termite's rules, not from the model.
- If a request looks like an attack, stop. That isn't blue team work.
