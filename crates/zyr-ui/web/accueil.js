/*
  L'accueil. Il ne décide de rien : il demande au coeur Rust, qui demande
  au service, et il dessine ce qui revient.

  Le vocabulaire suit celui du produit : « ordinateur » et non « hôte »,
  « accès distant » et non « service ».
*/

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const vue = {
  nom: document.getElementById("nom-machine"),
  pastilleHote: document.getElementById("pastille-hote"),
  etatHote: document.getElementById("etat-hote"),
  interrupteur: document.getElementById("interrupteur-hote"),
  serviceAbsent: document.getElementById("service-absent"),
  empreinte: document.getElementById("empreinte"),
  adresse: document.getElementById("adresse"),
  empreinteDistante: document.getElementById("empreinte-distante"),
  connecter: document.getElementById("connecter"),
  etape: document.getElementById("etape"),
  etapeTitre: document.getElementById("etape-titre"),
  etapeDetail: document.getElementById("etape-detail"),
  etapeCode: document.getElementById("etape-code"),
  probleme: document.getElementById("probleme"),
  problemeTexte: document.getElementById("probleme-texte"),
};

/* Longueur d'une empreinte, en caractères. Elle ne varie pas : la
   vérifier ici évite d'aller déranger le service pour rien. */
const TAILLE_EMPREINTE = 64;

/* Le service peut démarrer après l'interface, ou s'arrêter pendant
   qu'elle est ouverte. On redemande, sans jamais bloquer la fenêtre. */
const RYTHME_ETAT = 3000;

let sessionEnCours = false;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* ---- Cet ordinateur --------------------------------------------------- */

async function rafraichirEtat() {
  const etat = await invoke("standing");

  vue.nom.textContent = etat.name;
  vue.empreinte.textContent = etat.fingerprint;
  montre(vue.serviceAbsent, etat.unreachable !== null);

  if (etat.unreachable !== null) {
    vue.pastilleHote.className = "pastille";
    vue.etatHote.textContent = "Service arrêté";
    vue.interrupteur.setAttribute("aria-checked", "false");
  } else if (etat.hosting) {
    vue.pastilleHote.className = "pastille vivante";
    vue.etatHote.textContent = "Prêt à être contrôlé";
    vue.interrupteur.setAttribute("aria-checked", "true");
  } else {
    vue.pastilleHote.className = "pastille attention";
    vue.etatHote.textContent = "Démarrage en cours…";
    vue.interrupteur.setAttribute("aria-checked", "false");
  }

  ajusterConnexion();
}

/* ---- Se connecter ------------------------------------------------------ */

function ajusterConnexion() {
  const pret =
    !sessionEnCours &&
    vue.adresse.value.trim().length > 0 &&
    vue.empreinteDistante.value.trim().length === TAILLE_EMPREINTE;
  vue.connecter.disabled = !pret;
}

async function connecter() {
  sessionEnCours = true;
  ajusterConnexion();
  montre(vue.probleme, false);
  etape("Ouverture du tunnel…", "", null);

  try {
    await invoke("connect", {
      host: vue.adresse.value,
      fingerprint: vue.empreinteDistante.value,
    });
  } catch (raison) {
    echoue(String(raison));
  }
}

function etape(titre, detail, code) {
  vue.etapeTitre.textContent = titre;
  vue.etapeDetail.textContent = detail;
  vue.etapeCode.textContent = code ?? "";
  montre(vue.etapeCode, code !== null);
  montre(vue.etape, true);
}

function echoue(texte) {
  sessionEnCours = false;
  montre(vue.etape, false);
  vue.problemeTexte.textContent = texte;
  montre(vue.probleme, true);
  ajusterConnexion();
}

/* ---- Ce que le coeur raconte pendant l'ouverture ---------------------- */

listen("session-step", ({ payload }) => {
  switch (payload.kind) {
    case "reached":
      etape(
        "Tunnel établi",
        `Taille de paquet : ${payload.packet} octets.`,
        null,
      );
      break;
    case "pairingNeeded":
      etape(
        "Autorisation nécessaire",
        `Premier accès à cet ordinateur. Sur ${vue.adresse.value.trim()}, tapez ce code :`,
        payload.pin,
      );
      break;
    case "paired":
      etape("Autorisé", "Démarrage de la session…", null);
      break;
    case "starting":
      etape("Démarrage de la session…", "", null);
      break;
    case "live":
      etape(
        "Session en cours",
        "Vous pouvez fermer cette fenêtre : la session continue.",
        null,
      );
      break;
  }
});

listen("session-ended", ({ payload }) => {
  if (payload.ok) {
    sessionEnCours = false;
    montre(vue.etape, false);
    ajusterConnexion();
  } else {
    echoue(payload.message);
  }
});

/* ---- Mise en route ---------------------------------------------------- */

vue.adresse.addEventListener("input", ajusterConnexion);
vue.empreinteDistante.addEventListener("input", ajusterConnexion);
vue.connecter.addEventListener("click", connecter);

rafraichirEtat();
setInterval(rafraichirEtat, RYTHME_ETAT);
