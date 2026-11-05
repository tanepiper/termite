# Generator Safety: Do Image, Video and Speech Models Refuse?

Created: 2026-11-05
Author: me@tane.dev
Agent: Claude Code
Status: DRAFT
Approved: No
Worktree: No
Type: Research

## Summary

**Goal:** measure whether image, video and speech generators refuse harmful
prompts, or whether a filter blocks the result, and compare safety-removed
versions with the originals.

This is separate from termite's shell command tests. It needs its own corpus
and scoring, and it is the most expensive test we'd run.

**Blocked until:** the Hugging Face content policy has been read and the owner
has approved this spec. No generator runs before both are done.

## Limits

These apply to every run and have no override.

**Never tested, in any setup:**

- Any sexual content. A safety-removed model can produce sexual images from a
  prompt that didn't ask for them, and sexual images of minors or real people
  are illegal or seriously harmful wherever they are stored. So the category is
  left out entirely, and prompts avoid anything that leads towards it.
- Images, video or voices of real people.
- Any prompt that mentions or depicts children.

The sandbox doesn't change these limits. It stops output leaving the job. It
doesn't change what is legal to create, and jobs run on Hugging Face's machines
under their content policy.

**Categories in scope:**

- weapons
- gore
- hate symbols
- forged documents, such as fake IDs or fake screenshots

## Model types

Text-to-Image, Image-Text-to-Image, Image-to-Image, Text-to-Video,
Image-to-Video, Image-Text-to-Video and Text-to-Speech. Speech tests need their
own categories, because the list above is about images.

## How a test works

1. A prompt from the corpus goes to the generator inside a confined job.
2. The job records whether the model or its filter refused.
3. If an image was made, a classifier scores it inside the job.
4. The image is deleted straight after scoring.
5. Only counts leave the job.

No image, video or audio is stored, uploaded or shown to anyone. The job has no
network, and `--write` covers only a temporary folder and the counts file.

Classifiers come from Image Classification, Zero-Shot Image Classification and
Audio Classification. Their accuracy needs checking on harmless examples first.

## Scoring

Per model and category: prompts refused, blocked by a filter, and generated and
flagged by the classifier. Each count has a stated classifier error rate.

## Running it

- `scripts/hosted.sh` gains a generator mode: a different image and server, the
  same `confine` and gate. No token enters a job.
- Generators need a GPU. Start with the smallest model that fits a T4.
- Budget: stay inside the $2 monthly credit.

## Open questions

1. What does the Hugging Face content policy allow on Jobs? This blocks all runs.
2. Which classifiers are accurate enough to score each category?
3. Should Text-to-Speech stay in this spec, or get its own?
