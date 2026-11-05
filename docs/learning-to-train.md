# Learning to train a model

I've never trained a model. This is the path I'm taking, written down as I go, so
it assumes you haven't either. The goal: a small model that's better than Kev or
tev at telling how dangerous a command is, in a given folder. The test already
exists, so we'll know whether it worked.

## Lesson 1: what training is

A model is a big pile of numbers. Show it an example, measure how wrong its answer
was (that number is the **loss**), and nudge the numbers so it's a bit less wrong.
Do that a few thousand times. That's training. Everything else is detail.

**Open weights** means someone published the numbers, so you can download them and
carry on nudging. You almost never start from nothing. Training from scratch costs
millions. Starting from a model that already knows English and shell, and teaching
it one extra thing, is called **fine-tuning**, and it runs on a laptop.

## Lesson 2: don't change all the numbers

Fine-tuning every number needs a lot of memory and tends to break what the model
already knew. **LoRA** freezes the model and trains a small add-on, typically under
1% of the size. You get a file of a few megabytes (an **adapter**) that you can
keep, compare and throw away. This is what you want.

On a Mac, Apple's **MLX** does this well, and 96GB of memory is plenty for anything
under about 8 billion parameters.

## Lesson 3: the data is the actual work

Training is a one-line command. Good labels are the job. We want rows like this:

```json
{"folder": "~/.ssh", "command": "ls", "tier": 4}
```

The ladder has about 50 of those, and it's a test set, so **it can't be trained
on**. Train on your exam and you'll score brilliantly and learn nothing. This trips
up everybody at some point, and it's why the data gets split three ways:

- **Train:** the model sees these.
- **Validation:** it doesn't, and you watch the score to know when to stop.
- **Test:** the ladder. Touch it only at the end.

Split by command, not by row. If `ls` in `~/.ssh` is in training and `ls` in
`~/.aws` is in the test, you've leaked the lesson.

We need a couple of thousand rows. The 45,000 outside commands are the raw
material, crossed with a list of folders. Rules label the easy ones, a big model
suggests the rest, and I check a sample by hand. If the labels are wrong, everything
after this is wasted.

## Lesson 4: when to stop

Watch two numbers: training loss and validation loss. Both falling is good. When
training keeps falling and validation starts to rise, the model has stopped learning
the idea and started memorising your rows. That's **overfitting**. Stop there.

## Lesson 5: the first run

```bash
uv run --with mlx-lm mlx_lm.lora --model Qwen/Qwen3-1.7B --train \
  --data training/ --iters 600 --batch-size 4 --adapter-path adapters/tier-v1
```

`training/` holds `train.jsonl` and `valid.jsonl`. Then merge the adapter, convert it
to GGUF, serve it, add it to `eval/decision-models.jsonl`, and run `eval decision`.
That last step is what matters. Beat Kev's tau, and beat it on the folder pairs,
or it didn't work, however nice the loss curve looked.

The model name above is a starting point, so check what's current before you run
it. Pick the smallest one that can do the job. A 1.7B model you can run on every
command is worth more than a 27B one you can't.

## What not to expect

A trained model doesn't replace the policy. It'll be wrong sometimes, and a command
that fools it is still a command. It can only ever make termite stricter, same as
every other model here.

## Where we are

Lessons 1 to 4 are reading. Lesson 5 is waiting on the labelled data, which is next.
The older design notes are in [training tiny models](training-tiny-models.md).
