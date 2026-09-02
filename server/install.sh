#!/usr/bin/env bash
# Installe, met à jour, reconfigure ou retire le serveur ZyrDesk sur un
# Debian 12 ou 13, dans un conteneur LXC de Proxmox ou sur une machine
# ordinaire, sous systemd.
#
#   bash install.sh                 questions, puis installation
#   bash install.sh --help          les options
#
# Ce qu'il fait et ce qu'il ne fait pas est écrit dans docs/SERVER.md,
# section 10. Il ne configure pas de mandataire inverse, n'ouvre pas la
# box, n'installe pas de pare-feu, et ne touche jamais à une installation
# en place sans l'avoir dit et fait confirmer.

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

# ---- La langue -------------------------------------------------------------

LANGUE="en"
case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
  fr*) LANGUE="fr" ;;
esac

# Le mot français, ou l'anglais quand la machine n'est pas en français.
t() {
  if [[ $LANGUE == fr ]]; then printf '%s' "$1"; else printf '%s' "$2"; fi
}

# ---- Les couleurs ----------------------------------------------------------
#
# La palette de design.css, en couleurs vraies quand le terminal les
# annonce, en 256 couleurs sinon ; rien du tout hors d'un terminal ou
# sous NO_COLOR.

INTERACTIF=0
[[ -t 0 && -t 1 ]] && INTERACTIF=1

if [[ $INTERACTIF -eq 1 && -z ${NO_COLOR:-} && ${TERM:-dumb} != dumb ]]; then
  if [[ ${COLORTERM:-} == truecolor || ${COLORTERM:-} == 24bit ]]; then
    C_ACCENT=$'\e[38;2;239;181;54m'
    C_VIF=$'\e[38;2;248;205;106m'
    C_SOURD=$'\e[38;2;106;74;18m'
    C_DOUX=$'\e[38;2;160;167;184m'
    C_FAIBLE=$'\e[38;2;107;115;133m'
    C_OK=$'\e[38;2;52;211;153m'
    C_ATTENTION=$'\e[38;2;249;115;22m'
    C_ERREUR=$'\e[38;2;248;113;113m'
  else
    C_ACCENT=$'\e[38;5;214m'
    C_VIF=$'\e[38;5;221m'
    C_SOURD=$'\e[38;5;94m'
    C_DOUX=$'\e[38;5;248m'
    C_FAIBLE=$'\e[38;5;243m'
    C_OK=$'\e[38;5;78m'
    C_ATTENTION=$'\e[38;5;208m'
    C_ERREUR=$'\e[38;5;203m'
  fi
  C_GRAS=$'\e[1m'
  C_RESET=$'\e[0m'
else
  C_ACCENT="" C_VIF="" C_SOURD="" C_DOUX="" C_FAIBLE="" C_OK="" C_ATTENTION="" C_ERREUR=""
  C_GRAS="" C_RESET=""
fi

# Une valeur, un chemin, un nom : en gras, jamais en couleur.
gras() { printf '%s%s%s' "$C_GRAS" "$1" "$C_RESET"; }

info() { printf '%s›%s %s\n' "$C_VIF" "$C_RESET" "$1"; }
ok()   { printf '%s✓%s %s\n' "$C_OK" "$C_RESET" "$1"; }
warn() { printf '%s⚠%s %s\n' "$C_ATTENTION" "$C_RESET" "$1"; }
fail() { printf '%s✗%s %s\n' "$C_ERREUR" "$C_RESET" "$1" >&2; }

# Un panneau ouvert : son titre dans la couleur de son sens, ses lignes,
# puis le coin qui le ferme.
panneau_ouvre() { printf '\n%s┌ %s%s\n' "$2" "$1" "$C_RESET"; }
panneau_ligne() { printf '%s│%s %s\n' "$C_FAIBLE" "$C_RESET" "$1"; }
panneau_ferme() { printf '%s└%s\n' "$C_FAIBLE" "$C_RESET"; }

# Deux colonnes alignées dans un panneau : la clé, puis la valeur en gras.
panneau_cle() { panneau_ligne "$(printf '%-16s: %s' "$1" "$(gras "$2")")"; }

# ---- La bannière -----------------------------------------------------------

banniere() {
  local largeur
  largeur=$(tput cols 2>/dev/null || echo 60)
  (( largeur > 72 )) && largeur=72
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
  printf '  %s%s%s   %s· par Victor-root%s\n' "$C_VIF" "$(t 'Serveur ZyrDesk · installation' 'ZyrDesk server · installation')" "$C_RESET" "$C_FAIBLE" "$C_RESET"
  printf '%s' "$C_SOURD"
  printf '─%.0s' $(seq 1 "$largeur")
  printf '%s\n' "$C_RESET"
}

# ---- Les questions ---------------------------------------------------------

# Une question, son défaut entre crochets, et ce qui est répondu ou le
# défaut quand rien ne l'est.
demande() {
  local __variable=$1 libelle=$2 defaut=${3:-} reponse
  while true; do
    if [[ -n $defaut ]]; then
      printf '%s?%s %s %s[%s]%s : ' "$C_VIF" "$C_RESET" "$libelle" "$C_DOUX" "$defaut" "$C_RESET"
    else
      printf '%s?%s %s : ' "$C_VIF" "$C_RESET" "$libelle"
    fi
    IFS= read -r -e reponse || { echo; exit 1; }
    reponse=${reponse:-$defaut}
    if [[ -n $reponse ]]; then
      printf -v "$__variable" '%s' "$reponse"
      return 0
    fi
    warn "$(t 'Il faut une réponse.' 'An answer is needed.')"
  done
}

