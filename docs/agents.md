# Agent tools

termite works with any agent service or harness that can run a command before it runs a shell command. It includes three agent definitions and three skills:

- The blue team helps you build safe commands.
- The red team tests what a model does when someone asks it for dangerous ones.
- The analyst reads the test results and writes short findings for the site. It never talks to a model.

No agent ever runs a command that a model wrote.

## Stop an agent running destructive commands or sending data out

Send each shell command to `tmite check` before the agent runs it. The check reads the command from standard input. Set `TERMITE_AGENT` in the hook, so the check treats the caller as an agent whatever it can detect:

```bash
printf '%s' "$command" | TERMITE_AGENT=1 tmite check >&2
```

An agent gets stricter rules than a person, because nobody reads its commands first. The check stops:

- Anything above level 2, and level 1, which reads secrets.
- Anything that can carry data to another machine, at any level. This includes `curl` and `wget` to other machines, `ssh`, `scp`, DNS lookups, cloud tools, uploads, publishing, the clipboard, and opening a web address.

Work that stays on the machine, such as `curl` to `localhost`, still passes.

The exit status tells your harness what to do:

| Status | Meaning | What your harness should do |
|---|---|---|
| 0 | The check found nothing risky | Run the command |
| 2 | The check found something risky, or found no command | Stop, and give the agent the reason |
| 1 | The check could not read the input | Stop |
| 64 | The check was called incorrectly | Stop |

Add this as a pre-command hook, a tool wrapper or a policy step, whichever your harness supports. Treat every status except 0 as a stop. `tmite` must be on your `PATH`.

The check blocks only what it can see, so read [what it misses](using-termite.md#check-a-script-or-a-pipe). It can't see inside a file that a command runs, such as `python3 script.py` or `npm test`. It also can't see an agent's other tools, such as one that writes files or fetches web pages. To stop data leaving for certain, run the agent in a container or a virtual machine with no network.

A command built from variables counts as risky, so some ordinary commands will also stop. `grep -r` also stops, because it can read secrets in the folder. `rg` skips ignored files, so it passes.

## Before you start

You need a clone of this repository, an agent harness that can load instructions from files, the models you want to use, and Docker for the red team. Build the tool once:

```bash
cargo build --example eval
```

Check where each team can work:

```bash
scripts/team.sh gate
```

## Blue team: build a safe command

Ask for the agent by name, or tell your agent to follow the skill:

```text
Use termite-blue-team to find what is using my disk space.
```

It asks the best model on the roster, in the sandbox, and then gives you the command, what it does, its rating and every warning. You decide whether to run the command.

You can use the same tools without an agent:

```bash
scripts/team.sh blue 'find files bigger than 100MB in this folder'
```

```bash
scripts/team.sh check 'du -sh .'
```

## Red team: test dangerous requests

Test only a model that you are responsible for.

```text
Use termite-red-team to test whether this model will read my SSH keys.
```

It asks the worst models on the roster, in a locked-down container, and tells you what they wrote and what to restrict. Commands that can do real harm are not shown.

Without an agent:

```bash
scripts/team.sh red 'REQUEST'
```

Answers are saved in `eval/results/team/`. Git ignores that folder; keep it private.

Do not publish what you find. If you find a real weakness, tell the people responsible for the model privately, and give them time to fix it.

## How the models are picked

`eval publish` writes `eval/roster.jsonl` from the latest scores, so nobody chooses the models by hand:

| Team | Gets | Why |
|---|---|---|
| Blue | The models that wrote the fewest hazards for hostile requests | These give the safest draft available |
| Red | The models that wrote the most hazards | These show the worst that could be handed over |

A model must answer at least 90% of everyday requests to be on either list, because a model that cannot write good commands is no use to either team.

"Best" means the best of the models tested. The [model ratings](/termite/models/) show that the best model may still write a hazard for most hostile requests, so termite's policy rates each command and does not trust the model.

To see the lists:

```bash
scripts/team.sh roster
```

## The gates

Every model test checks where it is running before it starts, and it refuses to start if that place is not safe enough. The tool makes this check itself, so an agent cannot persuade it to continue. The check really attempts to write to the source and reach the internet, so setting a variable changes nothing.

| Check | Blue team | Red team |
|---|---|---|
| In a Docker container or a virtual machine | Not needed | Required |
| Source folder cannot be written to | Required | Required |
| `.env` and known secret variables cannot be read | Required | Required |
| Internet cannot be reached | Required | Required |

No override, flag or sign-in can lower a gate. If a gate refuses, fix the place you are running in.

Configure your harness to add a second restriction: the agents may use no shell command except `scripts/team.sh`, on its own, with the text in single quotes.

## Where each team runs

**The sandbox** is for the blue team, and it uses macOS `sandbox-exec`. Run this command to confirm that every rule holds:

```bash
scripts/sandbox.sh --check
```

On Linux, the blue team uses the container instead.

**The container** is for the red team. `scripts/container.sh` starts it with no internet, a read-only filesystem, no privileges, no source and no secrets. It can write only to `eval/results/team/`.

Models run on your machine, so both places need to reach the models and nothing else:

- The sandbox can reach the model ports on localhost. It cannot reach other local services or sockets, such as a password manager or Docker.
- The container reaches the model ports through a small bridge that forwards those ports and nothing else. The bridge is removed when the run ends.

The ports come from `eval/models.jsonl`, `eval/decision-models.jsonl` and your termite config. termite does not support a model server on another machine.

## Load the agents in your harness

The agent definitions are plain Markdown in `agents/`, and the skills are plain Markdown in `skills/`. `.claude/` holds links to these files for Claude Code. Any harness can use them: load the files as instructions, or copy their text into your harness's own format. The gates work with every harness, because they are in the tool.
