# Training small shell models

Status: draft plan. None of this work is built yet.

## Why

termite works with general models today: Qwen3.8-27B writes the commands, and Kev-4B ranks them and judges their safety. This pair is large for a one-line answer:

- The 27B chat model runs at about 17 tokens a second and needs about 28 GB of memory. A request takes 10 to 40 seconds, and most of that time goes on loading and swapping models.
- With reasoning on, the same model spends about 70 seconds before it writes `du -sh .`. termite now turns reasoning off, which gives the same answer in 9 seconds.
- General models are less accurate on the details of each shell: BSD and GNU flags that differ (`sed -i ''`, `stat -f` and `stat -c`, `date -v`), zsh and bash quoting, macOS tools (`launchctl`, `defaults`, `pbcopy`, `diskutil`, `mdfind`), and which of several methods is the usual one.

The goal is two small specialist models, each under about 2B parameters, that answer in under a second on a laptop and know shells better than a general 27B model.

## The two models

| | Command model | Decision model |
|---|---|---|
| Job | Takes a request and context, and returns 1 to 5 candidate commands as JSON | Ranks candidates, and answers `only_reads`, `no_secrets` and `harmful` |
| Served by | Any OpenAI-compatible server (`/v1/chat/completions`) | llama.cpp or Ollama (`/v1/systemone`) |
| Output | Constrained by the JSON schema in `src/generate.rs` | Probabilities over fixed options |
| Target size | 0.6B to 1.7B | 0.5B to 1B, and it must score better than Kev-4B |
| Most serious failure | A wrong command that looks correct | Rating an unsafe command as safe |

The command model can sometimes be wrong, because the user approves or chooses each command. The decision model must almost never rate an unsafe command as safe, because `?x` could then inject it with no question. The local `read_only` policy runs first and is the real boundary, so the decision model can only make an answer stricter. A poor decision model still makes `?x` less useful if it is too cautious, and riskier if the policy has a gap.

## Data

### Sources

1. **tldr-pages** (CC BY 4.0): short, typical examples for thousands of commands, already split by platform (`common`, `linux`, `osx`). This is the best single starting point.
2. **Man pages** from the target systems: macOS (BSD tools) and a few Linux distributions (GNU coreutils, busybox). These confirm that flags exist, and they are not training text.
3. **NL2Bash** and similar public pairs: these add volume, but they are old, Linux-only and inconsistent, so they need strict filtering.
4. **Generated pairs**: a large model writes requests in many phrasings ("free up space", "what is using my disk", "du but sorted") for each starting command, and it also writes similar commands that are wrong or unsafe.
5. **termite's own use (opt-in only, later)**: which candidate the user chose in `??`, and which `?` suggestions they edited or rejected. This data must never be collected without an explicit setting. It must have paths, hostnames and secrets removed, and it must stay on the machine unless the user exports it.

### Verify each command in a sandbox

Each generated command should run in a throwaway sandbox before it becomes training data. It must never run on the host.

- **Linux:** one throwaway container for each platform (Debian, Alpine with busybox, Arch), with a prepared fake home folder, git repository, docker socket stub and similar test files.
- **macOS:** a throwaway macOS virtual machine (Tart or similar) with the same test files.
- **Record:** the exit status, the output, a **filesystem diff** from before and after, and whether the command tried to use the network.

These records provide the labels:

| Label | Source |
|---|---|
| works | Exit status 0, and a large model judges that the output matches the request |
| `only_reads` | An empty filesystem diff, and no process or service changes |
| `no_secrets` | The output does not match the secret patterns in the test files, which include planted keys and tokens |
| `harmful` | The diff deletes or overwrites test data, or changes anything outside the folder |
| platform | The sandboxes where the command worked: `common`, `linux`, `osx` |

The planted secrets are important. `cat ~/.ssh/id_ed25519` only reads, but Kev-4B rated it 0.08 on `no_secrets` because it recognises the path. A trained model should learn this from real effects as well as from names.

### Balance

- Include extra examples of the differences between BSD and GNU, and between zsh and bash.
- For the decision model, include many **hard negatives**: commands that look read-only but are not (`find … -delete`, `git checkout .`, `sed -i`, `tee`, `xargs rm`, `curl … | sh`, `docker system prune`).
- Apply the policy (`src/policy`) to the data: a command that the policy blocks must never be labelled safe, whatever the sandbox recorded.

## Training

1. **Base models to compare:** Qwen3 0.6B and 1.7B, a small Gemma, and SmolLM2-1.7B. Choose by the size of the GGUF file at Q8, and by how the tokenizer handles shell punctuation.
2. **Supervised fine-tuning** on verified pairs, in the exact prompt format that termite sends: the system prompt from `src/generate.rs` with the OS, shell and folder, and the JSON schema output. Start with LoRA, and use a full fine-tune only if it gives better results.
3. **Distillation** from the large model, for descriptions and for ranking several working commands.
4. **Preference tuning** (DPO) from `??` choices, when opt-in data exists: the chosen command ranks above the others.
5. **Decision model:** train it as a classifier on the three questions and the ranking question. Check how llama.cpp's `/v1/systemone` scores a model (it reads probabilities over fixed options) and how a model is marked as a decision model (the router lists it with a `decision` tag, and Ollama reports a `decision` capability). **Open question:** which format Kev-4B and tev1 were trained on, and whether fine-tuning from Kev is possible.
6. **Export** to GGUF (Q8_0 and Q4_K_M), publish on Hugging Face, and add the repository, file, revision and hash to `src/bin/tmite/setup/catalog.json` so that `tmite setup` offers them.

## Evaluation

Use a fixed test set that is never used for training. Run it against each candidate model, and against the current pair (Qwen3.8-27B and Kev-4B) as the baseline.

- **Command model:** the share of requests where the top command works in the sandbox, for each platform; the share where any of the top 5 works; and the share of commands that `check_command` rejects (multi-line commands and hidden characters).
- **Decision model:** the calibration table in the `src/decide.rs` tests, extended to a few hundred rows. The main measure is **unsafe commands rated safe at the current thresholds, and this must be zero.** The second measure is the share of read-only commands that pass, which shows how useful `?x` is.
- **Speed:** the time to the first answer on an M-series laptop with both models loaded. The target is under 1 second for `?`, and under 2 seconds for `?x`.

## First steps

1. Turn the decision calibration rows into a JSON evaluation file and a small runner, so that one command can score any model.
2. Build the Linux sandbox and the test home folder, then run the tldr `common` and `linux` examples through it to get the first verified set.
3. Measure the baseline pair on that set.
4. Try a LoRA on Qwen3-0.6B for the command model, and compare the results.

## Rules for this work

- Training data never includes a user's real history unless they opted in, and it never leaves their machine without an export that they started.
- The policy stays the boundary. A trained decision model replaces Kev-4B, and it does not replace `read_only`.
- Each published model includes its data sources and evaluation results.
