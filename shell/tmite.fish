# tmite.fish: ask a local model for a command from the fish prompt.
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
# Load it from ~/.config/fish/config.fish:
#
#   tmite shell fish | source
#
# Set TMITE_BIN if tmite is not on PATH.

set -q TMITE_BIN; or set -g TMITE_BIN tmite

# Ask a question and read one key into the global _tmite_answer. Enter and
# anything that goes wrong both leave it empty.
function _tmite_key
    printf '%s ' $argv[1]
    set -g _tmite_answer ''
    read --nchars 1 --silent --prompt-str '' _tmite_answer </dev/tty
    or set -g _tmite_answer interrupted
end

# Yes to a question. Enter means yes, unless the command is risky.
function _tmite_yes
    if test "$argv[1]" = risky
        _tmite_key 'Risky. Put it at your prompt anyway? [y/N]'
        string match -qr '^[yY]$' -- "$_tmite_answer"
    else
        _tmite_key 'Put it at your prompt? [Y/n]'
        string match -qr '^[yY]?$' -- "$_tmite_answer"
    end
end

function _tmite_enter
    # The request, without leading spaces.
    set -l query (string trim --left -- (commandline -b | string collect))
    if not string match -qr '^[?]' -- "$query"
        commandline -f execute
        return
    end

    while true
        # On its own line, so clearing it leaves the request on screen.
        printf '\ntmite: asking…\n'
        # Progress lines come first; the answer is everything after them.
        set -l lines (command $TMITE_BIN --zle -- "$query" 2>&1 </dev/null | string match -rv '^progress\t')
        printf '\e[1A\r\e[K'
        set -l kind $lines[1]
        set -l rating $lines[2]

        switch "$kind"
            case confirm
                printf '%s\n' $lines[4..-1]
                commandline -r ''
                if _tmite_yes "$rating"
                    commandline -r -- $lines[3]
                end
            case straight_in
                printf '%s\n' $lines[4..-1]
                commandline -r -- $lines[3]
            case refused
                printf '%s\n' $lines[4..-1]
                commandline -r ''
            case choose
                # Line 2 is the count here, not a rating.
                set -l count $rating
                printf '%s\n' $lines[(math $count + 3)..-1]
                _tmite_key 'Press a number, a to ask again, any other key to cancel:'
                if string match -qr '^[aA]$' -- "$_tmite_answer"
                    continue
                end
                commandline -r ''
                if string match -qr '^[1-9]$' -- "$_tmite_answer"; and test "$_tmite_answer" -le "$count"
                    set -l pick (string split --max 1 \t -- $lines[(math $_tmite_answer + 2)])
                    if test "$pick[1]" != risky; or _tmite_yes risky
                        commandline -r -- $pick[2]
                    end
                end
            case no_command failed
                printf '%s\n' $lines[4..-1]
            case '*'
                # Not the line format: tmite is missing, or printed an error.
                printf '%s\n' $lines
        end
        break
    end
    commandline -f repaint
end

for mode in default insert
    bind -M $mode \r _tmite_enter
    bind -M $mode \n _tmite_enter
end
