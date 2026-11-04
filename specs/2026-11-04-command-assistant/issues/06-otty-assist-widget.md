# 06 — `otty-assist`: otty widget, palette entry and terminal write

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 04, command blocks ticket 06 (idle prompt check)
Spec: [../spec.md](../spec.md)

## Details

- `app/src/widgets/assist/` with the usual layout (`event.rs`, `reducer.rs`, `state.rs`, `model.rs`, `view/`). Built in a fork of otty. The widget lives in the app and is the `otty-assist` integration; it depends on `termite` (git dependency on this repo), so no core logic is copied into otty.
- Palette: input starting with `?` lists one entry, "Ask: <request>", which sends `PaletteTarget::Ask { query }`; `app/src/events/palette.rs` routes it to `AssistIntent::Submit`.
- On submit, record the active `tab_id`/`terminal_id`, read the current folder (`terminal_cwd_from_blocks`), the session kind, and whether the shell is idle (the command blocks ticket 06 rule). Then run `assist::answer` through `Task::perform`.
- Intents: `Submit { query }`, `Answered(Answer)`, `Choose { index }`, `Run`, `Edit`, `Copy`, `Cancel`. Effect: `WriteToTerminal { tab_id, terminal_id, bytes }`, routed in `app/src/events/assist.rs` to `TerminalWorkspaceIntent::Widget(Event::Write { … })`.
- Edit writes the command alone; Run and `RunNow` write the command plus `\r`. Before writing, check the recorded terminal still exists and is idle; if not, offer Copy only.
- Card: the command in monospace, description, risk as text and color, score; a list for `Choose`; a reason line for `ConfirmWithReason`, `RunNow` and `Refused`; a spinner while waiting, and Esc cancels.
- New strings in `en.json`; other locales fall back to English.

- Known gap (from ticket 02): the policy cannot see shell aliases or functions in the terminal. Before a `RunNow`, ask the user's shell through otty's shell integration whether each program is an alias or function, and fall back to confirming if so. If the integration cannot answer (SSH, other shells), fall back too.

## Tests first

- Reducer: each `Answer` variant gives the right state; Run, Edit and `RunNow` give the right `WriteToTerminal` bytes for the recorded terminal; a closed or busy terminal gives no write (AC7).
- Palette: input `? x` lists the Ask entry; input `x` does not.
