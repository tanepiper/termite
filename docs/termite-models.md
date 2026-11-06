# termite's own models

termite works with any chat model and any decision model that you can serve locally. I also trained three small models for it. Version 0.1 of each is a baseline, so read its card before you use it. The third is an experiment that failed its tests, and termite does not use it.

| Model | Job | Size | Card |
|---|---|---|---|
| termite-command-4b 0.1 | Writes commands, and refuses hostile requests | 4.28 GB | [Safety and score card](https://huggingface.co/tanepiper/termite-command-4b) |
| termite-decide-0.8b 0.1 | Judges and ranks commands | 812 MB | [Safety and score card](https://huggingface.co/tanepiper/termite-decide-0.8b) |
| termite-classifier 0.1 (experimental) | Judges commands with no server. Not used by termite | 129 MB | [Safety and score card](https://huggingface.co/tanepiper/termite-classifier) |

**A model does not replace termite's rules.** Every command still goes through them, and a decision model can never approve what the rules refuse.

## How they scored

**termite-command-4b** wrote a hazard for 3 of 100 hostile requests. Its base model, Qwen3 4B 2507, wrote one for 33 of the 95 it was tested on. It answered 40 of 40 everyday requests usefully, and it refused all 30 harmful requests from an outside set. It usually gives only one command, so it missed its mark for offering alternatives.

**termite-decide-0.8b** rated no command at tier 3 or above as safe on [the safety ladder](safety-ladder.md). Its tau is 0.698, which is 0.005 below its mark of 0.703 and close to Kev-4B, a model five times its size. It missed 20 of 307 harmful commands in held-out tests.

**termite-classifier** missed all three of its marks. It rated 1 command at tier 3 or above as safe, its tau is 0.554 against a mark of 0.698, and it rated 13 of 14 harmless commands as safe. Don't rely on it for safety.

The cards list the known limits. I used the main test to choose between training runs, so these scores are likely to flatter the models.

The scores are also in [the model ratings](/termite/models/). [What changed in each release](/termite/models/termite/) compares each version with the one before it.

## The classifier experiment

The decision model needs a server. The goal of the classifier is to answer the same questions inside `tmite`, on any machine, in milliseconds. It isn't there yet.

| Run | What changed | Tau | Result |
|---|---|---|---|
| 1 | 10M parameters, trained only on labels from termite's rules | 0.37 | It didn't know what most programs do |
| 2 | Learned from termite-decide's answers on about 54,000 commands | 0.538 | The biggest gain |
| 3 | Corrected wrong labels and removed rows that weren't real commands | 0.383 | Lower, and each run happened once, so the cause isn't known |
| 4 | The same data at 30M parameters | 0.554 | The best run, but it still missed its marks |

A small model that learns from a larger one copies its mistakes, and it is unlikely to beat it. The next step is a better teacher, trained on data with cleaner licences. `tmite` won't load the classifier unless it passes its tests.

[What changed in each release](/termite/models/termite/) shows the full results for each run.

## Use them

Start each server in its own terminal:

```bash
llama-server -hf tanepiper/termite-command-4b:Q8_0 --jinja -c 8192 --port 8080
```

```bash
llama-server -hf tanepiper/termite-decide-0.8b:Q8_0 -c 4096 --port 8081
```

Then run `tmite setup`. It finds both servers, asks you to pick a model for each job, and writes your config.

To train your own, follow [Train your own models](learning-to-train.md).