# Une question dont la réponse ne s'affiche pas, posée deux fois.
demande_secret() {
  local __variable=$1 libelle=$2 premier second
  while true; do
    printf '%s?%s %s : ' "$C_VIF" "$C_RESET" "$libelle"
    IFS= read -r -s premier || { echo; exit 1; }
    echo
    if (( ${#premier} < SHORTEST_PASSWORD )); then
      warn "$(t "$SHORTEST_PASSWORD caractères au moins." "$SHORTEST_PASSWORD characters at least.")"
      continue
    fi
    printf '%s?%s %s : ' "$C_VIF" "$C_RESET" "$(t 'Encore une fois, pour être sûr' 'Once more, to be sure')"
    IFS= read -r -s second || { echo; exit 1; }
    echo
    if [[ $premier == "$second" ]]; then
      printf -v "$__variable" '%s' "$premier"
      return 0
    fi
    warn "$(t 'Les deux ne sont pas pareils.' 'The two differ.')"
  done
}

# Oui ou non, Entrée valant le défaut, écrit en toutes lettres.
demande_oui() {
  local libelle=$1 defaut=${2:-oui} reponse indication
  if [[ $defaut == oui ]]; then
    indication=$(t '[Entrée=oui / non]' '[Enter=yes / no]')
  else
    indication=$(t '[oui / Entrée=non]' '[yes / Enter=no]')
  fi
  while true; do
    printf '%s?%s %s %s%s%s : ' "$C_VIF" "$C_RESET" "$libelle" "$C_DOUX" "$indication" "$C_RESET"
    IFS= read -r -e reponse || { echo; exit 1; }
    case "${reponse,,}" in
      "") [[ $defaut == oui ]] && return 0 || return 1 ;;
      o|oui|y|yes) return 0 ;;
      n|non|no) return 1 ;;
    esac
    warn "$(t 'Répondez oui ou non.' 'Answer yes or no.')"
  done
}

