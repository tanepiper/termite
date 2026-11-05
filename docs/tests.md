# The tests

This page describes each test: what it sends, what it records, where it ran, and what it cannot show. It gives the number and the kind of requests. It does not give the words of the requests, or the instructions that termite sends to a model.

## What every model test has in common

- **Nothing runs.** A model's commands are recorded and checked. No command that a model wrote is ever run.
- **Each request starts fresh.** A model sees no earlier request, unless the test adds text on purpose.
- **Time limit.** A model has 120 seconds to answer. After that, the trial ends as a Timeout.
- **One row for each trial.** Every trial records the model, the machine, the code version and how the trial ended.
- **No request or command in the data.** The rows on [Explore](/termite/explore/) hold ids, numbers and fixed words.

A trial with a command model has these steps:

1. termite sends the request to the model, with its usual instructions.
2. The model replies with commands, or it declines.
3. termite compares each command with its list of known hazards. This step only records the result, and it does not change what happens next.
4. termite's policy checks each command.
5. A decision model rates the commands that the policy did not block.
6. The trial gets one [result](evaluation.md#result-words), and one [failure type](failures.md) if something went wrong.

## Core requests

**Question:** what does a model write, and what does termite then do with it?

This is the main test. Each model gets 161 requests, once each.

| Intent | Requests | What it is |
|---|---|---|
| Everyday | 40 | An ordinary job on a computer, in plain words |
| Risky | 21 | A legitimate job that can cause harm if it goes wrong |
| Hostile | 100 | An attack, or a request that a careful person would refuse |

The 100 hostile requests are written in four ways:

| Type | Requests | How it is written |
|---|---|---|
| Direct attack | 36 | It asks for the harmful thing openly |
| Prompt injection | 23 | It hides an instruction inside other text |
| Obfuscation | 22 | It disguises what the command does |
| Pretext | 19 | It gives a false reason to make the request look acceptable |

The requests cover these areas: deletion, the system, files, secrets, persistence, remote code, version control, containers, packages, system defences, data theft and termite itself.

Each request has a level from 1 to 7. Levels 1 and 2 are everyday, levels 3 and 4 are risky, and levels 5 to 7 are hostile.

**How it is scored:**

- A hostile or risky request is scored on whether the model wrote a hazard, and whether termite stopped it.
- An everyday request is also scored on whether the command does the job. A wrong command is a Wrong answer.

**Where it ran:** the Local Sandbox on Mac, and on Hugging Face (ZeroGPU, T4 GPU and CPU). Every run shows its machine.

**What it cannot show:**

- Each request ran once for each run, so one result can be chance. Repeated requests measure this.
- The hazard list has only known hazards. A new kind of harm is not counted.
- Some runs reached fewer than 161 requests. Compare runs of the same test set.

## Outside requests

**Question:** do the core results hold on requests that I did not write?

This test sends 120 requests from public collections:

| Requests | Source | What it is |
|---|---|---|
| 60 | A public set of shell requests | Everyday jobs |
| 30 | A public set of prompt injections | Text that tries to change what a model does |
| 30 | A public set of harmful requests | Requests that a model should refuse |

Each public set is pinned to one version and checked by hash.

**How it is scored:** in the same way as core requests.

**Where it ran:** on the Local Sandbox on Mac. Two models ran this test, a model and its copy with safety training removed, twice each.

**What it cannot show:** many of these requests are not about the shell, so a command is often not a sensible reply. Two models are too few to compare models.

## SSH requests

**Question:** what do models write for jobs on the SSH server, and what does termite do with it?

The other request sets had no request about the SSH server. This set adds 12 requests. They are kept apart from the core requests, so that older runs can still be compared.

| Requests | What it is |
|---|---|
| 3 | Checking a machine: versions, settings and locked accounts |
| 3 | Fixing a machine, such as turning off password logins |
| 3 | Hostile requests that weaken SSH logins |
| 3 | The same hostile requests, hidden in other text |

**How it is scored:** in the same way as core requests. A change to the SSH server must never be Injected.

**Where it ran:** on the Local Sandbox on Mac.

**What it cannot show:** 12 requests are a small sample. No login is attempted, so this test does not show whether a machine is affected by any SSH bug.

## Repeated requests

**Question:** does a model give the same answer twice?

A model can give different answers to the same request. This test sends the same requests again and again.

- It uses 14 requests: 2 from each of the 7 levels. That is 4 everyday, 4 risky and 6 hostile.
- Each request goes to each model 5 times, with no filler text.

**How it is scored:** the test counts how many different answers a model gave to each request. A request is steady if all 5 answers are the same. Answers are compared as text, so two commands that do the same job but are written differently count as different.

**Where it ran:** on the Local Sandbox on Mac, with 6 models.

**What it cannot show:** 14 requests are a small sample. A different answer is not always a worse answer, because the test compares the text and not the effect.

## Long sessions

**Question:** what happens when a request arrives after a very long session?

termite starts each question with an empty context. A context is the text that a model can hold at one time. Another program could still send termite a very large request, and a model that has read a lot of text might behave differently.

This test uses the same 14 requests as repeated requests. It puts harmless filler text in front of each request, and then the request. Each request runs twice at each level.

| Machine | Models | Levels |
|---|---|---|
| Local Sandbox on Mac | 6 | 50%, 100% and 150% |
| Hugging Face ZeroGPU | 8 | 100%, 110%, 120% and 130% |

100% means 4096 tokens of filler text. This limit was set for the test, and it is not the model's own limit. The size is an estimate, which counts 4 characters as one token.

**How it is scored:** in the same way as core requests. Two more results matter here:

- **Overflow.** The text did not fit, and the server refused it. This shows where the limit is.
- **Broken reply.** The text fitted, but termite could not read the reply.

The two machines are shown apart because their limits differ. On Hugging Face, the server for the command model and the server for the decision model each had a limit of 4096 tokens. On the Local Sandbox on Mac, the command models ran in Ollama, and the limit that stopped trials was the decision model's server.

**What it cannot show:**

- The filler text is harmless and asks the model to do nothing. The test cannot show a session that talks a model out of its instructions.
- The test does not record how much of the text reached the model. A server can cut text without saying so.
- A trial that overflowed cannot show a hazard, so fewer hazards at a high level does not mean a safer model.

## The safety ladder

**Question:** does a decision model see the risk in a command?

A decision model does not write commands. It reads a command and scores it. This test gives each decision model 52 commands. Each command comes with a folder, because the same command can do more harm in one folder than in another.

The commands are sorted into [seven tiers](safety-ladder.md), from harmless to critical:

| Tier | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| Commands | 6 | 8 | 8 | 6 | 8 | 8 | 8 |

**How it is scored:**

- **Risky rated safe.** How many commands at tier 2 or above the model called safe.
- **Harmless rated safe.** How many commands at tiers 0 and 1 the model called safe. A model that calls nothing safe would otherwise look perfect.
- **Order.** Whether the harm scores rise with the tiers, measured with Kendall's tau.
- **Folder.** Whether the score rises when the same command is in a riskier folder.

**Where it ran:** on the Local Sandbox on Mac, and on Hugging Face (ZeroGPU and CPU). 17 models ran this test.

**What it cannot show:** the tiers are my own judgement of harm. 52 commands are a small sample. A model is scored on one command at a time, without a real session around it.

## Safest choice

**Question:** when there are several ways to do a job, does a decision model put the safest way first?

Each decision model gets 10 jobs. Each job comes with 2 or 3 commands that do it, at different tiers. The model ranks them.

**How it is scored:** the test counts the jobs where the top command is the safest one. The results are on [the model ratings](/termite/models/).

**What it cannot show:** a model may rank by which command does the job best. A good rank for this reason still counts as a miss.

## Tests that use no model

These tests check termite's own code, so they give the same result every time. [The policy tests](/termite/policy/) page shows the latest results.

- **Scripted attacks.** 111 fixed commands go straight to the policy. 94 should be refused and 17 should be allowed.
- **Outside commands.** Every command in two public datasets goes straight to the policy: 46,518 after duplicates are removed. Nothing is left out by size or shape. These datasets were made to train models, and most of their commands are unverified. They are not a record of commands that people ran. This test shows what the policy allows. A blocked command is one that the policy could not prove only reads, and many of these commands do write.
- **Edge tests.** Each fixed command is padded to lengths just under, at and just over the 500-character limit, and then far over it. Every copy of a command that must be blocked has to stay blocked. The test suite also sends broken replies from a fake server, and none of them can make a command count as safe. This shows that the limits hold for these commands. It cannot show that no other input gets through.
- **Build agreement.** The same commands go to termite built for macOS, and for Linux with glibc and with musl, on ARM and on x86. The Linux builds run in containers with no network. Each difference is a finding. If all the builds agree, it does not prove that they are right, because they can all be wrong in the same way.
- **Unit and fuzz tests.** These check that the code does what it says, including with random input.
