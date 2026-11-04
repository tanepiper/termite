# tmite.zsh: ask a local model for a command from the zsh prompt.
#
# Type one of these and press Enter:
#
#   ? request    put the best command at the prompt; Enter runs it
#   ?? request   list every command; press its number to put it at the prompt
#   ?x request   run the command at once if every safety check passed,
#                otherwise behave like ?
#
# Source it from ~/.zshrc, after any plugin that wraps accept-line:
#
#   source /path/to/termite/shell/tmite.zsh
#
# Set TMITE_BIN if tmite is not on PATH.

: ${TMITE_BIN:=tmite}

# Keep the accept-line that was there before (another plugin may wrap it),
# once, so sourcing this file twice does not make the widget call itself.
if (( ! ${+widgets[_tmite_previous_accept_line]} )); then
  zle -A accept-line _tmite_previous_accept_line
fi

# Succeed if every program in the command (the first word, and the first
# word after each |) is a plain command or builtin. The safety checks judged
# the program by name; an alias or function of that name could run
# anything. On failure, REPLY holds the program that is not plain.
_tmite_plain_programs() {
  local word kind expect_program=1

  for word in ${(z)1}; do
    if (( expect_program )); then
      kind=$(whence -w -- "${(Q)word}" 2>/dev/null)
      kind=${kind##*: }
      if [[ $kind != command && $kind != builtin ]]; then
        REPLY=${(Q)word}
        return 1
      fi
      expect_program=0
    elif [[ $word == '|' ]]; then
      expect_program=1
    fi
  done

  return 0
}

_tmite_accept_line() {
  # The request, without leading spaces.
  local query="${BUFFER#"${BUFFER%%[![:space:]]*}"}"

  if [[ $query != '?'* ]]; then
    zle _tmite_previous_accept_line
    return
  fi

  zle -R "tmite: asking…"

  local output
  output=$(command "$TMITE_BIN" --zle -- "$query" 2>&1 </dev/null)

  local -a lines
  lines=("${(@f)output}")
  local kind=$lines[1] command=$lines[2] message=$lines[3]

  case $kind in
    confirm|confirm_reason)
      BUFFER=$command
      CURSOR=$#BUFFER
      zle -M -- "$message"
      ;;
    run)
      BUFFER=$command
      CURSOR=$#BUFFER
      if _tmite_plain_programs "$command"; then
        zle -I
        print -r -- "tmite: $message"
        zle _tmite_previous_accept_line
      else
        zle -M -- "Not run automatically: \`$REPLY\` is an alias or function here"
      fi
      ;;
    refused)
      BUFFER=
      zle -M -- "$message"
      ;;
    choose)
      # Line 2 is the count, then one "command<TAB>details" line each.
      local count=$command i menu=
      local -a options=("${(@)lines[3,count + 2]}")

      for (( i = 1; i <= count && i <= 9; i++ )); do
        menu+="$i. ${options[i]%%$'\t'*}"$'\n'"   ${options[i]#*$'\t'}"$'\n'
      done
      zle -M -- "${menu}Press a number, or any other key to cancel."
      # Draw the menu now: read -k below blocks before the next redisplay.
      zle -R

      local key
      read -k 1 key
      zle -M ""
      if [[ $key == [1-9] ]] && (( key <= count )); then
        BUFFER=${options[key]%%$'\t'*}
        CURSOR=$#BUFFER
      fi
      ;;
    no_command|failed)
      zle -M -- "$message"
      ;;
    *)
      # Not the line format: tmite is missing, or printed a usage error.
      zle -M -- "${output:-tmite printed nothing}"
      ;;
  esac
}

zle -N accept-line _tmite_accept_line
