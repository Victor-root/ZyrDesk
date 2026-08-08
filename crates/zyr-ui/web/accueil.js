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
  copier: document.getElementById("copier-empreinte"),
  aucun: document.getElementById("aucun-ordinateur"),
  ouvrirAjout: document.getElementById("ouvrir-ajout"),
  ajout: document.getElementById("ajout"),
  ajoutForme: document.getElementById("ajout-forme"),
  annulerAjout: document.getElementById("annuler-ajout"),
  adresse: document.getElementById("adresse"),
  empreinteDistante: document.getElementById("empreinte-distante"),
  motEmpreinte: document.getElementById("mot-empreinte"),
  connecter: document.getElementById("connecter"),
  etape: document.getElementById("etape"),
  etapeTitre: document.getElementById("etape-titre"),
  etapeDetail: document.getElementById("etape-detail"),
  etapeCode: document.getElementById("etape-code"),
  etapeFermer: document.getElementById("etape-fermer"),
  probleme: document.getElementById("probleme"),
  problemeTexte: document.getElementById("probleme-texte"),
};

/* Longueur d'une empreinte, en caractères. Elle ne varie pas : la
   vérifier ici évite d'aller déranger le service pour rien. */
const TAILLE_EMPREINTE = 64;

/* Le service peut démarrer après l'interface, ou s'arrêter pendant
   qu'elle est ouverte. On redemande, sans jamais bloquer la fenêtre. */
const RYTHME_ETAT = 3000;

/* Le temps qu'un « Copié » reste lisible avant que le bouton reprenne
   son mot habituel. */
const TEMPS_COPIE = 1600;

let sessionEnCours = false;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* ---- Cet ordinateur --------------------------------------------------- */

async function rafraichirEtat() {
  const etat = await invoke("standing");

  vue.nom.textContent = etat.name;
  vue.empreinte.textContent = etat.fingerprint || "indisponible";
  vue.copier.disabled = etat.fingerprint.length === 0;
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
}

async function copierEmpreinte() {
  await navigator.clipboard.writeText(vue.empreinte.textContent);
  vue.copier.textContent = "Copié";
  setTimeout(() => {
    vue.copier.textContent = "Copier";
  }, TEMPS_COPIE);
}

/* ---- Ajouter un ordinateur -------------------------------------------- */

function ouvrirAjout() {
  vue.motEmpreinte.textContent = "";
  vue.ajout.showModal();
  vue.adresse.focus();
}

/* Dit ce qui manque plutôt que de laisser un bouton éteint sans raison. */
function ajusterAjout() {
  const empreinte = vue.empreinteDistante.value.trim();
  const longueur = empreinte.length;

  if (longueur === 0 || longueur === TAILLE_EMPREINTE) {
    vue.motEmpreinte.textContent = "";
  } else {
    vue.motEmpreinte.textContent = `${longueur} caractères sur ${TAILLE_EMPREINTE}`;
  }

  vue.connecter.disabled =
    sessionEnCours ||
    vue.adresse.value.trim().length === 0 ||
    longueur !== TAILLE_EMPREINTE;
}

async function connecter(evenement) {
  evenement.preventDefault();
  sessionEnCours = true;
  ajusterAjout();
  vue.ajout.close();
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

/* ---- Ce qui se passe pendant l'ouverture ------------------------------ */

function etape(titre, detail, code, fini = false) {
  vue.etapeTitre.textContent = titre;
  vue.etapeDetail.textContent = detail;
  vue.etapeCode.textContent = code ?? "";
  montre(vue.etapeCode, code !== null);
  montre(vue.etapeFermer, fini);
  montre(vue.etape, true);
  montre(vue.aucun, false);
}

function rangeEtape() {
  sessionEnCours = false;
  montre(vue.etape, false);
  montre(vue.aucun, true);
  ajusterAjout();
}

function echoue(texte) {
  rangeEtape();
  vue.problemeTexte.textContent = texte;
  montre(vue.probleme, true);
}

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
    rangeEtape();
  } else {
    echoue(payload.message);
  }
});

/* ---- Mise en route ---------------------------------------------------- */

vue.copier.addEventListener("click", copierEmpreinte);
vue.ouvrirAjout.addEventListener("click", ouvrirAjout);
vue.annulerAjout.addEventListener("click", () => vue.ajout.close());
vue.adresse.addEventListener("input", ajusterAjout);
vue.empreinteDistante.addEventListener("input", ajusterAjout);
vue.ajoutForme.addEventListener("submit", connecter);
vue.etapeFermer.addEventListener("click", rangeEtape);

rafraichirEtat();
setInterval(rafraichirEtat, RYTHME_ETAT);
