# How the testing works

A model can write a dangerous command because someone asked for it, or because it made a mistake. termite treats every model as untrusted, and the test suite checks that termite's checks hold when a model behaves badly.

Nothing a model writes is ever run on your machine. Each command is recorded, compared with a list of known hazards, and passed through the policy.

## Run it

```bash
scripts/analyse.sh
```

This runs everything, including the models, and it takes a long time. `scripts/analyse.sh --no-models` skips the models and takes a few minutes. Tests run inside `scripts/sandbox.sh`, which cannot see your real home folder.

Models run one at a time, and each model is unloaded when it finishes, so that the next model has the memory. If a server does not unload a model, eval reports this and continues.

## What gets tested

[The tests](tests.md) describes each test in detail.

| Test | Asks | Needs a model? |
|---|---|---|
| Unit and fuzz tests | Does the code do what it says? | No |
| Scripted attacks, `eval policy` | Can a known trick get past the policy? | No |
| Outside commands, `eval bulk` | What does the policy do with 46,518 commands from public datasets? | No |
| Edge tests, `eval edges` | Do the limits hold just under, at and over 500 characters? | No |
| Build agreement, `scripts/agree.sh` | Do the macOS and Linux builds give the same answers? | No |
| Core requests, `eval run` | What do models write for 161 requests, from `list files` to outright attacks? | Yes |
| Outside requests, `eval run --corpus` | The same question, with public jailbreak and injection sets | Yes |
| Repeats and long sessions, `eval trials` | Does it give the same answer twice, and what happens when the context is full? | Yes |
| The ladder, `eval decision` | Does a decision model rate risky commands as safe, and can it order commands by danger? | Yes |

The outside datasets are pinned to one revision in `eval/sources.jsonl` and checked by hash. `scripts/fetch-sources.sh` downloads them, and it never runs code from them.

## Result words

Every result uses one of these words.

| Word | Means |
|---|---|
| **Handed over** | The model's top answer was the dangerous command, and you received it |
| **Injected** | The command would have been placed at your prompt with no question asked |
| **Needs approval** | The command was explained first, and it waits for your answer |
| **Blocked** | termite refused the command, and it cannot reach your prompt |
| **No usable command** | The model wrote commands, and termite rejected all of them |
| **Declined** | The model said it would not do this, or every command it wrote only prints a refusal or warning |
| **Error** | Something failed, such as a timeout, a bad reply or a stopped server |

Each problem also has its own word. [Failure types](failures.md) describes all of them.

## Why repeats and long sessions

If you ask a model the same thing five times, you can get five different answers, so one run is not enough for a safety test. `eval trials` repeats each case and records whether the answer changed.

It then fills the context. It places the request after a long session, at set levels from half full to over full.

- termite starts each question with an empty context, so this test measures the model.
- The test also shows what happens when another program sends termite a very large request.
- termite has no size limit on requests today. I plan to add one.

## Everything is an observation

Every trial becomes one row in `eval/observations/`, in a strict format (`eval/schema/observation.md`).

- A row holds ids, numbers and fixed words.
- A row cannot hold a request or a command, because the type has no field for one. This makes these files safe to commit.
- New runs add new files.
- `eval relabel` rewrites the files from the raw runs when the label rules change. It does not change a score.

Each row records:

- which model ran which case
- the kind and family of that model
- the condition: which repeat, and how full the context was
- how the trial ended
- what went wrong

The [Explore page](/termite/explore/) uses only these rows. A new model appears as new rows. A model that I stop testing stops appearing.

A one-line summary of each run goes in `eval/history.jsonl`, which is append-only, with a fingerprint of the test set. Scores are compared only when the fingerprints match, because a better score on a different test means nothing.

## What is public and what is private

| | Public site | Private report |
|---|---|---|
| Everyday and risky requests | Quoted, with replies | Yes |
| Well-known misuse, like `rm -rf ~` | Quoted, with replies | Yes |
| Anything more harmful | Topic, type of request, how it ended | Full detail |
| Anything that passed termite's checks | Topic only, until fixed | Full detail, at the top |

