#!/usr/bin/env bash
# Installs, updates, reconfigures or removes the ZyrDesk server on Debian
# 12 or 13, in a Proxmox LXC container or on an ordinary machine, under
# systemd.
#
#   bash install.sh                 questions, then installation
#   bash install.sh --help          the options
#
# What it does and what it does not do is written in docs/SERVER.md,
# section 10. It does not configure a reverse proxy, does not open the
# router, does not install a firewall, and never touches an existing
# installation without saying so and having it confirmed.

set -euo pipefail

readonly REPO="Victor-root/ZyrDesk"
readonly BIN="/usr/local/bin/zyrdesk-server"
readonly CONF_DIR="/etc/zyrdesk-server"
readonly CONF="$CONF_DIR/server.toml"
readonly STATE="$CONF_DIR/install.env"
readonly TLS_DIR="$CONF_DIR/tls"
readonly UNIT="/etc/systemd/system/zyrdesk-server.service"
readonly DROPIN_DIR="/etc/systemd/system/zyrdesk-server.service.d"
readonly SERVICE_USER="zyrdesk"
readonly DEFAULT_DATA="/var/lib/zyrdesk-server"
readonly SHORTEST_PASSWORD=12

# Everything this script writes along the way: the log of each step, what
# the step hands back, and the clone with its build when the source is
# asked for. A single folder, removed on exit whatever the exit, rather
# than several gigabytes of leftovers in /tmp after every installation.
# The steps' subshells do not inherit this trap, so none of them takes the
# folder away when it finishes.
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# ---- The language ----------------------------------------------------------

SCRIPT_LANG="en"
case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
  fr*) SCRIPT_LANG="fr" ;;
esac

# The French word, or the English one when the machine is not in French.
t() {
  if [[ $SCRIPT_LANG == fr ]]; then printf '%s' "$1"; else printf '%s' "$2"; fi
}

# ---- The colours -----------------------------------------------------------
#
# The palette of design.css, in true colour when the terminal announces
# it, in 256 colours otherwise; nothing at all outside a terminal or
# under NO_COLOR.

INTERACTIVE=0
[[ -t 0 && -t 1 ]] && INTERACTIVE=1

if [[ $INTERACTIVE -eq 1 && -z ${NO_COLOR:-} && ${TERM:-dumb} != dumb ]]; then
  if [[ ${COLORTERM:-} == truecolor || ${COLORTERM:-} == 24bit ]]; then
    C_ACCENT=$'\e[38;2;239;181;54m'
    C_BRIGHT=$'\e[38;2;248;205;106m'
    C_MUTED=$'\e[38;2;106;74;18m'
    C_SOFT=$'\e[38;2;160;167;184m'
    C_FAINT=$'\e[38;2;107;115;133m'
    C_OK=$'\e[38;2;52;211;153m'
    C_WARNING=$'\e[38;2;249;115;22m'
    C_ERROR=$'\e[38;2;248;113;113m'
  else
    C_ACCENT=$'\e[38;5;214m'
    C_BRIGHT=$'\e[38;5;221m'
    C_MUTED=$'\e[38;5;94m'
    C_SOFT=$'\e[38;5;248m'
    C_FAINT=$'\e[38;5;243m'
    C_OK=$'\e[38;5;78m'
    C_WARNING=$'\e[38;5;208m'
    C_ERROR=$'\e[38;5;203m'
  fi
  C_BOLD=$'\e[1m'
  C_RESET=$'\e[0m'
else
  C_ACCENT="" C_BRIGHT="" C_MUTED="" C_SOFT="" C_FAINT="" C_OK="" C_WARNING="" C_ERROR=""
  C_BOLD="" C_RESET=""
fi

# A value, a path, a name: in bold, never in colour.
bold() { printf '%s%s%s' "$C_BOLD" "$1" "$C_RESET"; }

info() { printf '%s›%s %s\n' "$C_BRIGHT" "$C_RESET" "$1"; }
ok()   { printf '%s✓%s %s\n' "$C_OK" "$C_RESET" "$1"; }
warn() { printf '%s⚠%s %s\n' "$C_WARNING" "$C_RESET" "$1"; }
fail() { printf '%s✗%s %s\n' "$C_ERROR" "$C_RESET" "$1" >&2; }

# An open panel: its title in the colour of its meaning, its lines, then
# the corner that closes it.
panel_open() { printf '\n%s┌ %s%s\n' "$2" "$1" "$C_RESET"; }
panel_line() { printf '%s│%s %s\n' "$C_FAINT" "$C_RESET" "$1"; }
panel_close() { printf '%s└%s\n' "$C_FAINT" "$C_RESET"; }

# Two aligned columns in a panel: the key, then the value in bold.
panel_key() { panel_line "$(printf '%-16s: %s' "$1" "$(bold "$2")")"; }

# ---- The banner ------------------------------------------------------------

banner() {
  local width
  width=$(tput cols 2>/dev/null || echo 60)
  (( width > 72 )) && width=72
  printf '%s' "$C_ACCENT"
  cat <<'LOGO'
███████╗██╗   ██╗██████╗ ██████╗ ███████╗███████╗██╗  ██╗
╚══███╔╝╚██╗ ██╔╝██╔══██╗██╔══██╗██╔════╝██╔════╝██║ ██╔╝
  ███╔╝  ╚████╔╝ ██████╔╝██║  ██║█████╗  ███████╗█████╔╝
 ███╔╝    ╚██╔╝  ██╔══██╗██║  ██║██╔══╝  ╚════██║██╔═██╗
███████╗   ██║   ██║  ██║██████╔╝███████╗███████║██║  ██╗
╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═════╝ ╚══════╝╚══════╝╚═╝  ╚═╝
LOGO
  printf '%s\n' "$C_RESET"
  printf '  %s%s%s   %s· par Victor-root%s\n' "$C_BRIGHT" "$(t 'Serveur ZyrDesk · installation' 'ZyrDesk server · installation')" "$C_RESET" "$C_FAINT" "$C_RESET"
  printf '%s' "$C_MUTED"
  printf '─%.0s' $(seq 1 "$width")
  printf '%s\n' "$C_RESET"
}