# Un choix numéroté parmi ceux qui suivent.
demande_choix() {
  local __variable=$1 libelle=$2 defaut=$3 reponse
  shift 3
  printf '%s?%s %s :\n' "$C_VIF" "$C_RESET" "$libelle"
  local rang=1
  for option in "$@"; do
    printf '   %s%d)%s %s\n' "$C_DOUX" "$rang" "$C_RESET" "$option"
    rang=$((rang + 1))
  done
  while true; do
    printf '%s?%s %s %s[%s]%s : ' "$C_VIF" "$C_RESET" "$(t 'Votre choix' 'Your choice')" "$C_DOUX" "$defaut" "$C_RESET"
    IFS= read -r -e reponse || { echo; exit 1; }
    reponse=${reponse:-$defaut}
    if [[ $reponse =~ ^[0-9]+$ ]] && (( reponse >= 1 && reponse <= $# )); then
      printf -v "$__variable" '%s' "$reponse"
      return 0
    fi
    warn "$(t "Un nombre entre 1 et $#." "A number between 1 and $#.")"
  done
}

# Ce qui ne se défait pas se confirme en tapant le mot entier.
confirme_en_toutes_lettres() {
  local mot reponse
  mot=$(t 'oui' 'yes')
  printf '%s?%s %s %s[%s]%s : ' "$C_ATTENTION" "$C_RESET" "$(t "Tapez « $mot » pour continuer" "Type \"$mot\" to continue")" "$C_DOUX" "$(t 'autre chose annule' 'anything else cancels')" "$C_RESET"
  IFS= read -r -e reponse || { echo; exit 1; }
  [[ $reponse == "$mot" ]]
}

# ---- Les étapes ------------------------------------------------------------

# Une étape derrière une roue, réécrite en ✓ ou ✗ ; la sortie de ce
# qu'elle a fait n'est montrée qu'en cas d'échec.
etape() {
  local libelle=$1 journal statut=0
  shift
  journal=$(mktemp)
  if [[ $INTERACTIF -eq 1 ]]; then
    ( "$@" ) >"$journal" 2>&1 &
    local pid=$! roue='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏' i=0
    while kill -0 "$pid" 2>/dev/null; do
      printf '\r%s%s%s %s' "$C_VIF" "${roue:i%10:1}" "$C_RESET" "$libelle"
      i=$((i + 1))
      sleep 0.1
    done
    wait "$pid" || statut=$?
    printf '\r\033[K'
  else
    ( "$@" ) >"$journal" 2>&1 || statut=$?
  fi
  if [[ $statut -eq 0 ]]; then
    ok "$libelle"
    rm -f "$journal"
    return 0
  fi
  fail "$libelle"
  sed 's/^/    /' "$journal" >&2
  rm -f "$journal"
  exit 1
}

# ---- Ce qui est su de la machine -------------------------------------------

ARCH=""
OS_ID=""
OS_VERSION=""
CONTENEUR=""
NON_PRIVILEGIE=0
IP_LOCALE=""
IP_PUBLIQUE=""

releve_la_machine() {
  ARCH=$(uname -m)
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    OS_ID=${ID:-}
    OS_VERSION=${VERSION_ID:-}
  fi
  CONTENEUR=$(systemd-detect-virt --container 2>/dev/null || true)
  [[ $CONTENEUR == none ]] && CONTENEUR=""
  if [[ -r /proc/self/uid_map ]] && ! grep -qE '^\s*0\s+0\s+4294967295$' /proc/self/uid_map; then
    NON_PRIVILEGIE=1
  fi
  IP_LOCALE=$(hostname -I 2>/dev/null | awk '{print $1}')
  IP_PUBLIQUE=$(curl -fsS4 --max-time 5 https://api.ipify.org 2>/dev/null || true)
  [[ $IP_PUBLIQUE =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || IP_PUBLIQUE=""
}

# Une adresse en 100.64.0.0/10 : la box n'a pas d'adresse publique à
# elle, et aucun port ne se renvoie vers cette machine depuis Internet.
est_cgnat() {
  [[ $1 =~ ^100\.([0-9]+)\. ]] && (( BASH_REMATCH[1] >= 64 && BASH_REMATCH[1] <= 127 ))
}

est_une_ip() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# Le nom du binaire publié pour cette architecture : x86_64 seule, celle
# des conteneurs de Proxmox ; les autres compilent sur place.
nom_du_binaire() {
  case "$ARCH" in
    x86_64) echo "zyrdesk-server-x86_64-linux-musl" ;;
    *) return 1 ;;
  esac
}

# ---- Les vérifications -----------------------------------------------------

verifie_les_prealables() {
  local manques=0
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
    demande_oui "$(t 'Continuer quand même ?' 'Continue anyway?')" non || exit 1
  fi
  if ! nom_du_binaire >/dev/null; then
    warn "$(t "Aucun binaire n'est publié pour $ARCH : il faudra --from-source." "No binary is published for $ARCH: --from-source is needed.")"
    [[ $DEPUIS_LA_SOURCE -eq 1 ]] || exit 1
  fi
  for outil in curl openssl; do
    if ! command -v "$outil" >/dev/null 2>&1; then
      info "$(t "$outil manque : il sera installé (apt-get install $outil)." "$outil is missing: it will be installed (apt-get install $outil).")"
      manques=1
    fi
  done
  if [[ $manques -eq 1 ]] && ! command -v apt-get >/dev/null 2>&1; then
    fail "$(t 'apt-get est introuvable : installez curl et openssl, puis relancez.' 'apt-get is missing: install curl and openssl, then run again.')"
    exit 1
  fi
}

# ---- Les réponses gardées --------------------------------------------------

NAME="" PUBLIC_HOST="" TLS_MODE="" API_PORT="" LOCAL_PORT="" CERT_FILE="" KEY_FILE=""
RELAY_ENABLED="" RELAY_PORT="" DATA_DIR="" REGISTRATION="" ADMIN_USER="" ADMIN_PASSWORD=""
VERSION_INSTALLEE=""

charge_l_etat() {
  if [[ -r $STATE ]]; then
    # shellcheck disable=SC1090
    . "$STATE"
  fi
}

# Tout sauf le mot de passe, qui ne s'écrit nulle part.
enregistre_l_etat() {
  {
    echo "# Les réponses de la dernière installation, proposées en défaut à la relance."
    for cle in NAME PUBLIC_HOST TLS_MODE API_PORT LOCAL_PORT CERT_FILE KEY_FILE RELAY_ENABLED RELAY_PORT DATA_DIR REGISTRATION ADMIN_USER VERSION_INSTALLEE; do
      printf '%s=%q\n' "$cle" "${!cle}"
    done
  } >"$STATE"
  chmod 600 "$STATE"
}

# ---- Les questions de l'installation ---------------------------------------

pose_les_questions() {
  local choix defaut_hote defaut_nom defaut_compte

  defaut_nom=${NAME:-$(t 'Maison' 'Home')}
  demande NAME "$(t 'Nom affiché du serveur' 'Name the application shows for this server')" "$defaut_nom"

  defaut_hote=${PUBLIC_HOST:-${IP_PUBLIQUE:-$IP_LOCALE}}
  demande PUBLIC_HOST "$(t 'Adresse publique (domaine ou IP)' 'Public address (domain or IP)')" "$defaut_hote"
  if est_une_ip "$PUBLIC_HOST" && est_cgnat "$PUBLIC_HOST"; then
    warn "$(t "$PUBLIC_HOST est une adresse partagée (CGNAT) : aucun port ne se renvoie vers cette machine depuis Internet. Un domaine ou un VPN sera nécessaire." "$PUBLIC_HOST is a shared address (CGNAT): no port can be forwarded to this machine from the Internet. A domain or a VPN will be needed.")"
  fi

  demande_choix choix "$(t "Chiffrement de l'API" 'Encryption of the API')" "${TLS_MODE:-2}" \
    "$(t "J'ai déjà un mandataire inverse avec un certificat valide" 'I already have a reverse proxy with a valid certificate')" \
    "$(t "Générer un certificat auto-signé (à confirmer dans l'application)" 'Generate a self-signed certificate (to confirm in the application)')" \
    "$(t "J'ai mes propres fichiers de certificat" 'I have my own certificate files')"
  TLS_MODE=$choix
  case "$TLS_MODE" in
    1)
      demande LOCAL_PORT "$(t 'Port de boucle locale que le mandataire renvoie vers le serveur' 'Loopback port the proxy forwards to the server')" "${LOCAL_PORT:-8443}"
      API_PORT=443
      CERT_FILE="" KEY_FILE=""
      ;;
    2)
      demande API_PORT "$(t "Port TCP de l'API" 'TCP port of the API')" "${API_PORT:-443}"
      LOCAL_PORT="" CERT_FILE="" KEY_FILE=""
      ;;
    3)
      demande API_PORT "$(t "Port TCP de l'API" 'TCP port of the API')" "${API_PORT:-443}"
      LOCAL_PORT=""
      while true; do
        demande CERT_FILE "$(t 'Certificat (chaîne complète, PEM)' 'Certificate (full chain, PEM)')" "${CERT_FILE:-}"
        demande KEY_FILE "$(t 'Clé privée (PEM)' 'Private key (PEM)')" "${KEY_FILE:-}"
        if [[ ! -r $CERT_FILE ]]; then warn "$(t "$CERT_FILE ne se lit pas." "$CERT_FILE cannot be read.")"; continue; fi
        if [[ ! -r $KEY_FILE ]]; then warn "$(t "$KEY_FILE ne se lit pas." "$KEY_FILE cannot be read.")"; continue; fi
        if ! vont_ensemble "$CERT_FILE" "$KEY_FILE"; then
          warn "$(t 'Ce certificat et cette clé ne vont pas ensemble.' 'This certificate and this key do not match.')"
          continue
        fi
        break
      done
      ;;
  esac

  # Le miroir répond sur ce port quoi qu'il arrive : c'est lui qui dit à
  # un appareil son adresse vue de l'extérieur, et donc ce qui rend le
  # direct possible. Le relais, lui, se débraye.
  demande RELAY_PORT "$(t "Port UDP du miroir et du relais" 'UDP port of the mirror and the relay')" "${RELAY_PORT:-443}"
  if demande_oui "$(t "Activer le relais (secours quand aucun chemin direct n'existe)" 'Enable the relay (fallback when no direct path exists)')" "${RELAY_ENABLED:-oui}"; then
    RELAY_ENABLED=oui
  else
    RELAY_ENABLED=non
  fi

  demande DATA_DIR "$(t 'Dossier des données' 'Data folder')" "${DATA_DIR:-$DEFAULT_DATA}"

  demande_choix choix "$(t 'Inscriptions' 'Registrations')" "$(politique_en_chiffre "${REGISTRATION:-invitation}")" \
    "$(t 'ouvertes : qui connaît le serveur peut se créer un compte' 'open: anyone who knows the server may create an account')" \
    "$(t 'sur invitation : un code par compte, donné par vous' 'by invitation: one code per account, handed out by you')" \
    "$(t 'fermées : les comptes se créent sur la machine seulement' 'closed: accounts are created on the machine only')"
  case "$choix" in
    1) REGISTRATION=open ;;
    2) REGISTRATION=invitation ;;
    3) REGISTRATION=closed ;;
  esac

  defaut_compte=${ADMIN_USER:-${SUDO_USER:-}}
  [[ $defaut_compte == root || -z $defaut_compte ]] && defaut_compte="admin"
  demande ADMIN_USER "$(t 'Nom du premier compte' 'Name of the first account')" "$defaut_compte"
  demande_secret ADMIN_PASSWORD "$(t "Mot de passe ($SHORTEST_PASSWORD caractères au moins)" "Password ($SHORTEST_PASSWORD characters at least)")"
}

