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
  sessions: document.getElementById("sessions"),
  ordinateurs: document.getElementById("ordinateurs"),
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

const MINUTE = 60;
const HEURE = 3600;

/* Ce que le service dit du réseau et de ce qui tourne. La fenêtre ne
   s'en souvient pas d'elle-même : une session appartient au service, et
   survit à cette fenêtre fermée, mise à jour ou plantée. */
let voisins = [];
let sessions = [];

/* Vrai entre le moment où cette fenêtre demande une session et celui où
   le service la tient. Pendant ce temps-là, elle est la seule à savoir
   qu'il se passe quelque chose. */
let ouverture = false;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* Une seule session à la fois depuis cet ordinateur : deux fenêtres
   vidéo en même temps ne se pilotent pas. */
function occupe() {
  return ouverture || sessions.length > 0;
}

/* ---- Cet ordinateur --------------------------------------------------- */

async function rafraichirEtat() {
  const etat = await invoke("standing");

  vue.nom.textContent = etat.name;
  vue.empreinte.textContent = etat.fingerprint || "indisponible";
  vue.copier.disabled = etat.fingerprint.length === 0;
  montre(vue.serviceAbsent, etat.unreachable !== null);

  // Le service arrêté n'est pas un accès distant désactivé : l'un est un
  // choix, l'autre une panne. L'interrupteur reste sur la position
  // choisie et devient inactionnable, plutôt que de sauter à « non » et
  // de faire croire à une décision que personne n'a prise.
  vue.interrupteur.disabled = etat.unreachable !== null || bascule;
  if (!bascule) {
    vue.interrupteur.setAttribute("aria-checked", etat.wanted ? "true" : "false");
  }

  if (etat.unreachable !== null) {
    vue.pastilleHote.className = "pastille";
    vue.etatHote.textContent = "Service arrêté";
  } else if (!etat.wanted) {
    vue.pastilleHote.className = "pastille";
    vue.etatHote.textContent = "Accès distant désactivé";
  } else if (etat.hosting) {
    vue.pastilleHote.className = "pastille vivante";
    vue.etatHote.textContent = "Prêt à être contrôlé";
  } else {
    vue.pastilleHote.className = "pastille attention";
    vue.etatHote.textContent = "Démarrage en cours…";
  }
}

/* Le temps que le service prenne acte, l'état qui revient est encore
   l'ancien. Sans ce verrou, l'interrupteur reviendrait en arrière sous
   le doigt avant de repartir. */
let bascule = false;

async function basculerAcces() {
  const veut = vue.interrupteur.getAttribute("aria-checked") !== "true";
  bascule = true;
  vue.interrupteur.setAttribute("aria-checked", veut ? "true" : "false");
  vue.interrupteur.disabled = true;
  montre(vue.probleme, false);

  try {
    await invoke("set_hosting", { on: veut });
  } catch (raison) {
    vue.interrupteur.setAttribute("aria-checked", veut ? "false" : "true");
    vue.problemeTexte.textContent = String(raison);
    montre(vue.probleme, true);
  } finally {
    bascule = false;
    await rafraichirEtat();
  }
}

async function copierEmpreinte() {
  await navigator.clipboard.writeText(vue.empreinte.textContent);
  vue.copier.textContent = "Copié";
  setTimeout(() => {
    vue.copier.textContent = "Copier";
  }, TEMPS_COPIE);
}

/* ---- Ce que tient le service ------------------------------------------ */

/* Une seule demande pour les deux : la session est nommée d'après
   l'ordinateur qui l'accueille, et les cartes changent d'allure selon
   qu'une session tourne ou non. Les demander séparément ferait dessiner
   deux fois de suite avec la moitié de la réponse. */
async function rafraichirLeReseau() {
  const [trouves, enCours] = await Promise.all([
    invoke("peers"),
    invoke("sessions"),
  ]);
  voisins = trouves;
  sessions = enCours;
  dessine();
}

function dessine() {
  dessineSessions();
  dessineOrdinateurs();
  ajusterAjout();
}

/* ---- Les sessions en cours -------------------------------------------- */

/* Ce qui est déjà à l'écran, pour ne redessiner que si ça a changé. */
let sessionsAffichees = "";

