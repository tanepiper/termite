# Train your own models

This guide shows how to train two small models for a tool that handles shell commands: one that writes commands, and one that judges them. It assumes that you have never trained a model before.

You need a set of rules in code that can rate a command, because the rules do most of the labelling. You also need a set of tests that the model never sees. Everything else can be rented by the hour, and a small model can cost only a few dollars to train.

**A trained model does not replace your rules.** It will sometimes be wrong, and a command written to deceive it can still get past it. Build it so that it can only make the answer stricter. It must never be able to approve something that the rules refuse.

## The words

A model is a large set of numbers. To train it, you show it an example, measure how wrong its answer was, and adjust the numbers so that the answer is slightly less wrong. That measure is the **loss**. You repeat this thousands of times.

**Open weights** means that someone published the numbers, so you can continue the training. Teaching an existing model one more thing is called **fine-tuning**. Training from nothing costs millions, so you almost never do it.

**LoRA** freezes the model and trains a small addition, usually under 1% of its size. The result is a small file, called an **adapter**, that you can keep, compare and delete. Changing every number needs far more memory, and it often damages what the model already knew.

**Overfitting** means that the model has started to memorise your rows and has stopped learning the idea. You see this when the training loss keeps falling and the loss on rows it hasn't seen starts to rise. That is the point to stop.

## Step 1: decide what each model must do

Write this down before you collect any data, and include the score that counts as a pass. If you set the pass mark afterwards, you will move it.

| | Command model | Decision model |
|---|---|---|
| Job | Takes a request and returns commands | Answers fixed questions about a command, and ranks several commands |
| Worst failure | Writes a hazard for a hostile request | Rates an unsafe command as safe |
| Who catches a mistake | The rules, and then the person who reads it | The rules |
| Example pass mark | Writes fewer hazards for hostile requests than the model you use now, and still answers nearly all everyday requests | Puts commands in a better order than the best small model you have tested, and rates no unsafe command as safe |

A decision model has a harder job than it looks. A person reads what a command model writes, but a decision model's answer may let a command skip the question.

## Step 2: choose a base model

- **Read the licence.** Use one that lets you change the model and publish the result, and check the licence of anything you start from.
- **Choose the smallest model that can do the job.** A small model that can check every command is more useful than a large one that can't. A decision model can be under 1 billion parameters.
- **Start from a model that already does a similar job, if one exists.** It gives you the right format and a score to beat.
- **Pin the exact revision,** so that the same run gives you the same model.
- **Use a base model for a decision model, not a chat model.** It picks an option and never chats, so it doesn't need chat training.

## Step 3: protect your test sets

Do this first. A model trained on its own test scores well and has learned nothing.

1. List every test set you have. Include the commands that models wrote in your earlier runs, and the test half of any public dataset.
2. Remove every row whose command or request also appears in a test set. Compare them after you tidy the spaces and case.
3. Split the rest by program, not by row. If `ls` in one folder is in training and `ls` in another folder is in the test, the test measures nothing new.
4. Also hold out whole families of commands, such as one kind of tool. This shows whether the model learned the idea or only the names. Put them back for the last training run.
5. Write a script that checks all of this, and run it after every build of the data.

The data ends up in three groups. The model learns from **train**, you watch **validation** to decide when to stop, and you use the **test** set only at the end.

## Step 4: collect commands

- Public datasets of requests and commands give you volume. Pin each one to a revision, and check its licence.
- Treat downloaded data as hostile. Put it in its own empty folder, keep your scripts somewhere else, and run Python with `-I` so that a planted file can't run.
- Filter hard. Long one-liners hold most of the errors, and a reader can't check them quickly. Remove rows that hold placeholders or only part of a command.
- Keep the data private if you aren't sure of your right to publish it. Never use a person's real shell history unless they chose to share it.

## Step 5: label with rules, not with a model

Run your rules over every command, and let them give one of three answers: yes, no, or can't say. Do this in a fake folder that holds nothing real. Rating a command doesn't run it.

**Never train on "can't say".** Leave those rows out, even if there are thousands of them. A guessed label teaches a guess.

Don't turn one number into a label. If you label every command above a certain level as harmful, the model learns that a command is harmful when it is only hard to check. Mark harm only when the rules give a reason for it, such as a damage warning, and leave the rest unlabelled. A broad warning causes the same fault in any label, so narrow each rule to what really shows the thing you are labelling.

**Then read the labels.** Take a random sample of the rows and read each one, because a script can't find this kind of fault. A score can look fine while the labels underneath are wrong. Fix the rules, build the data again, and read a new sample until very few rows are weak. Keep a list of the rows you reject, so that a rebuild doesn't bring them back.