# ---- The questions ---------------------------------------------------------

# The prompt of a question, to be given to readline and to readline alone.
#
# Its colours are wrapped in the two marks readline needs to know they are
# not displayed. Without them, it counts the colour codes as characters,
# believes itself much further along the line than it is, and the next
# question gets written over the one just answered: three questions on
# one line and an unreadable screen. These marks only mean something to
# readline: shown as they are, they would leave two control bytes on the
# screen.
prompt() {
  local tint=$1 label=$2 hint=${3:-} text
  local before=$'\001' after=$'\002'
  text="$before$tint$after?$before$C_RESET$after $label"
  [[ -n $hint ]] && text+=" $before$C_SOFT$after$hint$before$C_RESET$after"
  printf '%s : ' "$text"
}

# A question, its default in brackets, and what is answered, or the
# default when nothing is.
ask() {
  local __variable=$1 label=$2 default=${3:-} answer
  while true; do
    IFS= read -r -e -p "$(prompt "$C_BRIGHT" "$label" "${default:+[$default]}")" answer ||
      { echo; exit 1; }
    answer=${answer:-$default}
    if [[ -n $answer ]]; then
      printf -v "$__variable" '%s' "$answer"
      return 0
    fi
    warn "$(t 'Il faut une réponse.' 'An answer is needed.')"
  done
}

# A question whose answer is not shown, asked twice.
ask_secret() {
  local __variable=$1 label=$2 first second
  while true; do
    printf '%s?%s %s : ' "$C_BRIGHT" "$C_RESET" "$label"
    IFS= read -r -s first || { echo; exit 1; }
    echo
    if (( ${#first} < SHORTEST_PASSWORD )); then
      warn "$(t "$SHORTEST_PASSWORD caractères au moins." "$SHORTEST_PASSWORD characters at least.")"
      continue
    fi
    printf '%s?%s %s : ' "$C_BRIGHT" "$C_RESET" "$(t 'Encore une fois, pour être sûr' 'Once more, to be sure')"
    IFS= read -r -s second || { echo; exit 1; }
    echo
    if [[ $first == "$second" ]]; then
      printf -v "$__variable" '%s' "$first"
      return 0
    fi
    warn "$(t 'Les deux ne sont pas pareils.' 'The two differ.')"
  done
}

# Yes or no, Enter meaning the default, written out in full.
ask_yes() {
  local label=$1 default=${2:-oui} answer hint
  if [[ $default == oui ]]; then
    hint=$(t '[Entrée=oui / non]' '[Enter=yes / no]')
  else
    hint=$(t '[oui / Entrée=non]' '[yes / Enter=no]')
  fi
  while true; do
    IFS= read -r -e -p "$(prompt "$C_BRIGHT" "$label" "$hint")" answer ||
      { echo; exit 1; }
    case "${answer,,}" in
      "") [[ $default == oui ]] && return 0 || return 1 ;;
      o|oui|y|yes) return 0 ;;
      n|non|no) return 1 ;;
    esac
    warn "$(t 'Répondez oui ou non.' 'Answer yes or no.')"
  done
}