/* Une session est reconnue à l'empreinte et non à l'adresse : c'est la
   seule chose qui ne bouge pas d'un réseau à l'autre. */
function nomDe(session) {
  const connu = voisins.find(
    (ordinateur) => ordinateur.fingerprint === session.fingerprint,
  );
  return connu ? connu.name : session.towards;
}

function duree(secondes) {
  if (secondes < MINUTE) {
    return "moins d'une minute";
  }
  const minutes = Math.floor((secondes % HEURE) / MINUTE);
  if (secondes < HEURE) {
    return `${minutes} minute${minutes > 1 ? "s" : ""}`;
  }
  const heures = Math.floor(secondes / HEURE);
  return minutes === 0
    ? `${heures} h`
    : `${heures} h ${String(minutes).padStart(2, "0")}`;
}

function motDeSession(session) {
  return `Ouverte depuis ${duree(session.since)}. Cette fenêtre peut être fermée : la session continue toute seule.`;
}

/* La carte de l'ordinateur, juste en dessous, porte déjà son adresse et
   son état : ce bandeau dit ce qui se passe, il ne le répète pas. */
function carteSession(session) {
  const element = document.createElement("div");
  element.className = "carte session apparait";

  const nom = document.createElement("p");
  nom.className = "sous-titre session-nom";
  const pastille = document.createElement("span");
  pastille.className = "pastille vivante";
  const texte = document.createElement("span");
  texte.textContent = `Session en cours vers ${nomDe(session)}`;
  nom.append(pastille, texte);

  const mot = document.createElement("p");
  mot.className = "legende session-mot";

  element.append(nom, mot);
  return element;
}

function dessineSessions() {
  const signature = sessions.map((session) => session.fingerprint).join(" ");
  if (signature !== sessionsAffichees) {
    sessionsAffichees = signature;
    vue.sessions.replaceChildren(...sessions.map(carteSession));
  }

  // La durée avance sans que la carte renaisse : une carte qui
  // réapparaîtrait à chaque minute attirerait l'oeil pour rien.
  const mots = vue.sessions.querySelectorAll(".session-mot");
  sessions.forEach((session, rang) => {
    mots[rang].textContent = motDeSession(session);
  });

  montre(vue.sessions, sessions.length > 0);
}

/* ---- Les ordinateurs du réseau ---------------------------------------- */

/* Redessiner la liste à chaque passage ferait clignoter les cartes et
   perdrait le survol en cours. On ne touche qu'à ce qui a changé. */
let listeAffichee = "";

function carte(ordinateur) {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "carte ordinateur";

  const nom = document.createElement("p");
  nom.className = "sous-titre ordinateur-nom";
  const pastille = document.createElement("span");
  pastille.className = "pastille vivante";
  const texte = document.createElement("span");
  texte.textContent = ordinateur.name;
  nom.append(pastille, texte);

  const adresse = document.createElement("p");
  adresse.className = "legende";
  adresse.textContent = ordinateur.address;

  const appel = document.createElement("p");
  appel.className = "legende ordinateur-appel";
  const sien = sessions.some(
    (session) => session.fingerprint === ordinateur.fingerprint,
  );
  if (sien) {
    element.classList.add("en-session");
  }
  appel.textContent = sien ? "Session en cours" : "Se connecter";
  element.disabled = occupe();

  element.append(nom, adresse, appel);
  element.addEventListener("click", () =>
    lance(ordinateur.address, ordinateur.fingerprint),
  );
  return element;
}

/* Un ordinateur qui n'est pas sur ce réseau, ou dont l'annonce est
   bloquée, doit rester ajoutable. Sans cette tuile, la découverte
   d'un seul voisin ferait disparaître le seul moyen d'en ajouter un. */
function tuileAjout() {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "carte ordinateur ajout-tuile";
  element.disabled = occupe();

  const signe = document.createElement("p");
  signe.className = "sous-titre";
  signe.textContent = "+";

  const mot = document.createElement("p");
  mot.className = "legende";
  mot.textContent = "Ajouter un ordinateur";

  element.append(signe, mot);
  element.addEventListener("click", ouvrirAjout);
  return element;
}

