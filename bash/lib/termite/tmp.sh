# shellcheck shell=bash
#
# termite tmp: private temporary folders that are always removed.
#
# Functions:
#   tmp_dir VARIABLE
#       Make a new folder only you can read (mktemp -d), store its path in
#       VARIABLE, and remove it with everything in it when the script
#       exits: after success, an error, or Ctrl C. Call it directly, not
#       in $( ), so the clean-up is registered in the script itself.
#       Example: tmp_dir work; curl -o "$work/page.html" -- "$url"

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_tmp_loaded-} ]] || return 0
_termite_tmp_loaded=1

declare -ga _tmp_dirs=()

tmp_dir() {
  local variable=${1-}
  [[ $variable =~ ^[A-Za-z_][A-Za-z0-9_]*$ && $variable != _tmp* ]] ||
    die "tmp_dir: '$variable' is not a usable variable name" 64
  ((BASH_SUBSHELL == 0)) ||
    die "tmp_dir: call it directly, not inside \$( )" 64

  local base=${TMPDIR:-/tmp} name=${TERMITE_SCRIPT_NAME//[^A-Za-z0-9._-]/_}
  local dir
  dir=$(mktemp -d "${base%/}/$name.XXXXXXXX") ||
    die "cannot make a temporary folder in $base"
  ((${#_tmp_dirs[@]})) || core_at_exit _tmp_cleanup
  _tmp_dirs+=("$dir")

  local -n _tmp_target=$variable
  _tmp_target=$dir
}

_tmp_cleanup() {
  local dir
  for dir in "${_tmp_dirs[@]}"; do
    if [[ -d $dir && ! -L $dir ]]; then
      rm -rf -- "$dir"
    fi
  done
}