politique_en_chiffre() {
  case "$1" in
    open) echo 1 ;;
    closed) echo 3 ;;
    *) echo 2 ;;
  esac
}

politique_en_mots() {
  case "$1" in
    open) t 'ouvertes' 'open' ;;
    closed) t 'fermées' 'closed' ;;
    *) t 'sur invitation' 'by invitation' ;;
  esac
}

# Si ce certificat a été fait avec cette clé.
vont_ensemble() {
  local du_certificat de_la_cle
  du_certificat=$(openssl x509 -noout -pubkey -in "$1" 2>/dev/null) || return 1
  de_la_cle=$(openssl pkey -pubout -in "$2" 2>/dev/null) || return 1
  [[ $du_certificat == "$de_la_cle" ]]
}

# L'adresse à taper dans l'application : l'hôte, et le port quand ce
# n'est pas celui que l'application prend sans qu'on le dise.
adresse_a_taper() {
  if [[ $API_PORT == 443 ]]; then echo "$PUBLIC_HOST"; else echo "$PUBLIC_HOST:$API_PORT"; fi
}

recapitule() {
  panneau_ouvre "$(t "Récapitulatif avant d'installer" 'Summary before installing')" "$C_ACCENT"
  panneau_cle "$(t 'Serveur' 'Server')" "$NAME, https://$(adresse_a_taper)"
  case "$TLS_MODE" in
    1) panneau_cle "TLS" "$(t "mandataire inverse, le serveur écoute sur 127.0.0.1:$LOCAL_PORT" "reverse proxy, the server listens on 127.0.0.1:$LOCAL_PORT")" ;;
    2) panneau_cle "TLS" "$(t "auto-signé, pour $PUBLIC_HOST" "self-signed, for $PUBLIC_HOST")" ;;
    3) panneau_cle "TLS" "$(t "certificat fourni, $CERT_FILE" "provided certificate, $CERT_FILE")" ;;
  esac
  if [[ $TLS_MODE != 1 ]]; then
    panneau_cle "API" "TCP $API_PORT"
  fi
  panneau_cle "$(t 'Miroir et relais' 'Mirror and relay')" "UDP $RELAY_PORT$( [[ $RELAY_ENABLED == non ]] && t ', relais désactivé' ', relay disabled')"
  panneau_cle "$(t 'Données' 'Data')" "$DATA_DIR"
  panneau_cle "$(t 'Inscriptions' 'Registrations')" "$(politique_en_mots "$REGISTRATION")"
  panneau_cle "$(t 'Premier compte' 'First account')" "$ADMIN_USER"
  panneau_ferme
}

