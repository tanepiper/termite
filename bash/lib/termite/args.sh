# shellcheck shell=bash
#
# termite args: command line options.
#
# Every script gets these without declaring them:
#   -h, --help      call the script's usage function, exit 0
#   -v, --verbose   more output (twice for debug)
#   -q, --quiet     only warnings and errors
#   -y, --yes       answer yes to every confirm
#       --version   print "NAME VERSION" (see args_version), exit 0
#   --              everything after it is an argument, not an option
#
# Functions:
#   args_flag NAME [SHORT]
#       Declare --NAME (and -SHORT). ARGS[NAME] counts how often it was
#       given: 0 when absent. Example: args_flag dry-run n
#
#   args_value NAME [SHORT] [DEFAULT]
#       Declare --NAME VALUE, --NAME=VALUE, -SHORT VALUE and -SHORTVALUE.
#       ARGS[NAME] is the last value given, or DEFAULT.
#       Example: args_value days d 7
#
#   args_version VERSION
#       The version --version prints. Example: args_version 1.2.0
#
#   args_on NAME
#       True when flag --NAME was given. Example: if args_on dry-run; then
#
#   args_parse "$@"
#       Read the arguments. Options and arguments may be mixed; short
#       flags may be combined (-vn). Arguments go to ARGS_REST in order.
#       An unknown option, a missing value, or a value given to a flag is
#       a usage error: exit 64 with a pointer to --help.
#
#   args_error MESSAGE
#       Report a usage error the same way (exit 64). Use it for checks of
#       your own, such as a missing argument.
#
# Variables:
#   ARGS        associative array of option values, by long name
#   ARGS_REST   array of the arguments that are not options

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  printf '%s is a library; source it from a script\n' "${0##*/}" >&2
  exit 64
fi
[[ -z ${_termite_args_loaded-} ]] || return 0
_termite_args_loaded=1

declare -gA ARGS=()
declare -ga ARGS_REST=()
declare -gA _args_kind=() _args_by_short=()
_args_version=unknown

args_error() {
  log_error "${1:-bad usage}"
  printf "Try '%s --help'.\n" "$TERMITE_SCRIPT_NAME" >&2
  exit 64
}

# The kind of option NAME (flag or value), or nothing when NAME is not a
# declared option. Checks the name first, so odd text is never used as
# an array key.
_args_kind_of() {
  [[ ${1-} =~ ^[a-z0-9-]+$ ]] || return 0
  printf '%s' "${_args_kind[$1]-}"
}

_args_define() {
  local kind=$1 name=${2-} short=${3-} default=${4-}
  [[ $name =~ ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ ]] ||
    die "args: '$name' is not a valid option name" 64
  [[ -z $short || $short =~ ^[a-zA-Z]$ ]] ||
    die "args: '$short' is not a valid short option" 64
  [[ -z ${_args_kind[$name]+set} ]] || die "args: --$name is declared twice" 64
  if [[ -n $short ]]; then
    [[ -z ${_args_by_short[$short]+set} ]] ||
      die "args: -$short is declared twice" 64
    _args_by_short[$short]=$name
  fi
  _args_kind[$name]=$kind
  if [[ $kind == flag ]]; then
    ARGS[$name]=0
  else
    ARGS[$name]=$default
  fi
}

args_version() {
  [[ -n ${1-} ]] || die "args_version: give a version" 64
  _args_version=$1
}

args_on() {
  [[ ${_args_kind[${1-}]-} == flag ]] || die "args_on: --${1-} is not a flag" 64
  [[ ${ARGS[$1]} != 0 ]]
}

args_flag() {
  _args_define flag "$@"
}

args_value() {
  _args_define value "$@"
}

args_parse() {
  ARGS_REST=()
  local arg name value short rest i
  while (($#)); do
    arg=$1
    shift
    case $arg in
      --)
        ARGS_REST+=("$@")
        break
        ;;
      --*=*)
        name=${arg%%=*}
        name=${name#--}
        value=${arg#*=}
        case $(_args_kind_of "$name") in
          value) ARGS[$name]=$value ;;
          flag) args_error "--$name takes no value" ;;
          *) args_error "unknown option --$(log_printable "$name")" ;;
        esac
        ;;
      --?*)
        name=${arg#--}
        case $(_args_kind_of "$name") in
          flag) ARGS[$name]=$((${ARGS[$name]} + 1)) ;;
          value)
            (($#)) || args_error "--$name needs a value"
            ARGS[$name]=$1
            shift
            ;;
          *) args_error "unknown option --$(log_printable "$name")" ;;
        esac
        ;;
      -?*)
        for ((i = 1; i < ${#arg}; i++)); do
          short=${arg:i:1}
          name=''
          [[ ! $short =~ ^[a-zA-Z]$ ]] || name=${_args_by_short[$short]-}
          case $(_args_kind_of "$name") in
            flag) ARGS[$name]=$((${ARGS[$name]} + 1)) ;;
            value)
              rest=${arg:i+1}
              if [[ -n $rest ]]; then
                ARGS[$name]=$rest
              else
                (($#)) || args_error "-$short needs a value"
                ARGS[$name]=$1
                shift
              fi
              break
              ;;
            *) args_error "unknown option -$(log_printable "$short")" ;;
          esac
        done
        ;;
      *)
        ARGS_REST+=("$arg")
        ;;
    esac
  done

  if args_on help; then
    declare -F usage >/dev/null || die "this script has no help text" 70
    usage
    exit 0
  fi
  if args_on version; then
    printf '%s %s\n' "$TERMITE_SCRIPT_NAME" "$_args_version"
    exit 0
  fi
  # shellcheck disable=SC2034 # TERMITE_LOG_LEVEL is read by log.sh
  if args_on quiet; then
    TERMITE_LOG_LEVEL=0
  else
    TERMITE_LOG_LEVEL=$((ARGS[verbose] >= 2 ? 3 : 1 + ARGS[verbose]))
  fi
}

args_flag help h
args_flag verbose v
args_flag quiet q
args_flag yes y
args_flag version