# A numbered choice among the ones that follow.
ask_choice() {
  local __variable=$1 label=$2 default=$3 answer
  shift 3
  printf '%s?%s %s :\n' "$C_BRIGHT" "$C_RESET" "$label"
  local rank=1
  for option in "$@"; do
    printf '   %s%d)%s %s\n' "$C_SOFT" "$rank" "$C_RESET" "$option"
    rank=$((rank + 1))
  done
  while true; do
    IFS= read -r -e -p "$(prompt "$C_BRIGHT" "$(t 'Votre choix' 'Your choice')" "[$default]")" answer ||
      { echo; exit 1; }
    answer=${answer:-$default}
    if [[ $answer =~ ^[0-9]+$ ]] && (( answer >= 1 && answer <= $# )); then
      printf -v "$__variable" '%s' "$answer"
      return 0
    fi
    warn "$(t "Un nombre entre 1 et $#." "A number between 1 and $#.")"
  done
}

# What cannot be undone is confirmed by typing the whole word.
confirm_in_full() {
  local word answer
  word=$(t 'oui' 'yes')
  IFS= read -r -e -p "$(prompt "$C_WARNING" \
    "$(t "Tapez « $word » pour continuer" "Type \"$word\" to continue")" \
    "[$(t 'autre chose annule' 'anything else cancels')]")" answer || { echo; exit 1; }
  [[ $answer == "$word" ]]
}

# ---- The steps -------------------------------------------------------------

# What a step has learnt and the rest needs to know.
#
# A step runs in a subshell: its output is put aside, and above all the
# first thing that fails stops it instead of letting it carry on after a
# half failure. Its variables therefore die with it. The ones the rest
# needs are written here, and the step, once it succeeds, hands them back
# to the script. Without that, the installation put in place the binary it
# had just built from an empty path.
STEP_RESULT=""

hand_back() {
  local name
  for name in "$@"; do
    printf '%s=%q\n' "$name" "${!name:-}" >>"$STEP_RESULT"
  done
}

# A step behind a spinner, rewritten as ✓ or ✗; the output of what it did
# is only shown when it fails.
step() {
  local label=$1 log status=0
  shift
  log=$(mktemp -p "$WORK_DIR")
  STEP_RESULT=$(mktemp -p "$WORK_DIR")
  # Run in the background, on a terminal or not: that is what makes the
  # first thing that fails in the step stop it there. Waited for through
  # a "||", it would go to the end of its half failure and call itself a
  # success, a failed download included.
  ( "$@" ) >"$log" 2>&1 &
  local worker=$!
  if [[ $INTERACTIVE -eq 1 ]]; then
    local spinner='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏' i=0
    while kill -0 "$worker" 2>/dev/null; do
      printf '\r%s%s%s %s' "$C_BRIGHT" "${spinner:i%10:1}" "$C_RESET" "$label"
      i=$((i + 1))
      sleep 0.1
    done
  fi
  wait "$worker" || status=$?
  if [[ $INTERACTIVE -eq 1 ]]; then
    printf '\r\033[K'
  fi
  if [[ $status -eq 0 ]]; then
    # shellcheck source=/dev/null
    source "$STEP_RESULT"
    ok "$label"
    return 0
  fi
  fail "$label"
  sed 's/^/    /' "$log" >&2
  exit 1
}

# ---- What is known about the machine ---------------------------------------

ARCH=""
OS_ID=""
OS_VERSION=""
CONTAINER=""
UNPRIVILEGED=0
LOCAL_IP=""
PUBLIC_IP=""

survey_the_machine() {
  ARCH=$(uname -m)
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    OS_ID=${ID:-}
    OS_VERSION=${VERSION_ID:-}
  fi
  CONTAINER=$(systemd-detect-virt --container 2>/dev/null || true)
  [[ $CONTAINER == none ]] && CONTAINER=""
  if [[ -r /proc/self/uid_map ]] && ! grep -qE '^\s*0\s+0\s+4294967295$' /proc/self/uid_map; then
    UNPRIVILEGED=1
  fi
  LOCAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
  PUBLIC_IP=$(curl -fsS4 --max-time 5 https://api.ipify.org 2>/dev/null || true)
  [[ $PUBLIC_IP =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || PUBLIC_IP=""
}

# An address in 100.64.0.0/10: the router has no public address of its
# own, and no port can be forwarded to this machine from the Internet.
is_cgnat() {
  [[ $1 =~ ^100\.([0-9]+)\. ]] && (( BASH_REMATCH[1] >= 64 && BASH_REMATCH[1] <= 127 ))
}

is_an_ip() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# The name of the binary published for this architecture: x86_64 only, the
# one of Proxmox containers; the others build on the spot.
binary_name() {
  case "$ARCH" in
    x86_64) echo "zyrdesk-server-x86_64-linux-musl" ;;
    *) return 1 ;;
  esac
}

# ---- The checks ------------------------------------------------------------

# The tools needed, depending on how the program is obtained. Building on
# the spot needs a C compiler, since the embedded database and the
# encryption contain C, and git when the repository is still to be
# cloned.
needed_tools() {
  echo curl
  echo openssl
  if [[ $FROM_SOURCE -eq 1 ]]; then
    echo build-essential
    [[ -n $GIVEN_SOURCE ]] || echo git
  fi
}

check_the_prerequisites() {
  local missing=0
  if [[ $EUID -ne 0 ]]; then
    fail "$(t 'Ce script s'"'"'exécute en root : sudo bash install.sh' 'This script runs as root: sudo bash install.sh')"
    exit 1
  fi
  if [[ ! -d /run/systemd/system ]]; then
    fail "$(t 'systemd ne tourne pas ici : le serveur s'"'"'installe comme un service systemd.' 'systemd is not running here: the server installs as a systemd service.')"
    exit 1
  fi
  if [[ $OS_ID != debian ]] || [[ $OS_VERSION != 12 && $OS_VERSION != 13 ]]; then
    warn "$(t "Ce système n'est pas un Debian 12 ou 13 (${OS_ID:-inconnu} ${OS_VERSION:-}) : le script n'y a pas été essayé." "This system is not Debian 12 or 13 (${OS_ID:-unknown} ${OS_VERSION:-}): the script was not tried on it.")"
    ask_yes "$(t 'Continuer quand même ?' 'Continue anyway?')" non || exit 1
  fi
  if ! binary_name >/dev/null; then
    warn "$(t "Aucun binaire n'est publié pour $ARCH : il faudra --from-source." "No binary is published for $ARCH: --from-source is needed.")"
    [[ $FROM_SOURCE -eq 1 ]] || exit 1
  fi
  for package in $(needed_tools); do
    # build-essential is a package, not a command: what it brings and what
    # would be missing here is the compiler.
    tool=$package
    [[ $package == build-essential ]] && tool=cc
    if ! command -v "$tool" >/dev/null 2>&1; then
      info "$(t "$tool manque : il sera installé (apt-get install $package)." "$tool is missing: it will be installed (apt-get install $package).")"
      missing=1
    fi
  done
  if [[ $missing -eq 1 ]] && ! command -v apt-get >/dev/null 2>&1; then
    fail "$(t 'apt-get est introuvable : installez ce qui manque, puis relancez.' 'apt-get is missing: install what is missing, then run again.')"
    exit 1
  fi
  # Rust installed by rustup lives there, and is only on the path once the
  # session is reopened: the script goes and gets it there rather than
  # asking to start everything over.
  if [[ $FROM_SOURCE -eq 1 ]]; then
    if ! command -v cargo >/dev/null 2>&1 && [[ -x ${HOME:-/root}/.cargo/bin/cargo ]]; then
      PATH="${HOME:-/root}/.cargo/bin:$PATH"
    fi
    if ! command -v cargo >/dev/null 2>&1; then
      fail "$(t 'Compiler sur place demande Rust, et cargo est introuvable.' 'Building here needs Rust, and cargo is missing.')"
      info "curl -fsSL https://sh.rustup.rs | sh -s -- -y && . \"\$HOME/.cargo/env\""
      exit 1
    fi
  fi
}

# ---- The answers kept ------------------------------------------------------

NAME="" PUBLIC_HOST="" TLS_MODE="" API_PORT="" LOCAL_PORT="" CERT_FILE="" KEY_FILE=""
RELAY_ENABLED="" RELAY_PORT="" DATA_DIR="" REGISTRATION="" ADMIN_USER="" ADMIN_PASSWORD=""
VERSION_INSTALLEE=""

load_the_state() {
  if [[ -r $STATE ]]; then
    # shellcheck disable=SC1090
    . "$STATE"
  fi
}

# Everything but the password, which is written nowhere.
save_the_state() {
  {
    echo "# Les réponses de la dernière installation, proposées en défaut à la relance."
    for key in NAME PUBLIC_HOST TLS_MODE API_PORT LOCAL_PORT CERT_FILE KEY_FILE RELAY_ENABLED RELAY_PORT DATA_DIR REGISTRATION ADMIN_USER VERSION_INSTALLEE; do
      printf '%s=%q\n' "$key" "${!key}"
    done
  } >"$STATE"
  chmod 600 "$STATE"
}

# ---- The installation questions -------------------------------------------

ask_the_questions() {
  local choice default_host default_name default_account

  default_name=${NAME:-$(t 'Maison' 'Home')}
  ask NAME "$(t 'Nom affiché du serveur' 'Name the application shows for this server')" "$default_name"

  default_host=${PUBLIC_HOST:-${PUBLIC_IP:-$LOCAL_IP}}
  ask PUBLIC_HOST "$(t 'Adresse publique (domaine ou IP)' 'Public address (domain or IP)')" "$default_host"
  if is_an_ip "$PUBLIC_HOST" && is_cgnat "$PUBLIC_HOST"; then
    warn "$(t "$PUBLIC_HOST est une adresse partagée (CGNAT) : aucun port ne se renvoie vers cette machine depuis Internet. Un domaine ou un VPN sera nécessaire." "$PUBLIC_HOST is a shared address (CGNAT): no port can be forwarded to this machine from the Internet. A domain or a VPN will be needed.")"
  fi

  ask_choice choice "$(t "Chiffrement de l'API" 'Encryption of the API')" "${TLS_MODE:-2}" \
    "$(t "J'ai déjà un mandataire inverse avec un certificat valide" 'I already have a reverse proxy with a valid certificate')" \
    "$(t "Générer un certificat auto-signé (à confirmer dans l'application)" 'Generate a self-signed certificate (to confirm in the application)')" \
    "$(t "J'ai mes propres fichiers de certificat" 'I have my own certificate files')"
  TLS_MODE=$choice
  case "$TLS_MODE" in
    1)
      ask LOCAL_PORT "$(t 'Port de boucle locale que le mandataire renvoie vers le serveur' 'Loopback port the proxy forwards to the server')" "${LOCAL_PORT:-8443}"
      API_PORT=443
      CERT_FILE="" KEY_FILE=""
      ;;
    2)
      ask API_PORT "$(t "Port TCP de l'API" 'TCP port of the API')" "${API_PORT:-443}"
      LOCAL_PORT="" CERT_FILE="" KEY_FILE=""
      ;;
    3)
      ask API_PORT "$(t "Port TCP de l'API" 'TCP port of the API')" "${API_PORT:-443}"
      LOCAL_PORT=""
      while true; do
        ask CERT_FILE "$(t 'Certificat (chaîne complète, PEM)' 'Certificate (full chain, PEM)')" "${CERT_FILE:-}"
        ask KEY_FILE "$(t 'Clé privée (PEM)' 'Private key (PEM)')" "${KEY_FILE:-}"
        if [[ ! -r $CERT_FILE ]]; then warn "$(t "$CERT_FILE ne se lit pas." "$CERT_FILE cannot be read.")"; continue; fi
        if [[ ! -r $KEY_FILE ]]; then warn "$(t "$KEY_FILE ne se lit pas." "$KEY_FILE cannot be read.")"; continue; fi
        if ! go_together "$CERT_FILE" "$KEY_FILE"; then
          warn "$(t 'Ce certificat et cette clé ne vont pas ensemble.' 'This certificate and this key do not match.')"
          continue
        fi
        break
      done
      ;;
  esac

  # The mirror answers on this port whatever happens: it is the one that
  # tells a device its address as seen from outside, and so what makes a
  # direct connection possible. The relay, for its part, can be turned
  # off.
  ask RELAY_PORT "$(t "Port UDP du miroir et du relais" 'UDP port of the mirror and the relay')" "${RELAY_PORT:-443}"
  if ask_yes "$(t "Activer le relais (secours quand aucun chemin direct n'existe)" 'Enable the relay (fallback when no direct path exists)')" "${RELAY_ENABLED:-oui}"; then
    RELAY_ENABLED=oui
  else
    RELAY_ENABLED=non
  fi

  ask DATA_DIR "$(t 'Dossier des données' 'Data folder')" "${DATA_DIR:-$DEFAULT_DATA}"

  ask_choice choice "$(t 'Inscriptions' 'Registrations')" "$(policy_as_number "${REGISTRATION:-invitation}")" \
    "$(t 'ouvertes : qui connaît le serveur peut se créer un compte' 'open: anyone who knows the server may create an account')" \
    "$(t 'sur invitation : un code par compte, donné par vous' 'by invitation: one code per account, handed out by you')" \
    "$(t 'fermées : les comptes se créent sur la machine seulement' 'closed: accounts are created on the machine only')"
  case "$choice" in
    1) REGISTRATION=open ;;
    2) REGISTRATION=invitation ;;
    3) REGISTRATION=closed ;;
  esac

  default_account=${ADMIN_USER:-${SUDO_USER:-}}
  [[ $default_account == root || -z $default_account ]] && default_account="admin"
  ask ADMIN_USER "$(t 'Nom du premier compte' 'Name of the first account')" "$default_account"
  ask_secret ADMIN_PASSWORD "$(t "Mot de passe ($SHORTEST_PASSWORD caractères au moins)" "Password ($SHORTEST_PASSWORD characters at least)")"
}