The best label comes from what a command really does. To get this, run it in a throwaway virtual machine with no network, and compare the disk before and after.

## Step 6: teach the difference

A model learns from contrast. Rows that are all safe or all dangerous teach it very little.

- **Pairs that look alike.** Write a safe command next to a dangerous one that looks almost the same, with the opposite labels.
- **Commands that look harmless and aren't.** Include commands that look like reads but change things.
- **The same command in different places.** A command can be safe in a project and risky in a folder of keys.
- **Hand-written rows for the hard cases.** Write your own rows where public data has too few, such as commands that show secrets.

For the command model, a hostile request gets a refusal as its answer. **A hostile example teaches refusal. It never teaches the hazard.** Never put a harmful command in a training target, whatever the reason.

## Step 7: use the exact format

Train on exactly what the tool sends. Use the same system prompt, the same wording for each question, and the same reply format. Write a check that fails if the wording in the data differs from the wording in the code. If the two drift apart, the scores in training mean nothing in use.

A decision row looks like this. It holds the situation, one question and fixed options:

```text
User request: show what is in this folder
Shell: zsh on macos
Folder: /Users/sam/code/app
Session: local
Command: ls -la

Does this command only read or display information, without creating, changing or deleting anything?
  no: it creates, changes or deletes something
  yes: it only reads or displays
```

The model picks an option and returns a probability, and it writes no text. This makes it fast, and it can't be talked into a long answer. Useful questions about a command are: does it only read, does it keep secrets hidden, and could it do harm. Another question can rank several commands for the same job.

A command row is an ordinary chat: the system prompt, the request, and the exact JSON reply that the tool expects.

## Step 8: train

Rent a GPU by the hour. These settings are a good start for a small decision model with a few thousand to tens of thousands of records:

| Setting | Value |
|---|---|
| Method | LoRA, rank 16 |
| Learning rate | 0.00005, with a short warm-up |
| Passes over the data | 1 |
| Batch size | 8, with gradient checkpointing to save memory |
| Optimiser | AdamW, with a weight decay of 0.01 |

Run a short trial first, with a few hundred rows. It finds the faults that cost the most, such as running out of memory, for a few cents.

Keep the job locked down:

- Put no access token in the job. Mount the data and the base model from private storage, and collect the results from there.
- Pin the versions of every library.
- Don't run code from a model's repository.

## Step 9: measure the failure that matters

Accuracy hides the failure you care about, because most commands are ordinary. Count the dangerous mistakes on their own: how many harmful commands the model called harmless, and how many commands that change something it called read-only.

Score the model before and after, on the same rows. Set its confidence scale on one half of the validation rows, and report the score on the other half, so that the scale can't flatter the result.

Use rows from whole programs that the model never saw, and record the results in a table like this:

| Mistake | Before | After |
|---|---|---|
| Called a harmful command harmless | Count | Count |
| Called a change read-only | Count | Count |
| Called a command that shows secrets safe | Count | Count |
| Top pick matched the best command | Percentage | Percentage |

**This is not the pass mark.** The same rules labelled these rows, so the numbers show that the model learned the rules. They don't show that the rules are right.

## Step 10: test the file you will ship

Convert the model to GGUF, shrink it to Q8_0, and serve it with the same server that your tool uses. Then send the held-out rows again, through the same route that your tool uses. Conversion can change the answers, and a model that scores well in training can fail in the real format.

For a model served through llama.cpp's `/v1/systemone`, the converter needs the adapter, the head and the training settings together. Run the conversion in a job with llama.cpp's own image, and pin that image to a digest before you release.

Count the same mistakes for the converted file as you did in step 9, and compare the two.

## Step 11: read every miss

Read each wrong answer yourself. The total tells you how much is wrong, and the rows tell you why.

Misses usually fall into a few groups. Some come from a family of tools that you held out on purpose, so the model never saw them. That is the test working: put those rows back for the last run. Others come from a label you aren't sure of, because the question and the label don't quite agree. In that case, fix the data or the question. Don't add rows until the score goes up.

## Step 12: run the real test, then publish

Serve the final file, and run the test set that no training step ever touched. Run the same test on the models you want to replace. If the new model doesn't reach your pass mark, the training didn't work, whatever the earlier numbers show.

When it passes:

1. Publish the weights with a safety card. It says what the model is for, what it was trained on, how it scored, and how it fails.
2. Give the base model and its licence.
3. Publish GGUF files, with the revision and hash, so that people can check what they run.
4. Test it again in your tool, with your rules on.
