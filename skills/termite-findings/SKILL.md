---
name: termite-findings
description: Analyse termite's test results and write callouts for the site. Use after new runs are published, or when someone asks which models are safest or what changed. Works from public data only.
---

# Findings: say what the results show

The goal is a few short, true statements that a reader can use. A script does the counting and the statistics, so the same data always gives the same findings. You choose what matters and say it plainly.

## Steps

1. Work out the findings:

   ```bash
   scripts/findings.py
   ```

2. Read `site/src/data/findings.json`. Each finding has a `kind`, a `strength` and the `numbers` behind it.

3. Compare it with `site/src/data/callouts.json`. Replace any callout the data has moved on from, and add what's new. Give each one a `group`. Mark about six as `featured` for the home page, and pick what would change what someone does. Mix them: cover each group, and show what models get wrong, what termite stops, and where caution goes too far. Give each featured callout a `section`: the `id` of the header on the ratings page where the reader can read more.

4. Check your work:

   ```bash
   scripts/findings.py --check
   ```

   It stops if a callout names a finding that no longer exists, if the data doesn't support it, or if the text has a number that isn't in the finding. Fix the callout. Never change the check or the numbers.

5. Tell the person what you changed and why. List anything interesting you left out, and what test would settle it.

## How strong a finding is

| Strength | Means | What you can say |
|---|---|---|
| `strong` | It holds after allowing for how many comparisons were made | State it as a result |
| `suggestive` | It's in the data, but it could be chance | Say "in this run", or leave it out |
| `none` | The difference could easily be chance | No callout. A gap between two totals is not a finding |

For a safety-trained model and its refusals-removed copy, the script compares them request by request. Don't compare the two totals yourself.

## Rules

- Compare like with like. Findings use each model's newest run that could refuse, on the newest test set.
- Each result is one run unless the data says otherwise. Never say a model "is" safe. Say what it did.
- Models whose ranges overlap share a tier. Don't name one winner from a tier.
- Don't explain why a model behaved as it did. The data shows what happened, not why.
- Never quote a request or a command.
- Mistakes in earlier analysis stay out of callouts. Show the current facts.

## Wording

Use plain English that translates well: short sentences and everyday words. A callout has a title that states the finding, and one or two sentences with the numbers. Say what to do with it where that's clear. Write "Injected" and "Needs approval" for those states.