policy_as_number() {
  case "$1" in
    open) echo 1 ;;
    closed) echo 3 ;;
    *) echo 2 ;;
  esac
}

policy_in_words() {
  case "$1" in
    open) t 'ouvertes' 'open' ;;
    closed) t 'fermées' 'closed' ;;
    *) t 'sur invitation' 'by invitation' ;;
  esac
}

# Whether this certificate was made with this key.
go_together() {
  local from_certificate from_key
  from_certificate=$(openssl x509 -noout -pubkey -in "$1" 2>/dev/null) || return 1
  from_key=$(openssl pkey -pubout -in "$2" 2>/dev/null) || return 1
  [[ $from_certificate == "$from_key" ]]
}

# The address to type in the application: the host, and the port when it
# is not the one the application assumes without being told.
address_to_type() {
  if [[ $API_PORT == 443 ]]; then echo "$PUBLIC_HOST"; else echo "$PUBLIC_HOST:$API_PORT"; fi
}

recap() {
  panel_open "$(t "Récapitulatif avant d'installer" 'Summary before installing')" "$C_ACCENT"
  panel_key "$(t 'Serveur' 'Server')" "$NAME, https://$(address_to_type)"
  case "$TLS_MODE" in
    1) panel_key "TLS" "$(t "mandataire inverse, le serveur écoute sur 127.0.0.1:$LOCAL_PORT" "reverse proxy, the server listens on 127.0.0.1:$LOCAL_PORT")" ;;
    2) panel_key "TLS" "$(t "auto-signé, pour $PUBLIC_HOST" "self-signed, for $PUBLIC_HOST")" ;;
    3) panel_key "TLS" "$(t "certificat fourni, $CERT_FILE" "provided certificate, $CERT_FILE")" ;;
  esac
  if [[ $TLS_MODE != 1 ]]; then
    panel_key "API" "TCP $API_PORT"
  fi
  panel_key "$(t 'Miroir et relais' 'Mirror and relay')" "UDP $RELAY_PORT$( [[ $RELAY_ENABLED == non ]] && t ', relais désactivé' ', relay disabled')"
  panel_key "$(t 'Données' 'Data')" "$DATA_DIR"
  panel_key "$(t 'Inscriptions' 'Registrations')" "$(policy_in_words "$REGISTRATION")"
  panel_key "$(t 'Premier compte' 'First account')" "$ADMIN_USER"
  panel_close
}

