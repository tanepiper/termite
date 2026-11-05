# In plain English

You don't need to be technical to read this site. Here's what it's about, and what
the words mean.

## What termite is

A terminal is the plain text window where you give a computer typed instructions.
It's powerful, and it doesn't ask if you're sure. One short line can delete
everything you own.

Those instructions are hard to remember, so people ask AI to write them. That works
until the AI gets it wrong, or someone tricks it. termite sits in the middle. The AI
writes the instruction, and termite explains what it does and warns you if it looks
dangerous. It never carries it out. You always do that yourself.

It also does a second job: testing AI models before anyone trusts them, by asking
them to do bad things and writing down what they did.

## If you're deciding whether to use a model

Three things to take from this site:

- **Small AI models mostly do as they're told.** Ask one to do something harmful and
  it usually will. Don't count on the model to say no.
- **The safety has to sit outside the model.** Plain rules that can't be talked
  round are what kept these commands from going anywhere.
- **A person stays in charge.** termite never carries out an instruction on its own.

## The words

| Word | What it means |
|---|---|
| **Model** | An AI, like the one behind ChatGPT. Here, small ones that run on your own computer rather than someone else's |
| **Command** | One typed instruction to the computer |
| **Prompt** | The spot in the terminal where you type. A command sitting there has not happened yet |
| **Hazardous** | A command that could do real harm: delete things, show passwords, let someone in |
| **Hostile request** | Someone asking the AI to do something harmful on purpose |
| **Safety training** | The training that teaches an AI to say no to harmful requests |
| **Safety training removed** | A copy of a model with that training deliberately stripped out, so it won't say no. People share these openly, so we test them to see the worst case |
| **Command model** | The AI that writes the commands |
| **Decision model** | A second AI that scores how dangerous a command looks |
| **Red team** | People who attack something on purpose, to find the weak spots first |
| **Blue team** | People who defend it |
| **Grade** | How often a model went along with hostile requests, from A (rarely) to E (nearly always). F means something got past termite's rules |
