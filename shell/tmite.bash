# tmite.bash: ask a local model for a command from the bash prompt.
#
# A convenience only: `tmite` does the same job without it. Type one of these
# and press Enter:
#
#   ? request    explains one command; say yes to put it at the prompt
#   ?? request   explains every command; pick one, ask again or cancel
#   ?x request   puts the command straight at the prompt if every check
#                passed, otherwise behaves like ?
#   ?b request   writes a script as a draft, and puts the command that shows
#                and installs it at the prompt
#
# termite never runs anything. A command only ever reaches the prompt, and
# pressing Enter is up to you.
#
# Load it from ~/.bashrc (needs bash 4 or newer):
#
#   eval "$(tmite shell bash)"
#
# Set TMITE_BIN if tmite is not on PATH.

: "${TMITE_BIN:=tmite}"

# bash can't stop Enter from a key binding, so Enter is a macro of two keys:
# the first runs _tmite_enter, the second does whatever _tmite_enter left it
# bound to. It's set to do nothing first, so a request can never be accepted.
_tmite_hold() { bind '"\C-x\C-u": redraw-current-line'; }
_tmite_pass() { bind '"\C-x\C-u": accept-line'; }

# Ask tmite, drawing progress while it works. Sets _tmite_lines. Progress
# lines come first; anything after the answer starts is the answer.
_tmite_ask() {
  local line message
  _tmite_lines=()
  printf 'tmite: asking…'
  while IFS= read -r line || [[ -n $line ]]; do
    if (( ${#_tmite_lines[@]} == 0 )) && [[ $line == progress$'\t'* ]]; then
      # One row only: a line that wraps cannot be cleared.
      message="tmite: ${line#*$'\t'}"
      printf '\r\033[K%s' "${message:0:${COLUMNS:-80}-1}"
    else
      _tmite_lines+=("$line")
    fi
  done < <(command "$TMITE_BIN" --zle -- "$1" 2>&1 </dev/null)
  printf '\r\033[K'
}

# Print the lines from index $1 on. Colour comes from tmite.
_tmite_show() {
  printf '%s\n' "${_tmite_lines[@]:$1}"
}

# Ask a question and read one key into _tmite_answer.
_tmite_key() {
  printf '%s ' "$1"
  IFS= read -rsn1 _tmite_answer </dev/tty
  printf '\n'
}

# Yes to a question. Enter means yes, unless the command is risky.
_tmite_yes() {
  if [[ $1 == risky ]]; then
    _tmite_key 'Risky. Put it at your prompt anyway? [y/N]'
    [[ $_tmite_answer == [yY] ]]
  else
    _tmite_key 'Put it at your prompt? [Y/n]'
    [[ -z $_tmite_answer || $_tmite_answer == [yY] ]]
  fi
}

_tmite_insert() {
  READLINE_LINE=$1
  READLINE_POINT=${#READLINE_LINE}
}

# Returns 1 to ask again.
_tmite_choose() {
  local count=$1 pick rating
  _tmite_show $(( count + 2 ))
  _tmite_key 'Press a number, a to ask again, any other key to cancel:'

  if [[ $_tmite_answer == [aA] ]]; then
    return 1
  fi
  READLINE_LINE=
  READLINE_POINT=0
  if [[ $_tmite_answer == [1-9] ]] && (( _tmite_answer <= count )); then
    pick=${_tmite_lines[_tmite_answer + 1]}
    rating=${pick%%$'\t'*}
    if [[ $rating != risky ]] || _tmite_yes risky; then
      _tmite_insert "${pick#*$'\t'}"
    fi
  fi
  return 0
}

_tmite_enter() {
  _tmite_hold

  # The request, without leading spaces.
  local query="${READLINE_LINE#"${READLINE_LINE%%[![:space:]]*}"}"
  if [[ $query != '?'* ]]; then
    _tmite_pass
    return
  fi

  # readline clears the line before a bound function runs, so print the
  # request again or it vanishes from the screen.
  printf '%s\n' "$query"

  local kind rating command
  while true; do
    _tmite_ask "$query"
    kind=${_tmite_lines[0]-} rating=${_tmite_lines[1]-} command=${_tmite_lines[2]-}

    # An older tmite has no rating line, so its lines mean something else here.
    # Line 3 would be its description, so nothing may reach the prompt.
    if [[ $kind == confirm || $kind == straight_in ]] &&
      [[ $rating != reads && $rating != changes && $rating != risky ]]; then
      printf '%s\n' 'tmite: this widget does not match the installed tmite. Nothing was put at your prompt. Update tmite, and then open a new shell.'
      READLINE_LINE=
      READLINE_POINT=0
      break
    fi

    case $kind in
      confirm)
        _tmite_show 3
        READLINE_LINE=
        READLINE_POINT=0
        if _tmite_yes "$rating"; then
          _tmite_insert "$command"
        fi
        ;;
      straight_in)
        _tmite_show 3
        _tmite_insert "$command"
        ;;
      refused)
        _tmite_show 3
        READLINE_LINE=
        READLINE_POINT=0
        ;;
      choose)
        # Line 2 is the count here, not a rating.
        _tmite_choose "$rating" || continue
        ;;
      no_command|failed)
        _tmite_show 3
        ;;
      *)
        # Not the line format: tmite is missing, or printed a usage error.
        _tmite_show 0
        ;;
    esac
    break
  done
}

if (( BASH_VERSINFO[0] >= 4 )); then
  _tmite_hold
  bind -x '"\C-x\C-t": _tmite_enter'
  bind '"\C-m": "\C-x\C-t\C-x\C-u"'
  bind '"\C-j": "\C-x\C-t\C-x\C-u"'
else
  printf 'tmite: the bash widget needs bash 4 or newer, and this is %s. tmite itself still works.\n' "$BASH_VERSION" >&2
fi