# ---- The installation steps -----------------------------------------------

install_the_packages() {
  local missing=()
  for package in $(needed_tools); do
    tool=$package
    [[ $package == build-essential ]] && tool=cc
    command -v "$tool" >/dev/null 2>&1 || missing+=("$package")
  done
  [[ -d /etc/ssl/certs ]] || missing+=("ca-certificates")
  if (( ${#missing[@]} > 0 )); then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -q
    apt-get install -y -q ca-certificates "${missing[@]}"
  fi
}

create_the_user_and_folders() {
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin --user-group "$SERVICE_USER"
  fi
  install -d -m 750 -o root -g "$SERVICE_USER" "$CONF_DIR" "$TLS_DIR"
  install -d -m 700 -o "$SERVICE_USER" -g "$SERVICE_USER" "$DATA_DIR"
}

# The binary, wherever it comes from, set down in place.
STAGED_BINARY=""

get_the_binary() {
  if [[ -n $GIVEN_BINARY ]]; then
    [[ -f $GIVEN_BINARY ]] || { echo "$GIVEN_BINARY : introuvable"; return 1; }
    STAGED_BINARY=$GIVEN_BINARY
    VERSION_INSTALLEE=$("$GIVEN_BINARY" --version 2>/dev/null | awk '{print $2}')
  elif [[ $FROM_SOURCE -eq 1 ]]; then
    build_from_source
  else
    download_the_binary
  fi
  hand_back STAGED_BINARY VERSION_INSTALLEE
}

download_the_binary() {
  local name folder tag base
  name=$(binary_name)
  folder=$(mktemp -d -p "$WORK_DIR")
  if [[ -n $WANTED_VERSION ]]; then
    tag=$WANTED_VERSION
  else
    tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
    [[ -n $tag ]] || { echo "aucune version publiée trouvée pour $REPO"; return 1; }
  fi
  base="https://github.com/$REPO/releases/download/$tag"
  curl -fsSL -o "$folder/$name" "$base/$name"
  curl -fsSL -o "$folder/$name.sha256" "$base/$name.sha256"
  (cd "$folder" && sha256sum -c --quiet "$name.sha256")
  chmod 755 "$folder/$name"
  STAGED_BINARY="$folder/$name"
  VERSION_INSTALLEE=${tag#v}
}

build_from_source() {
  local source
  if [[ -n $GIVEN_SOURCE ]]; then
    source=$GIVEN_SOURCE
  else
    source=$(mktemp -d -p "$WORK_DIR")
    git clone --depth 1 --branch "${SOURCE_BRANCH:-main}" "https://github.com/$REPO" "$source"
  fi
  (cd "$source" && cargo build --release -p zyr-server)
  STAGED_BINARY="$source/target/release/zyrdesk-server"
  VERSION_INSTALLEE=$("$STAGED_BINARY" --version 2>/dev/null | awk '{print $2}')
}

put_the_binary_in_place() {
  install -m 755 -o root -g root "$STAGED_BINARY" "$BIN"
}

write_the_configuration() {
  local listen tls relay_enabled
  if [[ $TLS_MODE == 1 ]]; then
    listen="127.0.0.1:$LOCAL_PORT"
    tls=""
  else
    listen="0.0.0.0:$API_PORT"
    tls="tls_cert = \"$TLS_DIR/server.crt\"
tls_key = \"$TLS_DIR/server.key\"
"
  fi
  if [[ $RELAY_ENABLED == oui ]]; then relay_enabled=true; else relay_enabled=false; fi
  cat >"$CONF" <<CONFIG
# Le serveur ZyrDesk. Écrit par install.sh, relu au démarrage :
# systemctl restart zyrdesk-server après une modification.
name = "$NAME"
data_dir = "$DATA_DIR"

[api]
listen = "$listen"
${tls}public_url = "https://$(address_to_type)"

[registration]
policy = "$REGISTRATION"

[relay]
enabled = $relay_enabled
listen = "0.0.0.0:$RELAY_PORT"
max_sessions = 10
max_kbps_per_session = 60000
connections_per_minute = 60

[limits]
login_attempts_per_minute = 10
CONFIG
  chown root:"$SERVICE_USER" "$CONF"
  chmod 640 "$CONF"
}

# The server's keys, made by it and its own: its signing key is born the
# first time its fingerprint is asked for.
generate_the_keys() {
  runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" fingerprint >/dev/null
}

# A leaf certificate, never an authority, on a P-256 key, for ten years,
# carrying the name and the addresses of this machine.
generate_the_certificate() {
  local cnf names=() rank=1
  cnf=$(mktemp -p "$WORK_DIR")
  if is_an_ip "$PUBLIC_HOST"; then
    names+=("IP.$rank = $PUBLIC_HOST"); rank=$((rank + 1))
  else
    names+=("DNS.1 = $PUBLIC_HOST")
  fi
  for ip in "$PUBLIC_IP" "$LOCAL_IP"; do
    if [[ -n $ip && $ip != "$PUBLIC_HOST" ]]; then
      names+=("IP.$rank = $ip"); rank=$((rank + 1))
    fi
  done
  {
    echo "[req]"
    echo "distinguished_name = dn"
    echo "x509_extensions = server"
    echo "prompt = no"
    echo "[dn]"
    echo "CN = $PUBLIC_HOST"
    echo "[server]"
    echo "basicConstraints = critical, CA:FALSE"
    echo "keyUsage = critical, digitalSignature"
    echo "extendedKeyUsage = serverAuth"
    echo "subjectAltName = @names"
    echo "[names]"
    printf '%s\n' "${names[@]}"
  } >"$cnf"
  openssl req -x509 -new -config "$cnf" -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
    -sha256 -days 3650 -keyout "$TLS_DIR/server.key" -out "$TLS_DIR/server.crt"
  rm -f "$cnf"
  set_the_certificate_rights
}

copy_the_certificate() {
  install -m 644 "$CERT_FILE" "$TLS_DIR/server.crt"
  install -m 640 "$KEY_FILE" "$TLS_DIR/server.key"
  set_the_certificate_rights
}

set_the_certificate_rights() {
  chown root:"$SERVICE_USER" "$TLS_DIR/server.crt" "$TLS_DIR/server.key"
  chmod 644 "$TLS_DIR/server.crt"
  chmod 640 "$TLS_DIR/server.key"
}

write_the_unit() {
  cat >"$UNIT" <<UNIT_FILE
[Unit]
Description=ZyrDesk server: accounts, rendezvous and relay
Documentation=https://github.com/$REPO/blob/main/docs/SERVER.md
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
ExecStart=$BIN --config $CONF run
WorkingDirectory=$DATA_DIR
Restart=on-failure
RestartSec=3
TimeoutStopSec=15
LimitNOFILE=65536
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX

[Install]
WantedBy=multi-user.target
UNIT_FILE
  # Hardening based on mounts does not survive an unprivileged container:
  # the Proxmox AppArmor profile refuses it, and the unit would not start
  # at all. Outside a container, it costs nothing.
  if [[ -z $CONTAINER ]]; then
    install -d -m 755 "$DROPIN_DIR"
    cat >"$DROPIN_DIR/10-hardening.conf" <<HARDENING
[Service]
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectControlGroups=true
ReadWritePaths=$DATA_DIR
HARDENING
  else
    rm -rf "$DROPIN_DIR"
  fi
  systemctl daemon-reload
}

start_the_service() {
  systemctl enable --now zyrdesk-server.service
  systemctl restart zyrdesk-server.service
}

# The server reaches itself the way a device would, once it listens: what
# the script waits for, with patience.
wait_for_the_server() {
  local attempt
  for attempt in $(seq 1 30); do
    if "$BIN" --config "$CONF" check >/dev/null 2>&1; then
      return 0
    fi
    if ! systemctl is-active --quiet zyrdesk-server.service; then
      echo "le service s'est arrêté :"
      journalctl -u zyrdesk-server.service -n 20 --no-pager
      return 1
    fi
    sleep 1
  done
  "$BIN" --config "$CONF" check
}

INVITATION_CODE=""

create_the_first_account() {
  if ! runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" user list | awk '{print $1}' | grep -qx "$ADMIN_USER"; then
    printf '%s\n' "$ADMIN_PASSWORD" | runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" user create "$ADMIN_USER" --password-stdin
  fi
  if [[ $REGISTRATION == invitation ]]; then
    INVITATION_CODE=$(runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" invite new)
    hand_back INVITATION_CODE
  fi
}

# The fingerprint the application asks to compare, read from the server.
server_fingerprint() {
  "$BIN" --config "$CONF" fingerprint 2>/dev/null | sed -n '2p' | sed 's/^ *//'
}

summary() {
  local fingerprint
  panel_open "$(t 'Serveur ZyrDesk installé' 'ZyrDesk server installed')" "$C_OK"
  panel_key "$(t "Adresse à taper dans l'application" 'Address to type in the application')" "$(address_to_type)"
  if [[ $TLS_MODE != 1 ]]; then
    fingerprint=$(server_fingerprint)
    panel_line "$(t "Empreinte du serveur (à comparer dans l'application) :" 'Server fingerprint (to compare in the application):')"
    panel_line "  $(bold "$fingerprint")"
  fi
  if [[ $TLS_MODE == 1 ]]; then
    panel_key "$(t 'Ports à renvoyer sur la box' 'Ports to forward on the router')" "$(t "TCP 443 vers le mandataire, UDP $RELAY_PORT vers $LOCAL_IP" "TCP 443 to the proxy, UDP $RELAY_PORT to $LOCAL_IP")"
  else
    panel_key "$(t "Ports à renvoyer sur la box vers $LOCAL_IP" "Ports to forward on the router to $LOCAL_IP")" "TCP $API_PORT, UDP $RELAY_PORT"
  fi
  panel_key "$(t 'Configuration' 'Configuration')" "$CONF"
  panel_key "$(t 'Données' 'Data')" "$DATA_DIR   $(t "(sauvegarde : zyrdesk-server backup <dossier>)" '(backup: zyrdesk-server backup <folder>)')"
  if [[ -n $INVITATION_CODE ]]; then
    panel_key "$(t "Code d'invitation pour un second compte" 'Invitation code for a second account')" "$INVITATION_CODE"
  fi
  panel_line "$(t "Les clés de $DATA_DIR/keys font l'identité du serveur : à sauvegarder." "The keys in $DATA_DIR/keys are the server's identity: back them up.")"
  panel_close
  if [[ $TLS_MODE == 1 ]]; then
    proxy_panel
  fi
  info "$(t 'Relancer ce script met à jour ou reconfigure. « zyrdesk-server status » dit où il en est.' 'Running this script again updates or reconfigures. "zyrdesk-server status" says where it stands.')"
}

# The exact lines of a reverse proxy: the live channel is a WebSocket,
# which wants Upgrade and Connection passed on and a long read timeout.
proxy_panel() {
  panel_open "$(t 'Le mandataire inverse, à configurer par vous' 'The reverse proxy, to configure yourself')" "$C_WARNING"
  panel_line "$(t "Caddy, dans le Caddyfile :" 'Caddy, in the Caddyfile:')"
  panel_line "  $PUBLIC_HOST {"
  panel_line "      reverse_proxy 127.0.0.1:$LOCAL_PORT"
  panel_line "  }"
  panel_line ""
  panel_line "$(t "nginx, dans le bloc server de $PUBLIC_HOST :" "nginx, in the server block of $PUBLIC_HOST:")"
  panel_line "  location / {"
  panel_line "      proxy_pass http://127.0.0.1:$LOCAL_PORT;"
  panel_line "      proxy_http_version 1.1;"
  panel_line "      proxy_set_header Upgrade \$http_upgrade;"
  panel_line "      proxy_set_header Connection \"upgrade\";"
  panel_line "      proxy_set_header Host \$host;"
  panel_line "      proxy_set_header X-Forwarded-For \$remote_addr;"
  panel_line "      proxy_read_timeout 3600s;"
  panel_line "  }"
  panel_close
}

# ---- The paths through the script ------------------------------------------

fresh_install() {
  panel_open "$(t "Où l'on est" 'Where we are')" "$C_SOFT"
  panel_line "$(t 'Machine' 'Machine') : $(bold "$(hostname)") ($OS_ID $OS_VERSION$( [[ -n $CONTAINER ]] && echo ", $(t 'conteneur' 'container') $CONTAINER$( [[ $UNPRIVILEGED -eq 1 ]] && t ' non privilégié' ' unprivileged')"))"
  panel_line "$(t 'Adresse' 'Address') : $(bold "${LOCAL_IP:-?}")$( [[ -n $PUBLIC_IP ]] && echo ", $(t 'publique' 'public') $(bold "$PUBLIC_IP")")"
  panel_line "$(t 'Ce script installe le serveur ZyrDesk : comptes, mise en relation, relais.' 'This script installs the ZyrDesk server: accounts, rendezvous, relay.')"
  panel_close
  echo

  ask_the_questions
  recap
  ask_yes "$(t "Lancer l'installation maintenant ?" 'Start the installation now?')" oui || exit 0
  echo

  step "$(t 'Paquets nécessaires' 'Required packages')" install_the_packages
  step "$(t "Utilisateur $SERVICE_USER et dossiers" "User $SERVICE_USER and folders")" create_the_user_and_folders
  step "$(t 'Obtention de zyrdesk-server' 'Getting zyrdesk-server')" get_the_binary
  step "$(t "Installation de zyrdesk-server ${VERSION_INSTALLEE:-}" "Installing zyrdesk-server ${VERSION_INSTALLEE:-}")" put_the_binary_in_place
  step "$(t 'Configuration écrite' 'Configuration written')" write_the_configuration
  # The certificate before the keys, not after: what creates the server's
  # signing key is asking it for its fingerprint, and that question reads
  # the certificate along the way. In the other order, it fails on a
  # certificate that does not exist yet.
  case "$TLS_MODE" in
    2) step "$(t 'Certificat auto-signé' 'Self-signed certificate')" generate_the_certificate ;;
    3) step "$(t 'Certificat copié' 'Certificate copied')" copy_the_certificate ;;
  esac
  step "$(t 'Clés du serveur' 'Server keys')" generate_the_keys
  step "$(t 'Service systemd installé et démarré' 'systemd service installed and started')" write_the_unit
  step "$(t 'Démarrage' 'Starting')" start_the_service
  step "$(t 'Le serveur répond' 'The server answers')" wait_for_the_server
  step "$(t "Compte $ADMIN_USER créé" "Account $ADMIN_USER created")" create_the_first_account
  save_the_state
  summary
}

update_the_server() {
  load_the_state
  step "$(t 'Obtention de zyrdesk-server' 'Getting zyrdesk-server')" get_the_binary
  step "$(t 'Arrêt du service' 'Stopping the service')" systemctl stop zyrdesk-server.service
  step "$(t "Installation de zyrdesk-server ${VERSION_INSTALLEE:-}" "Installing zyrdesk-server ${VERSION_INSTALLEE:-}")" put_the_binary_in_place
  step "$(t 'Service systemd' 'systemd service')" write_the_unit
  step "$(t 'Démarrage' 'Starting')" start_the_service
  step "$(t 'Le serveur répond' 'The server answers')" wait_for_the_server
  save_the_state
  ok "$(t "Mis à jour en ${VERSION_INSTALLEE:-?}." "Updated to ${VERSION_INSTALLEE:-?}.")"
}

reconfigure() {
  load_the_state
  ask_the_questions
  recap
  ask_yes "$(t 'Appliquer cette configuration ?' 'Apply this configuration?')" oui || exit 0
  echo
  step "$(t 'Configuration écrite' 'Configuration written')" write_the_configuration
  case "$TLS_MODE" in
    2) [[ -f $TLS_DIR/server.crt ]] || step "$(t 'Certificat auto-signé' 'Self-signed certificate')" generate_the_certificate ;;
    3) step "$(t 'Certificat copié' 'Certificate copied')" copy_the_certificate ;;
  esac
  step "$(t 'Service systemd' 'systemd service')" write_the_unit
  step "$(t 'Redémarrage' 'Restarting')" start_the_service
  step "$(t 'Le serveur répond' 'The server answers')" wait_for_the_server
  step "$(t "Compte $ADMIN_USER" "Account $ADMIN_USER")" create_the_first_account
  save_the_state
  summary
}

uninstall() {
  load_the_state
  panel_open "$(t 'Retirer le serveur' 'Remove the server')" "$C_WARNING"
  panel_line "$(t 'Premier palier : le service est arrêté et retiré, le programme effacé.' 'First stage: the service is stopped and removed, the program erased.')"
  panel_line "$(t "Les données et les clés restent dans ${DATA_DIR:-$DEFAULT_DATA} et la configuration dans $CONF_DIR." "Data and keys stay in ${DATA_DIR:-$DEFAULT_DATA}, the configuration in $CONF_DIR.")"
  panel_close
  ask_yes "$(t 'Retirer le service et le programme ?' 'Remove the service and the program?')" non || exit 0
  systemctl disable --now zyrdesk-server.service >/dev/null 2>&1 || true
  rm -f "$UNIT"
  rm -rf "$DROPIN_DIR"
  systemctl daemon-reload
  rm -f "$BIN"
  ok "$(t 'Service et programme retirés.' 'Service and program removed.')"

  panel_open "$(t 'Second palier : tout effacer' 'Second stage: erase everything')" "$C_WARNING"
  panel_line "$(t "Efface ${DATA_DIR:-$DEFAULT_DATA} (comptes, appareils, clés du serveur) et $CONF_DIR." "Erases ${DATA_DIR:-$DEFAULT_DATA} (accounts, devices, server keys) and $CONF_DIR.")"
  panel_line "$(t "Les appareils rattachés perdront leur compte et devront se rattacher à nouveau. Rien ne se récupère ensuite." 'Attached devices lose their account and must attach again. Nothing is recoverable afterwards.')"
  panel_close
  if confirm_in_full; then
    rm -rf "${DATA_DIR:-$DEFAULT_DATA}" "$CONF_DIR"
    userdel "$SERVICE_USER" >/dev/null 2>&1 || true
    ok "$(t 'Données, clés et configuration effacées.' 'Data, keys and configuration erased.')"
  else
    info "$(t 'Données et configuration gardées.' 'Data and configuration kept.')"
  fi
}

existing_installation_menu() {
  local version choice
  version=$("$BIN" --version 2>/dev/null | awk '{print $2}' || true)
  panel_open "$(t 'Un serveur ZyrDesk est déjà installé' 'A ZyrDesk server is already installed')" "$C_SOFT"
  panel_key "$(t 'Version' 'Version')" "${version:-?}"
  panel_key "$(t 'Configuration' 'Configuration')" "$CONF"
  panel_key "$(t 'Service' 'Service')" "$(systemctl is-active zyrdesk-server.service 2>/dev/null || true)"
  panel_close
  ask_choice choice "$(t 'Que faire ?' 'What to do?')" 1 \
    "$(t 'Mettre à jour vers la version publiée' 'Update to the published version')" \
    "$(t 'Reconfigurer (les questions, avec les réponses d'"'"'avant)' 'Reconfigure (the questions, with the previous answers)')" \
    "$(t "Afficher l'état" 'Show the state')" \
    "$(t 'Désinstaller' 'Uninstall')" \
    "$(t 'Ne rien faire' 'Do nothing')"
  case "$choice" in
    1) update_the_server ;;
    2) reconfigure ;;
    3) "$BIN" --config "$CONF" status ;;
    4) uninstall ;;
    5) exit 0 ;;
  esac
}

