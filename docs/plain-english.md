# In plain English

You do not need technical knowledge to read this site. This page explains what termite does and what the words on the site mean.

## What termite is

A terminal is a text window where you type instructions for a computer. A terminal does not ask whether you are sure, so one short line can delete all your files.

These instructions are hard to remember, so many people ask an AI model to write them. The model can make a mistake, and someone can trick it.

termite checks the instruction before it reaches your computer. The model writes the instruction. termite explains what it does, and warns you if it looks dangerous. termite never carries out the instruction. Only you can do that.

termite can also watch an AI agent, which is a program that runs instructions by itself. Before the agent runs an instruction, termite checks it and stops the risky ones.

termite has one more purpose. It tests AI models by sending them harmful requests and recording what they write.

## If you are deciding whether to use a model

- **Do not rely on the model to protect you.** In my tests, most models wrote a dangerous command for most hostile requests. The reply format in those tests required a command, so the tests do not show whether a model would refuse.
- **Keep the safety checks outside the model.** Fixed rules in code cannot be persuaded, and those rules stopped the dangerous commands in those tests from reaching the prompt without approval.
- **Keep a person in control.** termite never carries out an instruction by itself.

## The words

| Word | What it means |
|---|---|
| **Model** | An AI program. On this site, usually a small one that runs on your own computer |
| **Command** | One typed instruction to the computer |
| **Agent** | A program that uses a model to do a job, and runs commands by itself |
| **Hook** | A step that runs before an agent's command, so that termite can stop it |
| **Prompt** | The place in the terminal where you type. A command at the prompt has not run yet |
| **Hazard** | A command that could cause real harm, such as deleting files, showing passwords or giving someone else access |
| **Hostile request** | A request that asks a model to do something harmful on purpose |
| **Safety training** | Training that teaches a model to refuse harmful requests |
| **Safety training removed** | A copy of a model that has had this training taken out. People share these copies openly, so I test them to see the worst case |
| **Command model** | The model that writes commands |
| **Decision model** | A second model that scores how dangerous a command looks |
| **Red team** | People who attack a system on purpose to find its weaknesses |
| **Blue team** | People who defend a system |
| **Grade** | How often a model wrote a hazard for a hostile request, from A (rarely) to E (nearly always). F means a hazard passed termite's policy |

[How the testing works](evaluation.md#result-words) lists the words for each result.