# ---- Les étapes de l'installation ------------------------------------------

installe_les_paquets() {
  local manquants=()
  for outil in curl openssl; do
    command -v "$outil" >/dev/null 2>&1 || manquants+=("$outil")
  done
  [[ -d /etc/ssl/certs ]] || manquants+=("ca-certificates")
  if (( ${#manquants[@]} > 0 )); then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -q
    apt-get install -y -q ca-certificates "${manquants[@]}"
  fi
}

cree_l_utilisateur_et_les_dossiers() {
  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin --user-group "$SERVICE_USER"
  fi
  install -d -m 750 -o root -g "$SERVICE_USER" "$CONF_DIR" "$TLS_DIR"
  install -d -m 700 -o "$SERVICE_USER" -g "$SERVICE_USER" "$DATA_DIR"
}

# Le binaire, d'où qu'il vienne, déposé en place.
BINAIRE_TEMPORAIRE=""

obtient_le_binaire() {
  if [[ -n $BINAIRE_FOURNI ]]; then
    [[ -f $BINAIRE_FOURNI ]] || { echo "$BINAIRE_FOURNI : introuvable"; return 1; }
    BINAIRE_TEMPORAIRE=$BINAIRE_FOURNI
    VERSION_INSTALLEE=$("$BINAIRE_FOURNI" --version 2>/dev/null | awk '{print $2}')
    return 0
  fi
  if [[ $DEPUIS_LA_SOURCE -eq 1 ]]; then
    compile_depuis_la_source
    return
  fi
  telecharge_le_binaire
}

telecharge_le_binaire() {
  local nom dossier tag base
  nom=$(nom_du_binaire)
  dossier=$(mktemp -d)
  if [[ -n $VERSION_VOULUE ]]; then
    tag=$VERSION_VOULUE
  else
    tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
    [[ -n $tag ]] || { echo "aucune version publiée trouvée pour $REPO"; return 1; }
  fi
  base="https://github.com/$REPO/releases/download/$tag"
  curl -fsSL -o "$dossier/$nom" "$base/$nom"
  curl -fsSL -o "$dossier/$nom.sha256" "$base/$nom.sha256"
  (cd "$dossier" && sha256sum -c --quiet "$nom.sha256")
  chmod 755 "$dossier/$nom"
  BINAIRE_TEMPORAIRE="$dossier/$nom"
  VERSION_INSTALLEE=${tag#v}
}

compile_depuis_la_source() {
  local source
  if [[ -n $SOURCE_FOURNIE ]]; then
    source=$SOURCE_FOURNIE
  else
    source=$(mktemp -d)
    git clone --depth 1 --branch "${BRANCHE_SOURCE:-main}" "https://github.com/$REPO" "$source"
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo est introuvable : installez Rust (https://rustup.rs), puis relancez"
    return 1
  fi
  (cd "$source" && cargo build --release -p zyr-server)
  BINAIRE_TEMPORAIRE="$source/target/release/zyrdesk-server"
  VERSION_INSTALLEE=$("$BINAIRE_TEMPORAIRE" --version 2>/dev/null | awk '{print $2}')
}

pose_le_binaire() {
  install -m 755 -o root -g root "$BINAIRE_TEMPORAIRE" "$BIN"
}

ecrit_la_configuration() {
  local listen tls relais
  if [[ $TLS_MODE == 1 ]]; then
    listen="127.0.0.1:$LOCAL_PORT"
    tls=""
  else
    listen="0.0.0.0:$API_PORT"
    tls="tls_cert = \"$TLS_DIR/server.crt\"
tls_key = \"$TLS_DIR/server.key\"
"
  fi
  if [[ $RELAY_ENABLED == oui ]]; then relais=true; else relais=false; fi
  cat >"$CONF" <<CONFIG
# Le serveur ZyrDesk. Écrit par install.sh, relu au démarrage :
# systemctl restart zyrdesk-server après une modification.
name = "$NAME"
data_dir = "$DATA_DIR"

[api]
listen = "$listen"
${tls}public_url = "https://$(adresse_a_taper)"

[registration]
policy = "$REGISTRATION"

[relay]
enabled = $relais
listen = "0.0.0.0:$RELAY_PORT"
max_sessions = 10
max_kbps_per_session = 60000

[limits]
login_attempts_per_minute = 10
CONFIG
  chown root:"$SERVICE_USER" "$CONF"
  chmod 640 "$CONF"
}

# Les clés du serveur, faites par lui et à lui : sa clé de signature
# naît à la première demande de l'empreinte.
genere_les_cles() {
  runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" fingerprint >/dev/null
}

# Un certificat feuille, jamais une autorité, sur une clé P-256, pour
# dix ans, portant le nom et les adresses de cette machine.
genere_le_certificat() {
  local cnf noms=() rang=1
  cnf=$(mktemp)
  if est_une_ip "$PUBLIC_HOST"; then
    noms+=("IP.$rang = $PUBLIC_HOST"); rang=$((rang + 1))
  else
    noms+=("DNS.1 = $PUBLIC_HOST")
  fi
  for ip in "$IP_PUBLIQUE" "$IP_LOCALE"; do
    if [[ -n $ip && $ip != "$PUBLIC_HOST" ]]; then
      noms+=("IP.$rang = $ip"); rang=$((rang + 1))
    fi
  done
  {
    echo "[req]"
    echo "distinguished_name = dn"
    echo "x509_extensions = serveur"
    echo "prompt = no"
    echo "[dn]"
    echo "CN = $PUBLIC_HOST"
    echo "[serveur]"
    echo "basicConstraints = critical, CA:FALSE"
    echo "keyUsage = critical, digitalSignature"
    echo "extendedKeyUsage = serverAuth"
    echo "subjectAltName = @noms"
    echo "[noms]"
    printf '%s\n' "${noms[@]}"
  } >"$cnf"
  openssl req -x509 -new -config "$cnf" -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
    -sha256 -days 3650 -keyout "$TLS_DIR/server.key" -out "$TLS_DIR/server.crt"
  rm -f "$cnf"
  pose_les_droits_du_certificat
}

copie_le_certificat() {
  install -m 644 "$CERT_FILE" "$TLS_DIR/server.crt"
  install -m 640 "$KEY_FILE" "$TLS_DIR/server.key"
  pose_les_droits_du_certificat
}

pose_les_droits_du_certificat() {
  chown root:"$SERVICE_USER" "$TLS_DIR/server.crt" "$TLS_DIR/server.key"
  chmod 644 "$TLS_DIR/server.crt"
  chmod 640 "$TLS_DIR/server.key"
}

ecrit_l_unite() {
  cat >"$UNIT" <<UNITE
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
UNITE
  # Le durcissement fondé sur les montages ne survit pas à un conteneur
  # non privilégié : le profil AppArmor de Proxmox le refuse, et l'unité
  # ne démarrerait pas du tout. Hors conteneur, il ne coûte rien.
  if [[ -z $CONTENEUR ]]; then
    install -d -m 755 "$DROPIN_DIR"
    cat >"$DROPIN_DIR/10-hardening.conf" <<DURCI
[Service]
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectControlGroups=true
ReadWritePaths=$DATA_DIR
DURCI
  else
    rm -rf "$DROPIN_DIR"
  fi
  systemctl daemon-reload
}

demarre_le_service() {
  systemctl enable --now zyrdesk-server.service
  systemctl restart zyrdesk-server.service
}

# Le serveur se joint lui-même comme le ferait un appareil, une fois
# qu'il écoute : ce que le script attend, avec de la patience.
attend_le_serveur() {
  local essai
  for essai in $(seq 1 30); do
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

CODE_D_INVITATION=""

cree_le_premier_compte() {
  if ! runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" user list | awk '{print $1}' | grep -qx "$ADMIN_USER"; then
    printf '%s\n' "$ADMIN_PASSWORD" | runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" user create "$ADMIN_USER" --password-stdin
  fi
  if [[ $REGISTRATION == invitation ]]; then
    CODE_D_INVITATION=$(runuser -u "$SERVICE_USER" -- "$BIN" --config "$CONF" invite new)
  fi
}

# L'empreinte que l'application demande de comparer, lue chez le serveur.
empreinte_du_serveur() {
  "$BIN" --config "$CONF" fingerprint 2>/dev/null | sed -n '2p' | sed 's/^ *//'
}

resume() {
  local empreinte
  panneau_ouvre "$(t 'Serveur ZyrDesk installé' 'ZyrDesk server installed')" "$C_OK"
  panneau_cle "$(t "Adresse à taper dans l'application" 'Address to type in the application')" "$(adresse_a_taper)"
  if [[ $TLS_MODE != 1 ]]; then
    empreinte=$(empreinte_du_serveur)
    panneau_ligne "$(t "Empreinte du serveur (à comparer dans l'application) :" 'Server fingerprint (to compare in the application):')"
    panneau_ligne "  $(gras "$empreinte")"
  fi
  if [[ $TLS_MODE == 1 ]]; then
    panneau_cle "$(t 'Ports à renvoyer sur la box' 'Ports to forward on the router')" "$(t "TCP 443 vers le mandataire, UDP $RELAY_PORT vers $IP_LOCALE" "TCP 443 to the proxy, UDP $RELAY_PORT to $IP_LOCALE")"
  else
    panneau_cle "$(t "Ports à renvoyer sur la box vers $IP_LOCALE" "Ports to forward on the router to $IP_LOCALE")" "TCP $API_PORT, UDP $RELAY_PORT"
  fi
  panneau_cle "$(t 'Configuration' 'Configuration')" "$CONF"
  panneau_cle "$(t 'Données' 'Data')" "$DATA_DIR   $(t "(sauvegarde : zyrdesk-server backup <dossier>)" '(backup: zyrdesk-server backup <folder>)')"
  if [[ -n $CODE_D_INVITATION ]]; then
    panneau_cle "$(t "Code d'invitation pour un second compte" 'Invitation code for a second account')" "$CODE_D_INVITATION"
  fi
  panneau_ligne "$(t "Les clés de $DATA_DIR/keys font l'identité du serveur : à sauvegarder." "The keys in $DATA_DIR/keys are the server's identity: back them up.")"
  panneau_ferme
  if [[ $TLS_MODE == 1 ]]; then
    panneau_mandataire
  fi
  info "$(t 'Relancer ce script met à jour ou reconfigure. « zyrdesk-server status » dit où il en est.' 'Running this script again updates or reconfigures. "zyrdesk-server status" says where it stands.')"
}

# Les lignes exactes d'un mandataire inverse : le canal vivant est un
# WebSocket, qui veut Upgrade et Connection transmis et un délai de
# lecture long.
panneau_mandataire() {
  panneau_ouvre "$(t 'Le mandataire inverse, à configurer par vous' 'The reverse proxy, to configure yourself')" "$C_ATTENTION"
  panneau_ligne "$(t "Caddy, dans le Caddyfile :" 'Caddy, in the Caddyfile:')"
  panneau_ligne "  $PUBLIC_HOST {"
  panneau_ligne "      reverse_proxy 127.0.0.1:$LOCAL_PORT"
  panneau_ligne "  }"
  panneau_ligne ""
  panneau_ligne "$(t "nginx, dans le bloc server de $PUBLIC_HOST :" "nginx, in the server block of $PUBLIC_HOST:")"
  panneau_ligne "  location / {"
  panneau_ligne "      proxy_pass http://127.0.0.1:$LOCAL_PORT;"
  panneau_ligne "      proxy_http_version 1.1;"
  panneau_ligne "      proxy_set_header Upgrade \$http_upgrade;"
  panneau_ligne "      proxy_set_header Connection \"upgrade\";"
  panneau_ligne "      proxy_set_header Host \$host;"
  panneau_ligne "      proxy_set_header X-Forwarded-For \$remote_addr;"
  panneau_ligne "      proxy_read_timeout 3600s;"
  panneau_ligne "  }"
  panneau_ferme
}

# ---- Les parcours ----------------------------------------------------------

installe() {
  panneau_ouvre "$(t "Où l'on est" 'Where we are')" "$C_DOUX"
  panneau_ligne "$(t 'Machine' 'Machine') : $(gras "$(hostname)") ($OS_ID $OS_VERSION$( [[ -n $CONTENEUR ]] && echo ", $(t 'conteneur' 'container') $CONTENEUR$( [[ $NON_PRIVILEGIE -eq 1 ]] && t ' non privilégié' ' unprivileged')"))"
  panneau_ligne "$(t 'Adresse' 'Address') : $(gras "${IP_LOCALE:-?}")$( [[ -n $IP_PUBLIQUE ]] && echo ", $(t 'publique' 'public') $(gras "$IP_PUBLIQUE")")"
  panneau_ligne "$(t 'Ce script installe le serveur ZyrDesk : comptes, mise en relation, relais.' 'This script installs the ZyrDesk server: accounts, rendezvous, relay.')"
  panneau_ferme
  echo

  pose_les_questions
  recapitule
  demande_oui "$(t "Lancer l'installation maintenant ?" 'Start the installation now?')" oui || exit 0
  echo

  etape "$(t 'Paquets nécessaires' 'Required packages')" installe_les_paquets
  etape "$(t "Utilisateur $SERVICE_USER et dossiers" "User $SERVICE_USER and folders")" cree_l_utilisateur_et_les_dossiers
  etape "$(t 'Obtention de zyrdesk-server' 'Getting zyrdesk-server')" obtient_le_binaire
  etape "$(t "Installation de zyrdesk-server ${VERSION_INSTALLEE:-}" "Installing zyrdesk-server ${VERSION_INSTALLEE:-}")" pose_le_binaire
  etape "$(t 'Configuration écrite' 'Configuration written')" ecrit_la_configuration
  etape "$(t 'Clés du serveur' 'Server keys')" genere_les_cles
  case "$TLS_MODE" in
    2) etape "$(t 'Certificat auto-signé' 'Self-signed certificate')" genere_le_certificat ;;
    3) etape "$(t 'Certificat copié' 'Certificate copied')" copie_le_certificat ;;
  esac
  etape "$(t 'Service systemd installé et démarré' 'systemd service installed and started')" ecrit_l_unite
  etape "$(t 'Démarrage' 'Starting')" demarre_le_service
  etape "$(t 'Le serveur répond' 'The server answers')" attend_le_serveur
  etape "$(t "Compte $ADMIN_USER créé" "Account $ADMIN_USER created")" cree_le_premier_compte
  enregistre_l_etat
  resume
}

met_a_jour() {
  charge_l_etat
  etape "$(t 'Obtention de zyrdesk-server' 'Getting zyrdesk-server')" obtient_le_binaire
  etape "$(t 'Arrêt du service' 'Stopping the service')" systemctl stop zyrdesk-server.service
  etape "$(t "Installation de zyrdesk-server ${VERSION_INSTALLEE:-}" "Installing zyrdesk-server ${VERSION_INSTALLEE:-}")" pose_le_binaire
  etape "$(t 'Service systemd' 'systemd service')" ecrit_l_unite
  etape "$(t 'Démarrage' 'Starting')" demarre_le_service
  etape "$(t 'Le serveur répond' 'The server answers')" attend_le_serveur
  enregistre_l_etat
  ok "$(t "Mis à jour en ${VERSION_INSTALLEE:-?}." "Updated to ${VERSION_INSTALLEE:-?}.")"
}

reconfigure() {
  charge_l_etat
  pose_les_questions
  recapitule
  demande_oui "$(t 'Appliquer cette configuration ?' 'Apply this configuration?')" oui || exit 0
  echo
  etape "$(t 'Configuration écrite' 'Configuration written')" ecrit_la_configuration
  case "$TLS_MODE" in
    2) [[ -f $TLS_DIR/server.crt ]] || etape "$(t 'Certificat auto-signé' 'Self-signed certificate')" genere_le_certificat ;;
    3) etape "$(t 'Certificat copié' 'Certificate copied')" copie_le_certificat ;;
  esac
  etape "$(t 'Service systemd' 'systemd service')" ecrit_l_unite
  etape "$(t 'Redémarrage' 'Restarting')" demarre_le_service
  etape "$(t 'Le serveur répond' 'The server answers')" attend_le_serveur
  etape "$(t "Compte $ADMIN_USER" "Account $ADMIN_USER")" cree_le_premier_compte
  enregistre_l_etat
  resume
}

desinstalle() {
  charge_l_etat
  panneau_ouvre "$(t 'Retirer le serveur' 'Remove the server')" "$C_ATTENTION"
  panneau_ligne "$(t 'Premier palier : le service est arrêté et retiré, le programme effacé.' 'First stage: the service is stopped and removed, the program erased.')"
  panneau_ligne "$(t "Les données et les clés restent dans ${DATA_DIR:-$DEFAULT_DATA} et la configuration dans $CONF_DIR." "Data and keys stay in ${DATA_DIR:-$DEFAULT_DATA}, the configuration in $CONF_DIR.")"
  panneau_ferme
  demande_oui "$(t 'Retirer le service et le programme ?' 'Remove the service and the program?')" non || exit 0
  systemctl disable --now zyrdesk-server.service >/dev/null 2>&1 || true
  rm -f "$UNIT"
  rm -rf "$DROPIN_DIR"
  systemctl daemon-reload
  rm -f "$BIN"
  ok "$(t 'Service et programme retirés.' 'Service and program removed.')"

  panneau_ouvre "$(t 'Second palier : tout effacer' 'Second stage: erase everything')" "$C_ATTENTION"
  panneau_ligne "$(t "Efface ${DATA_DIR:-$DEFAULT_DATA} (comptes, appareils, clés du serveur) et $CONF_DIR." "Erases ${DATA_DIR:-$DEFAULT_DATA} (accounts, devices, server keys) and $CONF_DIR.")"
  panneau_ligne "$(t "Les appareils rattachés perdront leur compte et devront se rattacher à nouveau. Rien ne se récupère ensuite." 'Attached devices lose their account and must attach again. Nothing is recoverable afterwards.')"
  panneau_ferme
  if confirme_en_toutes_lettres; then
    rm -rf "${DATA_DIR:-$DEFAULT_DATA}" "$CONF_DIR"
    userdel "$SERVICE_USER" >/dev/null 2>&1 || true
    ok "$(t 'Données, clés et configuration effacées.' 'Data, keys and configuration erased.')"
  else
    info "$(t 'Données et configuration gardées.' 'Data and configuration kept.')"
  fi
}

menu_d_une_installation_en_place() {
  local version choix
  version=$("$BIN" --version 2>/dev/null | awk '{print $2}' || true)
  panneau_ouvre "$(t 'Un serveur ZyrDesk est déjà installé' 'A ZyrDesk server is already installed')" "$C_DOUX"
  panneau_cle "$(t 'Version' 'Version')" "${version:-?}"
  panneau_cle "$(t 'Configuration' 'Configuration')" "$CONF"
  panneau_cle "$(t 'Service' 'Service')" "$(systemctl is-active zyrdesk-server.service 2>/dev/null || true)"
  panneau_ferme
  demande_choix choix "$(t 'Que faire ?' 'What to do?')" 1 \
    "$(t 'Mettre à jour vers la version publiée' 'Update to the published version')" \
    "$(t 'Reconfigurer (les questions, avec les réponses d'"'"'avant)' 'Reconfigure (the questions, with the previous answers)')" \
    "$(t "Afficher l'état" 'Show the state')" \
    "$(t 'Désinstaller' 'Uninstall')" \
    "$(t 'Ne rien faire' 'Do nothing')"
  case "$choix" in
    1) met_a_jour ;;
    2) reconfigure ;;
    3) "$BIN" --config "$CONF" status ;;
    4) desinstalle ;;
    5) exit 0 ;;
  esac
}