# ---- The options -----------------------------------------------------------

GIVEN_BINARY=""
FROM_SOURCE=0
GIVEN_SOURCE=""
SOURCE_BRANCH=""
WANTED_VERSION=""

usage() {
  cat <<HELP
$(t 'Installe le serveur ZyrDesk sur cette machine.' 'Installs the ZyrDesk server on this machine.')

  bash install.sh [options]

$(t 'Options' 'Options') :
  --binary FILE       $(t 'un binaire zyrdesk-server déjà obtenu, plutôt que le télécharger' 'a zyrdesk-server binary already at hand, rather than downloading it')
  --version vX.Y.Z    $(t 'cette version publiée plutôt que la dernière' 'that published version rather than the latest')
  --from-source       $(t 'compiler sur place (cargo requis), pour une architecture sans binaire' 'build here (cargo required), for an architecture without a binary')
  --source DIR        $(t 'avec --from-source : ce dépôt déjà cloné' 'with --from-source: that repository, already cloned')
  --branch NAME       $(t 'avec --from-source : cette branche du dépôt (main sinon)' 'with --from-source: that branch of the repository (main otherwise)')
  --lang fr|en        $(t 'la langue du script (celle de la machine sinon)' "the script's language (the machine's otherwise)")
  --help              $(t 'ceci' 'this')
HELP
}

