# shellcheck shell=bash
#
# termite require: check up front for what the script needs, and fail
# early with a message that says how to fix it.
#
# Functions:
#   require_cmd COMMAND...
#       Exit 1 unless every COMMAND is installed. Names every missing one,
#       with an install hint. Example: require_cmd rsync jq
#
#   require_bash MAJOR [MINOR]
#       Exit 1 unless bash is at least MAJOR.MINOR. Example: require_bash 5 3
#
#   require_os NAME...
#       Exit 1 unless this is one of the named systems: macos, linux.
#       Example: require_os macos

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_require_loaded-} ]] || return 0
_termite_require_loaded=1

# Commands whose package has another name.
declare -gA _require_package=(
  [rg]=ripgrep [gsed]=gnu-sed [gdate]=coreutils [gstat]=coreutils
  [convert]=imagemagick [http]=httpie [7z]=p7zip
)

_require_os_name() {
  case $OSTYPE in
    darwin*) printf 'macos' ;;
    linux*) printf 'linux' ;;
    *) printf '%s' "$OSTYPE" ;;
  esac
}

require_cmd() {
  local command
  local -a missing=() packages=()
  for command in "$@"; do
    if ! command -v -- "$command" >/dev/null 2>&1; then
      missing+=("$command")
      packages+=("${_require_package[$command]:-$command}")
    fi
  done
  ((${#missing[@]})) || return 0

  local IFS=' ' hint
  if [[ $(_require_os_name) == macos ]]; then
    hint="Install with: brew install ${packages[*]}"
  else
    hint="Install ${packages[*]} with your package manager."
  fi
  die "needs ${missing[*]}, not found on PATH. $hint"
}

require_bash() {
  local major=${1-} minor=${2:-0}
  [[ $major =~ ^[0-9]+$ && $minor =~ ^[0-9]+$ ]] ||
    die "require_bash: give a version such as 5 3" 64
  if ((BASH_VERSINFO[0] > major ||
    (BASH_VERSINFO[0] == major && BASH_VERSINFO[1] >= minor))); then
    return 0
  fi
  die "needs bash $major.$minor or later; this is $BASH_VERSION"
}

require_os() {
  local current name
  current=$(_require_os_name)
  for name in "$@"; do
    [[ $name != "$current" ]] || return 0
  done
  local IFS=' '
  die "runs only on $*; this is $current"
}
