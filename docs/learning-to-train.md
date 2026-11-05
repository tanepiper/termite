# Learning to train a model

I have never trained a model. This page records the path I am taking, and it assumes that you have not trained one either. The goal is a small model that is better than Kev or tev at rating how dangerous a command is in a given folder. The test already exists, so the result can be measured.

## Lesson 1: what training is

A model is a large set of numbers. To train it, you show it an example, measure how wrong its answer was (this number is the **loss**), and adjust the numbers so that the answer is slightly less wrong. You repeat this a few thousand times.

**Open weights** means that someone published the numbers, so you can download them and continue the training. You almost never start from nothing, because training from nothing costs millions. Starting from a model that already knows English and shell, and teaching it one more thing, is called **fine-tuning**, and it runs on a laptop.

## Lesson 2: do not change all the numbers

Fine-tuning every number needs a lot of memory, and it often damages what the model already knew. **LoRA** freezes the model and trains a small addition, usually under 1% of the model's size. The result is a file of a few megabytes (an **adapter**) that you can keep, compare and delete.

On a Mac, Apple's **MLX** supports this well, and 96GB of memory is enough for anything under about 8 billion parameters.

## Lesson 3: the data is most of the work

Training is one command, and good labels are the real work. The rows look like this:

```json
{"folder": "~/.ssh", "command": "ls", "tier": 4}
```

The ladder has about 50 of these rows, but it is a test set, so **it cannot be used for training**. A model trained on its own test scores well and learns nothing. For this reason, the data is split three ways:

- **Train:** the model sees these rows.
- **Validation:** the model does not see these rows, and you watch this score to decide when to stop.
- **Test:** the ladder. Use it only at the end.

Split by command, not by row. If `ls` in `~/.ssh` is in training and `ls` in `~/.aws` is in the test, the test no longer measures anything new.

I need about two thousand rows. The 45,000 outside commands are the starting material, combined with a list of folders. Rules label the easy rows, a large model suggests labels for the rest, and I check a sample by hand. If the labels are wrong, all the later work is wasted.

## Lesson 4: when to stop

Watch two numbers: training loss and validation loss. It is good when both fall. When training loss continues to fall and validation loss starts to rise, the model has stopped learning the idea and has started memorising your rows. This is called **overfitting**, and it is the point to stop.

## Lesson 5: the first run

```bash
uv run --with mlx-lm mlx_lm.lora --model Qwen/Qwen3-1.7B --train \
  --data training/ --iters 600 --batch-size 4 --adapter-path adapters/tier-v1
```

`training/` holds `train.jsonl` and `valid.jsonl`. Then merge the adapter, convert it
to GGUF, serve it, add it to `eval/decision-models.jsonl`, and run `eval decision`.
The last step decides the result: the new model must beat Kev's tau and also beat it on the folder pairs. If it does not, the training did not work, whatever the loss numbers show.

The model name above is a starting point, so check which models are current before you run it. Choose the smallest model that can do the job, because a 1.7B model that can check every command is more useful than a 27B model that cannot.

## What this will not do

A trained model does not replace the policy. It will sometimes be wrong, and a command that deceives it can still cause harm. Like every other model here, it can only make termite stricter.

## Current status

Lessons 1 to 4 need only reading. Lesson 5 needs the labelled data, which is the next step.
The older design notes are in [training tiny models](training-tiny-models.md).
