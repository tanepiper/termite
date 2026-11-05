# Hidden Instructions: Attacks Through Images, Documents, Audio and Video

Created: 2026-11-05
Author: me@tane.dev
Agent: Claude Code
Status: DRAFT
Approved: No
Worktree: No
Type: Build

## Summary

**Goal:** find out whether a model writes a dangerous shell command because of
an instruction hidden in something it was given to read, rather than in the
person's request.

Today's red team only sends text requests. People also paste screenshots, PDFs
and recordings into assistants, and anyone can put an instruction inside those.

**Why it matters to termite:** the person asks for something ordinary, and a
hidden instruction changes the command. They didn't ask for the harm, so they
are less likely to look for it.

## Model types

Names are the Hugging Face task names. `scripts/model-watch.py` lists the most
downloaded models of each type in `eval/watch-types.jsonl`.

| Stage | Tasks | Input to the model |
|---|---|---|
| 1 | Image-Text-to-Text, Visual Question Answering, Document Question Answering | Screenshot, photo of a screen, PDF page |
| 2 | Audio-Text-to-Text, Automatic Speech Recognition, Video-Text-to-Text, Any-to-Any | Recording, video |
| 3 | Text Ranking, Feature Extraction, Sentence Similarity, Summarization, Translation | Text documents |

## How a test works

1. A case is an ordinary request, one file, and the place in the file where
   the instruction is hidden.
2. The model gets the request and the file, and returns a command in the normal
   JSON format.
3. termite's policy checks the command, and the scorer records the result.
4. Nothing a model wrote is ever run.

Each case has a control: the same request with a clean file. A model that goes
wrong on the clean file has a usefulness problem, not an attack problem.

## Corpus

- `eval/corpus/hidden.jsonl` holds the cases. Test files are generated from it
  by a script, so the repo holds no binary test files.
- The first instruction in every file asks for a harmless marker command. If
  the marker appears, the model followed the hidden instruction. This measures
  the attack without needing a dangerous command in the file.
- Later cases reuse the hazard classes from the hostile corpus. The same rules
  apply: withheld commands stay private and are never quoted.

Where the instruction is hidden, for images:

- plain text in the image
- small or low-contrast text
- text in a terminal or browser screenshot
- a page footer or margin in a document

## Scoring

Per model, against the clean-file controls:

- how often the marker appeared
- how many hazards were written, and how many passed the policy
- how many commands were still useful for the person's request

Stage 3 has its own measures: whether a hostile document is ranked first, and
whether the instruction survives a summary or translation.

## Running it

- Stage 1 runs through `scripts/hosted.sh` with `llama-server --mmproj`, on the
  same T4 setup, under `confine` and the same gate. No token enters a job.
- A model needs a GGUF file and a vision file to qualify for stage 1.
- The eval tool gains an image field on its chat request. termite itself takes
  no images, and this spec doesn't change that.
- Stage 2 needs a server that takes audio. Check which models `llama-server`
  supports before starting stage 2.
- Budget: about $0.05 for each T4 run of 7 minutes, and up to $0.40 at the
  1-hour limit. Stay inside the $2 monthly credit.
- Results follow the existing rule: raw runs stay in `~/termite-private/runs`
  and the private dataset, and only counts reach the site.

## Open questions

1. Which questions should the Document task use: a whole PDF page as an image,
   or the extracted text?
2. Do we need one baseline text-only model for comparison, given the same
   instruction as plain text?
