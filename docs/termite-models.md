# termite's own models

termite works with any chat model and any decision model that you can serve locally. I also trained three small models for it, so read a model's card before you use it. The third is an experiment that failed its tests, and termite does not use it.

| Model | Job | Size | Card |
|---|---|---|---|
| termite-command-4b 0.2 | Writes commands, and refuses hostile requests | 4.28 GB | [Safety and score card](https://huggingface.co/tanepiper/termite-command-4b) |
| termite-decide-0.8b 0.2 | Judges and ranks commands | 812 MB | [Safety and score card](https://huggingface.co/tanepiper/termite-decide-0.8b) |
| termite-classifier 0.1 (experimental) | Judges commands with no server. Not used by termite | 129 MB | [Safety and score card](https://huggingface.co/tanepiper/termite-classifier) |

**A model does not replace termite's rules.** Every command still goes through them, and a decision model can never approve what the rules refuse.

## How they scored

**termite-command-4b** wrote a hazard for 1 of 100 hostile requests. Its base model, Qwen3 4B 2507, wrote one for 33 of the 95 it was tested on. It answered 38 of 40 everyday requests usefully, and it refused all 30 harmful requests from an outside set. It refuses more fair requests than version 0.1: it answered 51 of 60 outside everyday requests, down from 58.

**termite-decide-0.8b** rated 1 command at tier 3 or above as safe on [the safety ladder](safety-ladder.md), and termite's rules refuse that command. Its tau is 0.678, below its mark of 0.703 and below version 0.1's 0.698. It rated 46 of 68 harmless reads as safe, up from 36, and 5 of 48 harmful commands as safe, the same as version 0.1.

**termite-classifier** missed all three of its marks. It rated 1 command at tier 3 or above as safe, its tau is 0.554 against a mark of 0.698, and it rated 13 of 14 harmless commands as safe. Don't rely on it for safety.

Version 0.2 of both models is trained from clean sources, and some of its scores are worse than 0.1. The cards say why, and what the next version will try. They also list the known limits. I used the main test to choose between training runs, so these scores are likely to flatter the models.

The scores are also in [the model ratings](/termite/models/). [What changed in each release](/termite/models/termite/) compares each version with the one before it.

## The classifier experiment

The decision model needs a server. The goal of the classifier is to answer the same questions inside `tmite`, on any machine, in milliseconds. It isn't there yet.

| Run | What changed | Tau | Result |
|---|---|---|---|
| 1 | 10M parameters, trained only on labels from termite's rules | 0.37 | It didn't know what most programs do |
| 2 | Learned from termite-decide's answers on about 54,000 commands | 0.538 | The biggest gain |
| 3 | Corrected wrong labels and removed rows that weren't real commands | 0.383 | Lower, and each run happened once, so the cause isn't known |
| 4 | The same data at 30M parameters | 0.554 | The best run, but it still missed its marks |

A small model that learns from a larger one copies its mistakes, and it is unlikely to beat it. The next step is to learn from termite-decide 0.2, which is trained on clean sources. `tmite` won't load the classifier unless it passes its tests.

[What changed in each release](/termite/models/termite/) shows the full results for each run.

## Use them

Start each server in its own terminal:

```bash
llama-server -hf tanepiper/termite-command-4b -hff termite-command-4b-0.2-Q8_0.gguf --jinja -c 8192 --port 8080
```

```bash
llama-server -hf tanepiper/termite-decide-0.8b -hff termite-decide-0.8b-0.2-Q8_0.gguf -c 4096 --port 8081
```

Then run `tmite setup`. It finds both servers, asks you to pick a model for each job, and writes your config.

To train your own, follow [Train your own models](learning-to-train.md).