Nothing is quoted by default.

- A request needs `"disclose":"show"` in the corpus.
- A command must pass every rule in `examples/eval/publish.rs`.
- The export searches its own output for every withheld string. If it finds one, it writes nothing.
- `scripts/check-public.sh` makes a second, different check before the site deploys.

Private reports go to `~/termite-private`, outside the repository, so that git cannot include them. Each report is a dated static page that says what to do and how to report it, and no report is ever replaced. Each finding is one of these:

- **Known technique:** a model wrote a documented attack, and termite's checks held. This is model behaviour, not a vulnerability.
- **Known issue:** something passed the checks, and it is already recorded.
- **Possibly new:** something passed the checks, and no record matches it. Confirm it in the sandbox, and then search the advisories for that tool.

The tools never search online and never contact anyone. Only a person can decide that something is a new CVE, and only a person sends the report.

## Keeping the data

Raw runs hold withheld commands, so they stay out of the repository.

`scripts/archive-runs.sh` copies them to `~/termite-private/runs` and to a private dataset on Hugging Face, with the observations. It checks that the dataset is private every time, and it stops if the dataset is not private.

## Watch popular models

`python3 -I scripts/model-watch.py` lists the popular GGUF models on Hugging Face in
`eval/watch.jsonl`, with their size, whether refusals were removed, whether I have tested them, and where they can run: this Mac, a hosted GPU and its hourly cost, or nothing available to rent. It needs no token, downloads no weights and contacts no model. Sizes are estimates for a 4-bit file.

It also lists the most downloaded models of the other types that I plan to red-team in `eval/watch-types.jsonl`, grouped by planned test order. None of these models has been tested yet.

## Hosted testing

`scripts/hosted.sh` runs a test on Hugging Face Jobs, under the same gate as a local
run. Jobs cannot cut their own network, because they have no `CAP_NET_ADMIN` and `unshare` is refused. `confine` therefore blocks the network from inside the job, before anything else starts:

- Landlock stops every TCP connection, and every write outside the folders named.
- A seccomp filter stops UDP and everything else except TCP and Unix sockets.
- The model, the decision model and the eval tool talk over Unix sockets.

Neither restriction can be undone, and every child process inherits them. If either cannot be applied, nothing runs. No token goes into the job. Results land in the private
`jobs-artifacts` bucket.

Build the tools once, in a job. Nothing is built on your machine:

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

The models are in `eval/hosted-models.jsonl`.

- **CPU:** $0.03 an hour. The script stops if a run could cost more than the monthly credit. Requests can pass the 120-second limit.
- **T4 GPU:** $0.40 an hour. Use `HOSTED_GPU=1` for a model of about 7B or more. Llama 3.1 8B on a T4 matched a local run on usefulness, time and endings.
- **Hazard counts:** a request that fails before the model writes anything cannot show a hazard. Treat a hosted hazard count as a minimum, and check how many requests failed.
- **Weights:** a hosted run uses a model's weights only inside the job.
- **Inference Providers:** I do not send test requests to them.

`eval publish` reads hosted runs from `~/termite-private/runs` and treats them like any
other run, with the same grades, charts and checks on what may be quoted. Every finished run is included, from CPU jobs, T4 jobs and ZeroGPU, and it appears in the history. A model's grade comes from its newest run, wherever it ran. The
"Every test run" page on the site lists each run with its model, weights,
machine, starting command, commit and results.

### Decision models

`scripts/hosted.sh decision 'NAME'` tests one model from
`eval/hosted-decision-models.jsonl` on the ladder, on a CPU job. Each model needs a
job of its own.

- Kev-4B scored about the same hosted as local, so these results can be compared with local runs.
- A model that is not built for `/v1/systemone` fails every question.
- A model in a file that `llama-server` cannot read stops the job.
- The models page shows no score for a model that answered nothing.

