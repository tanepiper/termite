# Training our own tiny shell models

Status: draft, for discussion. Nothing here is built yet.

## Why

termite works today with general models: Qwen3.8-27B writes the commands
and Kev-4B ranks them and judges safety. That works, but it is heavy for a
one-line answer:

- The 27B chat model runs at about 17 tokens/s on this machine and needs
  ~28 GB. A query takes 10–40 s, mostly in model loading and swapping.
- With reasoning on, the same model spends ~70 s thinking before writing
  `du -sh .`. (We now turn reasoning off, which gives the same answer in
  9 s.)
- General models are weak exactly where shells are tricky: BSD and GNU
  flags that differ (`sed -i ''`, `stat -f` vs `stat -c`, `date -v`),
  zsh vs bash quoting, macOS-only tools (`launchctl`, `defaults`, `pbcopy`,
  `diskutil`, `mdfind`), and which of five ways is the idiomatic one.

The goal is two small specialists, each under ~2B parameters, that answer
in under a second on a laptop and know shells better than a general 27B
model.

## The two models

| | Command model | Decision model |
|---|---|---|
| Job | request + context → 1–5 candidate commands as JSON | rank candidates; answer `only_reads`, `no_secrets`, `harmful` |
| Served by | any OpenAI-compatible server (`/v1/chat/completions`) | llama.cpp or Ollama `/v1/systemone` |
| Output | constrained by the JSON schema in `src/generate.rs` | probabilities over fixed options |
| Size target | 0.6–1.7B | 0.5–1B (Kev-4B is the bar to beat) |
| Failure that matters | a wrong but plausible command | calling an unsafe command safe |

The command model is free to be wrong sometimes, because the user confirms
or picks. The decision model must almost never be wrong in one direction:
an unsafe command rated safe under `?x` runs without a prompt. (The local
`read_only` policy runs first and is the real boundary, so the decision
model can only make things stricter. A bad decision model still makes `?x`
useless if it is too cautious, or risky if the policy has a gap.)

## Data

### Sources

1. **tldr-pages** (CC BY 4.0): short, idiomatic examples for thousands of
   commands, already split by platform (`common`, `linux`, `osx`). The best
   single seed.
2. **Man pages** from the actual target systems: macOS (BSD userland) and a
   few Linux distributions (GNU coreutils, busybox). Used to check that
   flags exist, not as training text.
3. **NL2Bash** and similar public pairs: useful for volume, but old,
   Linux-only and noisy. Filter hard.
4. **Synthetic pairs**: a large model writes requests in many phrasings
   ("free up space", "what's eating my disk", "du but sorted") for each
   seed command, plus near-misses and unsafe variants.
5. **termite's own use (opt-in only, later)**: which candidate the user
   picked in `??`, and which `?` suggestions they edited or rejected. That
   is real preference data. It must never be collected without an explicit
   switch, must be scrubbed of paths, hostnames and secrets, and must stay
   on the machine unless the user exports it.

### Verify by running, in a sandbox

Each generated command should be executed before it becomes training data:

- Linux: a throwaway container per platform (Debian, Alpine/busybox, Arch),
  with a seeded fake home folder, git repo, docker socket stub and so on.
- macOS: a throwaway macOS VM (Tart or similar) with the same fixture.
- Record exit status, stdout/stderr, and a **filesystem diff** before and
  after, plus whether it tried the network.

That gives labels for free:

| Label | From |
|---|---|
| works | exit 0 and output that matches the request (judged by a large model) |
| `only_reads` | empty filesystem diff, no process or service changes |
| `no_secrets` | output does not match secret patterns from the fixture (planted keys and tokens) |
| `harmful` | diff deletes or overwrites fixture data, or touches outside the folder |
| platform | which sandboxes it worked in: `common`, `linux`, `osx` |

The planted-secret fixture matters: `cat ~/.ssh/id_ed25519` only reads, but
Kev-4B rated it 0.08 on `no_secrets` because it knows the path. A trained
model should learn the same from real effects, not just from names.

### Balance

- Over-sample BSD/GNU differences and zsh/bash differences.
- For the decision model, keep many **hard negatives**: commands that look
  read-only but are not (`find … -delete`, `git checkout .`,
  `sed -i`, `tee`, `xargs rm`, `curl … | sh`, `docker system prune`).
- Keep the policy (`src/policy`) in the loop: anything the policy refuses
  should never be labelled safe, whatever the sandbox saw.

## Training

1. **Base models** to compare: Qwen3 0.6B / 1.7B, Gemma small, SmolLM2-1.7B.
   Pick by size of the GGUF at Q8 and by tokenizer handling of shell
   punctuation.
2. **Supervised fine-tuning** (LoRA first, full fine-tune if it pays off) on
   verified pairs, in exactly the prompt format termite sends: the system
   prompt from `src/generate.rs` with OS, shell and folder, and the JSON
   schema output.
3. **Distillation** from the large model for descriptions and for ranking
   between several working commands.
4. **Preference tuning** (DPO) from the `??` picks once there is opt-in
   data: the chosen command beats the others.
5. For the decision model: train it as a classifier on the three questions
   and the ranking question. Check how llama.cpp's `/v1/systemone` scores a
   model (it reads probabilities over fixed options) and how a model is
   marked as a decision model (the router lists it with a `decision` tag;
   Ollama reports a `decision` capability). **Open question:** what format
   Kev-4B and tev1 were trained on, and whether we can fine-tune from Kev
   instead of from scratch.
6. Export to **GGUF** (Q8_0 and Q4_K_M), publish on Hugging Face, and add
   the repos to `src/bin/tmite/setup/hub.rs` so `tmite setup` offers them.

## Evaluation

A fixed held-out set, never trained on, run against every candidate model
and against today's pair (Qwen3.8-27B + Kev-4B) as the baseline:

- **Command model:** share of requests where a top-1 command works in the
  sandbox (by platform); share where any of the top 5 works; share of
  commands `check_command` rejects (multi-line, hidden characters).
- **Decision model:** the calibration table in `src/decide.rs` tests,
  extended to a few hundred rows. The gate metric is **unsafe rated safe
  at the current thresholds: must be zero.** Second metric: share of
  read-only commands passed (how useful `?x` is).
- **Speed:** time to first answer on an M-series laptop with both models
  loaded at once. Target: under 1 s for `?`, under 2 s for `?x`.

## First steps

1. Turn the decision calibration rows into a JSON eval file and a tiny
   runner, so any model can be scored with one command.
2. Build the Linux sandbox and the fixture home folder; run tldr `common`
   and `linux` examples through it to get the first verified set.
3. Measure the baseline pair on that set.
4. Try a LoRA on Qwen3-0.6B for the command model and compare.

## Things to keep true

- Training data never includes a user's real history unless they opted in,
  and never leaves their machine without an export they started.
- The policy stays the boundary. A trained decision model replaces Kev-4B;
  it does not replace `read_only`.
- Models are published with their data sources and eval results.
