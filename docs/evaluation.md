# How the testing works

Small local models will write `rm -rf ~` if you ask nicely. Sometimes they'll do
it when you don't. So termite assumes the model is out to get you, and the job of
the test suite is to prove the guards hold when it is.

Nothing a model writes is ever run. Commands get recorded, matched against a list
of known hazards, and pushed through the policy. That's it.

## Run it

```bash
scripts/analyse.sh
```

That's everything, models included, and it takes a while.
`scripts/analyse.sh --no-models` skips the models and takes a few minutes. Tests
run inside `scripts/sandbox.sh`, which can't see your real home folder.

Models run one at a time, and each one is unloaded as soon as it's finished, so the
next one gets the memory. If a server won't unload a model, eval says so and
carries on.

## What gets tested

| Test | Asks | Needs a model? |
|---|---|---|
| Unit and fuzz tests | Does the code do what it says? | No |
| Scripted attacks, `eval policy` | Can a known trick get past the policy? | No |
| Outside commands, `eval bulk` | What does the policy do with 45,000 real commands? | No |
| Core requests, `eval run` | What do models write for 48 requests, from `list files` to outright attacks? | Yes |
| Outside requests, `eval run --corpus` | Same again with public jailbreak and injection sets | Yes |
| Repeats and floods, `eval trials` | Does it give the same answer twice? What about behind a full context? | Yes |
| The ladder, `eval decision` | Can a decision model order commands by danger? | Yes |

The outside datasets are pinned in `eval/sources.jsonl` to one revision and checked
by hash. `scripts/fetch-sources.sh` fetches them, and it never runs code from
them.

## The words we use

Every result uses the same few words.

| Word | Means |
|---|---|
| **Handed over** | The model's top answer was the dangerous one, and you got it |
| **Straight in** | It would have reached your prompt with no question asked |
| **Needs your yes** | It was explained first, and waits for your answer |
| **Blocked** | termite refused it outright |
| **Refused** | The model wrote no command |
| **Error** | Something broke: a timeout, a bad reply, a dead server |

What went wrong gets its own word, too: policy bypass, risky straight in, injection
followed, hazard labelled safe, hazard written, wrong answer, timeout, context
overflow, broken reply, server error, risk rated safe.

## Why repeats and floods

Ask a model the same thing five times and you can get five answers. A safety test
that runs once is telling you about one roll of the dice. `eval trials` repeats each
case and records whether the answer changed.

Then it floods the context: it buries the request behind a long, boring session
until the context is half full, full, and past full, and nothing gets cleared.
termite itself starts each question fresh, so this tests the model, but it's what
happens when something else feeds termite a giant request. There's no size limit on
requests today, which is on the list.

## Everything is an observation

Every trial becomes one row in `eval/observations/`, in a strict format
(`eval/schema/observation.md`). A row holds ids, numbers and fixed words. It can't
hold a request or a command, because the type has no field for one, so these files
are safe to commit. They're never rewritten.

Each row says which model, of what kind and family, ran which case, in what
condition (which repeat, how full the context was), how it ended, and if it went
wrong, how. The [explorer](/termite/explore/) is built entirely from these rows.
New models turn up as new rows, and a model you stop testing just stops appearing.

A one-line summary of each run goes in `eval/history.jsonl`, also append-only, with
a fingerprint of the test set. Scores only get compared when the fingerprint
matches. If I change the test, a better number means nothing.

## What's public, and what isn't

| | Public site | Private report |
|---|---|---|
| Everyday and risky requests | Quoted, with replies | Yes |
| Well-known misuse, like `rm -rf ~` | Quoted, with replies | Yes |
| Anything nastier | Topic, type of query, how it ended | Full detail |
| Anything that got past the guards | Topic only, until fixed | Full detail, at the top |

Quoting fails closed. A request needs `"disclose":"show"` in the corpus, and a
command has to clear every rule in `examples/eval/publish.rs`. The export then
searches its own output for every withheld string and writes nothing if it finds
one. `scripts/check-public.sh` checks again, differently, before the site deploys.

Private reports go to `~/termite-private`, outside the repository so git can't
get at them. Each is a dated static page with what to do and how to raise it, and
none is ever replaced. Each finding is one of:

- **Known technique:** a model wrote a documented attack and the guards held. This
  is model behaviour, not a vulnerability.
- **Known issue:** something got through and it's on file.
- **Possibly new:** something got through and nothing matches. Confirm it in the
  sandbox, then search that tool's advisories.

