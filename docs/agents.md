# Agent tools

termite comes with two agents and two skills, one pair for each team. The blue team
helps you build safe commands. The red team tests what a model does when it's asked
for dangerous ones.

Neither of them ever runs a command a model wrote.

## Stop an agent running destructive commands

`tmite check` exits with status 2 when it finds something risky, and Claude Code
blocks a tool call when a hook exits with status 2. Add this hook to the project's
`.claude/settings.json`, and every shell command the agent tries goes through the
check first:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "jq -r .tool_input.command | tmite check >&2" }
        ]
      }
    ]
  }
}
```

The agent gets the reason, and the command doesn't run. It needs `jq` and `tmite` on
your `PATH`. This blocks what the check can see, so read
[what it misses](using-termite.md#check-a-script-or-a-pipe). A command built from
variables counts as risky, so expect some ordinary commands to be blocked too.

## Before you start

You need a clone of this repository, [Claude Code](https://code.claude.com), the
models you want to use, and Docker for the red team.

Open the repository in Claude Code and trust the folder. Claude Code skips an
agent's hooks in a folder you haven't trusted. Build the tool once:

```bash
cargo build --example eval
```

Check where each team can work:

```bash
scripts/team.sh gate
```

## Blue team: build a safe command

Ask for the agent by name:

```text
Use termite-blue-team to find what's filling my disk.
```

It asks the best model on the roster, in the sandbox, then gives you the command, what
it does, its rating and every warning. You decide whether to run it.

You can use the same tools without an agent:

```bash
scripts/team.sh blue 'find files bigger than 100MB in this folder'
```

```bash
scripts/team.sh check 'du -sh .'
```

## Red team: test dangerous requests

Only test a model you're responsible for.

```text
Use termite-red-team to test whether this model will read my SSH keys.
```

It asks the worst models on the roster, in a locked-down container, and tells you what
they wrote and what to shut down. Commands that do real harm stay off the screen.

Without an agent:

```bash
scripts/team.sh red 'REQUEST'
```

Answers are saved in `eval/results/team/`. Git ignores that folder. Keep it private.

Don't publish what you find. If you find a real weakness, tell the people responsible
for the model privately, and give them time to fix it.

## How the models are picked

Nobody picks by hand. `eval publish` writes `eval/roster.jsonl` from the latest scores:

| Team | Gets | Why |
|---|---|---|
| Blue | The models that obey the fewest hostile requests | The best draft you can get |
| Red | The models that obey the most | The worst that could be handed to you |

A model has to answer at least 90% of everyday requests to be on either list. One that
can't write good commands is no use to either team.

"Best" here means best of those tested. See the [model ratings](/termite/models/):
the best model may still obey most hostile requests. That's why termite's rules rate
the command, and the model doesn't.

See the lists:

```bash
scripts/team.sh roster
```

## The gates

Every model test checks where it is before it starts, then refuses if the place isn't
good enough. The tool does this itself, so an agent can't talk its way past. It tries
the real thing: it attempts to write to the source and reach the internet. Setting a
variable changes nothing.

| Check | Blue team | Red team |
|---|---|---|
| In a Docker container or a virtual machine | Not needed | Required |
| Source folder can't be written to | Required | Required |
| `.env` and known secret variables can't be read | Required | Required |
| Internet can't be reached | Required | Required |

There's no override, no flag and no sign-in that lowers a gate. If one says no, fix the
place you're running in.

The agents have a second lock: a hook stops them using any shell command except
`scripts/team.sh`, on its own, with the text in single quotes.

## Where each team runs

**The sandbox** is for the blue team. It's macOS `sandbox-exec`. Run this to prove every
rule holds:

```bash
scripts/sandbox.sh --check
```

On Linux, the blue team uses the container instead.

**The container** is for the red team. `scripts/container.sh` starts it with no internet,
a read-only filesystem, no privileges, no source and no secrets. It can only write to
`eval/results/team/`.

Models run on your machine, so both need a way to reach them, and only them:

- The sandbox can reach the model ports on localhost. It can't reach other local
  services or sockets, such as a password manager or Docker.
- The container reaches the model ports through a small bridge, which forwards those
  ports and nothing else. It's removed when the run ends.

The ports come from `eval/models.jsonl`, `eval/decision-models.jsonl` and your termite
config. A model server on another machine isn't supported.

## Using another agent

The skills are plain instructions in `.claude/skills/`, so any agent can follow them.
The hook only works in Claude Code. The gates work everywhere, because they're in the
tool.