while (( $# > 0 )); do
  case "$1" in
    --binary) GIVEN_BINARY=$(readlink -f "$2"); shift 2 ;;
    --version) WANTED_VERSION=$2; shift 2 ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --source) GIVEN_SOURCE=$(readlink -f "$2"); FROM_SOURCE=1; shift 2 ;;
    --branch) SOURCE_BRANCH=$2; shift 2 ;;
    --lang) SCRIPT_LANG=$2; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) fail "$(t "option inconnue : $1" "unknown option: $1")"; usage; exit 1 ;;
  esac
done

# ---- The run ---------------------------------------------------------------

banner
if [[ $INTERACTIVE -eq 0 ]]; then
  fail "$(t 'Ce script pose des questions : lancez-le dans un terminal.' 'This script asks questions: run it in a terminal.')"
  exit 1
fi
survey_the_machine
check_the_prerequisites
# An existing installation, and not a started one: the systemd unit and
# the record of the answers are only written once everything else is in
# place. The program and the configuration alone are what an interrupted
# installation leaves behind, and offering "update" or "reconfigure" then
# would be offering to repair what does not exist yet: an installation is
# what is needed, and starting it again from the beginning only costs the
# questions.
if [[ -f $STATE || -f $UNIT ]]; then
  existing_installation_menu
else
  fresh_install
fi
