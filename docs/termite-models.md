# termite's own models

termite works with any chat model and any decision model that you can serve locally. I also trained two small models for it. Version 0.1 of each is a baseline, so read its card before you use it.

| Model | Job | Size | Card |
|---|---|---|---|
| termite-command-4b 0.1 | Writes commands, and refuses hostile requests | 4.28 GB | [Safety and score card](https://huggingface.co/tanepiper/termite-command-4b) |
| termite-decide-0.8b 0.1 | Judges and ranks commands | 812 MB | [Safety and score card](https://huggingface.co/tanepiper/termite-decide-0.8b) |

**A model does not replace termite's rules.** Every command still goes through them, and a decision model can never approve what the rules refuse.

## How they scored

**termite-command-4b** wrote a hazard for 3 of 100 hostile requests. Its base model, Qwen3 4B 2507, wrote one for 33 of the 95 it was tested on. It answered 40 of 40 everyday requests usefully, and it refused all 30 harmful requests from an outside set. It usually gives only one command, so it missed its mark for offering alternatives.

**termite-decide-0.8b** rated no command at tier 3 or above as safe on [the safety ladder](safety-ladder.md). Its tau is 0.698, which is 0.005 below its mark of 0.703 and close to Kev-4B, a model five times its size. It missed 20 of 307 harmful commands in held-out tests.

Both cards list the known limits. I used the main test to choose between training runs, so these scores are likely to flatter the models.

The scores are also in [the model ratings](/termite/models/). [What changed in each release](/termite/models/termite/) compares each version with the one before it.

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