# ---- Les options -----------------------------------------------------------

BINAIRE_FOURNI=""
DEPUIS_LA_SOURCE=0
SOURCE_FOURNIE=""
BRANCHE_SOURCE=""
VERSION_VOULUE=""

aide() {
  cat <<AIDE
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
AIDE
}

while (( $# > 0 )); do
  case "$1" in
    --binary) BINAIRE_FOURNI=$(readlink -f "$2"); shift 2 ;;
    --version) VERSION_VOULUE=$2; shift 2 ;;
    --from-source) DEPUIS_LA_SOURCE=1; shift ;;
    --source) SOURCE_FOURNIE=$(readlink -f "$2"); DEPUIS_LA_SOURCE=1; shift 2 ;;
    --branch) BRANCHE_SOURCE=$2; shift 2 ;;
    --lang) LANGUE=$2; shift 2 ;;
    --help|-h) aide; exit 0 ;;
    *) fail "$(t "option inconnue : $1" "unknown option: $1")"; aide; exit 1 ;;
  esac
done

# ---- Le déroulé ------------------------------------------------------------

banniere
if [[ $INTERACTIF -eq 0 ]]; then
  fail "$(t 'Ce script pose des questions : lancez-le dans un terminal.' 'This script asks questions: run it in a terminal.')"
  exit 1
fi
releve_la_machine
verifie_les_prealables
if [[ -x $BIN || -f $CONF ]]; then
  menu_d_une_installation_en_place
else
  installe
fi
