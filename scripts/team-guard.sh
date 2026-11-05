#!/usr/bin/env bash
# The hook on the blue and red team agents: the only shell command they may
# use is scripts/team.sh, on its own. Exit 2 stops the command. Anything that
# can't be read clearly is stopped too. Takes the hook's JSON on stdin.
set -uo pipefail

command=$(jq -r '.tool_input.command // empty' 2>/dev/null) || command=
stop() {
  printf 'Stopped: agents may only run scripts/team.sh, on its own. %s\n' "$1" >&2
  exit 2
}

[[ -n $command ]] || stop 'No command found.'
[[ $command != *$'\n'* ]] || stop 'One line only.'
[[ $command =~ ^scripts/team\.sh\ (gate|roster|check|blue|red)($|\ ) ]] \
  || stop 'Use: scripts/team.sh gate|roster|check|blue|red.'

# The text must be one single-quoted word, so the shell can't act on any of it.
rest=${command#scripts/team.sh }
rest=${rest#* }
if [[ $rest == "$command" || $rest == "${command#scripts/team.sh }" ]]; then
  exit 0
fi
[[ $rest =~ ^\'[^\']*\'$ ]] || stop "Put the text in one pair of single quotes, with no single quotes inside."
exit 0