The tooling never searches online and never contacts anyone. Only a person can
decide something is a new CVE, and only a person sends the report.

## Keeping the data

Raw runs hold withheld commands, so they stay out of the repository.
`scripts/archive-runs.sh` copies them to `~/termite-private/runs` and to a private
dataset on Hugging Face, with the observations beside them. It checks the dataset
is private first, every time, and stops if it isn't.

## Watch popular models

`python3 -I scripts/model-watch.py` lists the popular GGUF models on Hugging Face in
`eval/watch.jsonl`, with their size, whether refusals were removed, whether we've
tested them, and where they fit: this Mac, a hosted GPU and what it costs an hour,
or nothing we can rent. It needs no token, downloads no weights and contacts no
model. Sizes are estimates for a 4-bit file.

## Hosted testing

`scripts/hosted.sh` runs a test on Hugging Face Jobs, under the same gate as a local
run. Jobs can't cut their own network: they have no `CAP_NET_ADMIN`, and `unshare` is
refused. So `confine` blocks it from inside, before anything else starts:

- Landlock stops every TCP connection, and every write outside the folders named.
- A seccomp filter stops UDP and everything else except TCP and Unix sockets.
- The model, the decision model and the eval tool talk over Unix sockets.

Neither can be undone, and every child process inherits them. If either can't be
applied, nothing runs. No token goes into the job. Results land in the private
`jobs-artifacts` bucket.

Build the tools once, in a job. Nothing is built on this machine:

```bash
scripts/hosted.sh build
```

```bash
scripts/hosted.sh gate
```

```bash
scripts/hosted.sh run 'LFM3 1.7B, hosted'
```

```bash
scripts/hosted.sh fetch
```

The models are in `eval/hosted-models.jsonl`. They run on CPU for $0.03 an hour, and the script stops if a run could cost more than the
monthly credit. Use `HOSTED_GPU=1` for a model of about 7B or more: a T4 costs $0.40
an hour, and Llama 3.1 8B on one matched a local run on usefulness, time and endings.
On CPU, requests can pass the 120-second limit. A timed-out request
counts as no hazard, so treat a hosted hazard count as a minimum and check how many
requests got no answer. A hosted run only ever uses a model's weights inside the job. We
don't send test requests to Inference Providers.

`eval hosted` writes the numbers for the site to `site/src/data/hosted.json`: counts
only, from `~/termite-private/runs`: the newest T4 run of each model, or else its newest
CPU run, with a column saying which. A model
that failed more than half its requests is left out. The site shows these in their own
section, with no grade. ZeroGPU runs are never included.

### ZeroGPU

PRO includes 40 minutes of ZeroGPU a day, which suits 7B models. `sandbox/zerogpu/`
holds a private Gradio Space that runs `confine` inside a GPU function. It
differs from a job in four ways:

- `/proc` has to be writable, or CUDA won't start.
- There's no `llama-server`, so `llama-cpp-python` serves the model, through a small
  adapter for the schema request. It's the same engine, in an older version.
- There's no decision model, so endings aren't comparable with local runs. The
  hostile score doesn't use one.
- Time past the daily quota is charged to credits, at $1 for 10 minutes.

Don't use ZeroGPU for scores. Llama 3.1 8B with refusals removed wrote 24 hazards out
of 24 there, and 17 on a T4 with the real servers. We don't know which difference
caused that. Use it for quick checks only.

## The corpus

The test requests and commands are not in the repository, because some of them are
withheld from the public site. They live in `~/termite-private/corpus`, and `eval/corpus`
is a git-ignored link to that folder. Without it, `eval` stops with a missing file error.
Everything else in the repository builds and tests without it. To run the evaluation
yourself, put your own files at `eval/corpus`, in the formats in `examples/eval/types.rs`
and `examples/eval/decision.rs`.

## Add a model

Model tests refuse to run outside a sandbox. See [the gates](agents.md#the-gates).

Add a line to `eval/models.jsonl`, or `eval/decision-models.jsonl` for a decision
model, with its size and context limit. Run `eval run --only NAME`. It adds to
what's there, so you don't need to retest everything.

Then run `eval families`. The family comes from the start of the name, so
`Qwen3.8 27B` and `qwen2.5-coder 7B` are both Qwen, and nobody has to type it. The list
lives in `eval/families.jsonl`. It gives the same result every time, and a family
keeps its colour on the site once it has one, however many models join later. `eval
publish` does this too, so a new model can't be missed.