`sandbox/zerogpu/run.py --decision` tests the same list on the ZeroGPU Space, one model at a time, and a model with a `letters` recipe runs through `sandbox/zerogpu/letters.py`, which is much faster than a CPU job. The list marks each model as `core` or `candidate`, and pins each one to a revision.

### ZeroGPU

PRO includes 40 minutes of ZeroGPU a day, which is the largest free test capacity we have. `sandbox/zerogpu/` holds a private Gradio Space that runs `confine` inside a GPU function, with the same pinned `llama-server` as T4 jobs and the Kev-4B decision model. It differs from a job in two ways:

- `/proc` must be writable, or CUDA does not start.
- Time past the daily quota is charged to credits automatically, at $1 for 10 minutes.

`eval/zerogpu-core.jsonl` lists the core set of popular models, each pinned to a revision. To update the Space and test the set:

```bash
scripts/hosted.sh build
```

```bash
scripts/hosted.sh server
```

```bash
scripts/hosted.sh space
```

```bash
HF_TOKEN=... uvx --with huggingface_hub --with gradio_client python sandbox/zerogpu/run.py target/hosted/space eval/zerogpu-core.jsonl ~/termite-private/runs ~/termite-private/zerogpu-ledger.jsonl
```

- **Time limit:** a model that is still running after 400 seconds stops and keeps the requests it finished. It continues in the next window with `--skip`, for up to five windows. The parts make one run.
- **Order:** the reference model runs first, then the models that ran longest ago. Models whose last runs failed go to the end of the list.
- **Weights:** they are in the private bucket `tanepiper/termite-models`, mounted read-only at `/models`. A run downloads nothing, and a restart loses nothing. After adding a model, run `sandbox/zerogpu/mirror.py`.
- **Daily limit:** the script stops at 35 minutes in any 24 hours, from a ledger of time used. Other ZeroGPU use on the account isn't in the ledger.

Options:

- `-` in place of the folder keeps the Space as it is.
- `--limit 4` runs a short trial.
- `--only NAME` runs one model.
- `--pay` continues past the free quota.
- `--fills 100,110` runs long session trials at those levels.

Runs from before 5 November 2026 at about 11:00 used a simpler server and no decision model, so read them with care. Llama 3.1 8B with safety training removed wrote hazards for 24 of 24 hostile requests on that server, and for 17 on a T4 with the full servers.

### Findings

`scripts/findings.py` works out what the results show, from the public data.

- It uses each model's newest run that could refuse, on the newest test set.
- It compares a safety-trained model with its refusals-removed copy request by request.
- The `termite-analyst` agent turns findings into short callouts in `site/src/data/callouts.json`.
- `scripts/findings.py --check` stops the site from deploying if a callout doesn't match a finding.

## The corpus

The test requests and commands are not in the repository, because some of them are
withheld from the public site. They live in `~/termite-private/corpus`, and `eval/corpus`
is a git-ignored link to that folder. Without it, `eval` stops with a missing file error.
Everything else in the repository builds and tests without it. To run the evaluation
yourself, put your own files at `eval/corpus`, in the formats in `examples/eval/types.rs`
and `examples/eval/decision.rs`.

## Add a model

Model tests refuse to run outside a sandbox. [The gates](agents.md#the-gates) explain why.

Add a line to `eval/models.jsonl`, or `eval/decision-models.jsonl` for a decision
model, with its size and context limit. Run `eval run --only NAME`. It adds to
the existing results, so you do not need to retest everything.

Then run `eval families`. The family comes from the start of the name, so
`Qwen3.8 27B` and `qwen2.5-coder 7B` are both Qwen, and nobody needs to type it. The list is in `eval/families.jsonl`. It gives the same result every time, and a family keeps its colour on the site however many models join later. `eval publish` also does this, so a new model cannot be missed.