function dessineOrdinateurs() {
  const signature = JSON.stringify([
    voisins,
    sessions.map((session) => session.fingerprint),
    ouverture,
  ]);
  if (signature === listeAffichee) {
    return;
  }
  listeAffichee = signature;

  vue.ordinateurs.replaceChildren(...voisins.map(carte), tuileAjout());
  montre(vue.ordinateurs, voisins.length > 0);
  // L'état vide ne s'affiche que s'il n'y a rien à montrer et rien en
  // cours : une session occupe déjà la place.
  montre(vue.aucun, voisins.length === 0 && !occupe());
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
    occupe() ||
    vue.adresse.value.trim().length === 0 ||
    longueur !== TAILLE_EMPREINTE;
}

function connecter(evenement) {
  evenement.preventDefault();
  vue.ajout.close();
  lance(vue.adresse.value, vue.empreinteDistante.value);
}

async function lance(adresse, empreinte) {
  if (occupe()) {
    return;
  }
  ouverture = true;
  dessine();
  montre(vue.probleme, false);
  etape("Ouverture du tunnel…", `Vers ${adresse.trim()}.`, null);

  try {
    await invoke("connect", { host: adresse, fingerprint: empreinte });
  } catch (raison) {
    echoue(String(raison));
  }
}

/* ---- Ce qui se passe pendant l'ouverture ------------------------------ */

function etape(titre, detail, code) {
  vue.etapeTitre.textContent = titre;
  vue.etapeDetail.textContent = detail;
  vue.etapeCode.textContent = code ?? "";
  montre(vue.etapeCode, code !== null);
  montre(vue.etape, true);
}

/* La fenêtre n'a plus rien à raconter : ce qui se passe maintenant se lit
   dans ce que tient le service. Le bandeau ne s'efface qu'une fois la
   réponse arrivée, sinon la page se vide le temps d'un aller-retour. */
async function rangeEtape() {
  ouverture = false;
  await rafraichirLeReseau();
  montre(vue.etape, false);
}

function echoue(texte) {
  vue.problemeTexte.textContent = texte;
  montre(vue.probleme, true);
  rangeEtape();
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
        "Premier accès à cet ordinateur. Tapez ce code sur celui que vous voulez contrôler :",
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
      // À partir d'ici le service tient la session, et n'importe quelle
      // fenêtre la retrouve, y compris une autre que celle-ci.
      rangeEtape();
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

/* ---- Thème ------------------------------------------------------------ */

/* Le choix vit dans theme.js, qui l'a déjà appliqué avant que cette page
   ne soit dessinée. Ce qui reste ici est de montrer lequel est actif et
   d'accorder les décorations de la fenêtre, qui appartiennent au système
   et non à la page. */
function marqueLeChoix() {
  const actif = window.theme.choisi();
  for (const bouton of document.querySelectorAll("[data-theme-choix]")) {
    bouton.setAttribute(
      "aria-pressed",
      bouton.dataset.themeChoix === actif ? "true" : "false",
    );
  }
}

for (const bouton of document.querySelectorAll("[data-theme-choix]")) {
  bouton.addEventListener("click", () => {
    window.theme.poser(bouton.dataset.themeChoix);
    marqueLeChoix();
  });
}

window.addEventListener("theme-pose", ({ detail }) => {
  invoke("set_theme", { clair: detail === "clair" }).catch(() => {});
  marqueLeChoix();
});

/* ---- Mise en route ---------------------------------------------------- */

vue.copier.addEventListener("click", copierEmpreinte);
vue.interrupteur.addEventListener("click", basculerAcces);
vue.ouvrirAjout.addEventListener("click", ouvrirAjout);
vue.annulerAjout.addEventListener("click", () => vue.ajout.close());
vue.adresse.addEventListener("input", ajusterAjout);
vue.empreinteDistante.addEventListener("input", ajusterAjout);
vue.ajoutForme.addEventListener("submit", connecter);

marqueLeChoix();
invoke("set_theme", {
  clair: document.documentElement.dataset.theme === "clair",
}).catch(() => {});

rafraichirEtat();
rafraichirLeReseau();
setInterval(() => {
  rafraichirEtat();
  rafraichirLeReseau();
}, RYTHME_ETAT);
