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
  empreinte: document.getElementById("empreinte"),
  copier: document.getElementById("copier-empreinte"),
  aFaire: document.getElementById("a-faire"),
  version: document.getElementById("version"),
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
  ouvrirJournal: document.getElementById("ouvrir-journal"),
  journal: document.getElementById("journal"),
  fermerJournal: document.getElementById("fermer-journal"),
  journalTexte: document.getElementById("journal-texte"),
  copierJournal: document.getElementById("copier-journal"),
  rafraichirJournal: document.getElementById("rafraichir-journal"),
  viderJournal: document.getElementById("vider-journal"),
  ouvrirJournaux: document.getElementById("ouvrir-journaux"),
  ouvrirReglages: document.getElementById("ouvrir-reglages"),
  reglages: document.getElementById("reglages"),
  fermerReglages: document.getElementById("fermer-reglages"),
  qualiteDetail: document.getElementById("qualite-detail"),
  confiance: document.getElementById("confiance"),
  stats: document.getElementById("stats"),
  dossierJournaux: document.getElementById("dossier-journaux"),
  ouvrirDossier: document.getElementById("ouvrir-dossier"),
  reglagesProbleme: document.getElementById("reglages-probleme"),
  reglagesProblemeTexte: document.getElementById("reglages-probleme-texte"),
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

/* Le temps qu'une demande de confirmation reste ouverte. */
const TEMPS_CONFIRMATION = 4000;

const MINUTE = 60;
const HEURE = 3600;

/* Ce que le service dit du réseau et de ce qui tourne. La fenêtre ne
   s'en souvient pas d'elle-même : une session appartient au service, et
   survit à cette fenêtre fermée, mise à jour ou plantée. */
let voisins = [];
let sessions = [];
let etat = null;
let moteurs = null;

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
  etat = await invoke("standing");

  vue.nom.textContent = etat.name;
  vue.empreinte.textContent = etat.fingerprint || "indisponible";
  vue.copier.disabled = etat.fingerprint.length === 0;

  // Le service arrêté n'est pas un accès distant désactivé : l'un est un
  // choix, l'autre une panne. L'interrupteur reste sur la position
  // choisie et devient inactionnable, plutôt que de sauter à « non » et
  // de faire croire à une décision que personne n'a prise.
  vue.interrupteur.disabled = etat.unreachable !== null || bascule;
  if (!bascule) {
    vue.interrupteur.setAttribute(
      "aria-checked",
      etat.wanted ? "true" : "false",
    );
  }

  vue.etatHote.textContent = motDeLEtat();
  vue.pastilleHote.className = `pastille ${couleurDeLEtat()}`;

  dessineCeQuiManque();
  dessineLaVersion();
}

function motDeLEtat() {
  if (etat.unreachable !== null) {
    return "Service arrêté";
  }
  if (!etat.wanted) {
    return "Accès distant désactivé";
  }
  if (etat.hosting) {
    return "Prêt à être contrôlé";
  }
  switch (etat.holdup) {
    case "engineMissing":
      return "Moteur hôte absent";
    case "engineWontStand":
      return "Le moteur hôte ne démarre pas";
    default:
      return "Démarrage en cours…";
  }
}

function couleurDeLEtat() {
  if (etat.unreachable !== null || !etat.wanted) {
    return "";
  }
  if (etat.hosting) {
    return "vivante";
  }
  return etat.holdup === "starting" ? "attention" : "erreur";
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

/* ---- Ce qu'il reste à faire -------------------------------------------- */

/* Ce qui empêche le produit de marcher, dit en clair et avec de quoi y
   remédier. Sans ça, un moteur absent se lit « démarrage en cours » pour
   toujours, et un service arrêté ne se répare que par une commande. */
let manquesAffiches = "";

function dessineCeQuiManque() {
  if (etat === null) {
    return;
  }
  const manques = [];

  if (etat.unreachable !== null) {
    manques.push({
      texte:
        "Le service ZyrDesk ne tourne pas. Cet ordinateur ne peut ni être contrôlé ni en contrôler un autre.",
      bouton: "Démarrer le service",
      action: demarrerService,
    });
  } else if (etat.wanted && etat.holdup === "engineMissing") {
    manques.push({
      texte:
        "Le moteur hôte n'est pas installé : cet ordinateur ne peut pas être contrôlé. Déposez-le dans son dossier, il sera repris tout seul.",
      bouton: "Ouvrir le dossier",
      action: () => ouvreDossier("host-engine"),
    });
  } else if (etat.wanted && etat.holdup === "engineWontStand") {
    manques.push({
      texte:
        "Le moteur hôte ne tient pas en marche. Coupez puis rallumez l'accès distant pour réessayer ; le journal dit pourquoi.",
      bouton: "Voir le journal",
      action: ouvrirJournal,
    });
  }

  if (moteurs !== null && !moteurs.clientHere) {
    manques.push({
      texte:
        "Le moteur client n'est pas installé : cet ordinateur ne peut en contrôler aucun autre.",
      bouton: "Ouvrir le dossier",
      action: () => ouvreDossier("client-engine"),
    });
  }

  const signature = JSON.stringify(manques.map((manque) => manque.texte));
  if (signature === manquesAffiches) {
    return;
  }
  manquesAffiches = signature;
  vue.aFaire.replaceChildren(...manques.map(bandeau));
}

function bandeau(manque) {
  const element = document.createElement("div");
  element.className = "bandeau alerte avec-action apparait";

  const mot = document.createElement("span");
  mot.className = "bandeau-mot";
  mot.textContent = manque.texte;

  const commande = document.createElement("button");
  commande.type = "button";
  commande.className = "bouton discret";
  commande.textContent = manque.bouton;
  commande.addEventListener("click", () => manque.action(commande));

  element.append(mot, commande);
  return element;
}

/* Windows demande les droits administrateur, et personne d'autre que la
   personne devant l'écran ne peut répondre : le bouton attend, en le
   disant. */
async function demarrerService(bouton) {
  const mot = bouton.textContent;
  bouton.disabled = true;
  bouton.textContent = "Démarrage…";
  montre(vue.probleme, false);

  try {
    await invoke("start_service");
  } catch (raison) {
    echoue(String(raison));
  } finally {
    bouton.disabled = false;
    bouton.textContent = mot;
    await rafraichirEtat();
    await rafraichirLesMoteurs();
  }
}

async function ouvreDossier(lequel) {
  try {
    await invoke("open_folder", { which: lequel });
  } catch (raison) {
    echoue(String(raison));
  }
}

async function rafraichirLesMoteurs() {
  moteurs = await invoke("engines");
  dessineCeQuiManque();
}

/* ---- Version ----------------------------------------------------------- */

/* Ce que fait tourner cette fenêtre, et ce que fait tourner le service.
   Les deux se compilent ensemble : le jour où ils diffèrent, c'est la
   panne, et il vaut mieux la lire que la chercher. */
let versionFenetre = "";

function dessineLaVersion() {
  // Tant que la fenêtre ne connaît pas la sienne, elle ne peut comparer
  // quoi que ce soit : afficher un désaccord ici le ferait clignoter en
  // ambre à chaque ouverture, pour rien.
  if (versionFenetre.length === 0) {
    return;
  }
  const service = etat === null ? "" : etat.serviceBuild;
  const sien = versionFenetre.includes(service);

  if (service.length === 0 || sien) {
    vue.version.textContent = versionFenetre;
    vue.version.classList.remove("desaccord");
    return;
  }
  vue.version.textContent = `${versionFenetre}, mais le service tourne encore en ${service}`;
  vue.version.classList.add("desaccord");
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
    case "pairing":
      etape(
        "Premier accès à cet ordinateur",
        "Les deux ordinateurs font connaissance. Rien à faire.",
        null,
      );
      break;
    case "pairingNeeded":
      etape(
        "Autorisation nécessaire",
        "Tapez ce code sur l'ordinateur que vous voulez contrôler :",
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

/* ---- Journal ----------------------------------------------------------- */

async function ouvrirJournal() {
  vue.journal.showModal();
  await rafraichirJournal();
}

async function rafraichirJournal() {
  vue.journalTexte.textContent = "Lecture…";
  vue.journalTexte.textContent = await invoke("journal");
  // Le plus récent est en bas : c'est là que se trouve ce qui vient
  // d'arriver, et c'est ce qu'on ouvre le journal pour lire.
  vue.journalTexte.scrollTop = vue.journalTexte.scrollHeight;
}

async function copierJournal() {
  await navigator.clipboard.writeText(vue.journalTexte.textContent);
  vue.copierJournal.textContent = "Copié";
  setTimeout(() => {
    vue.copierJournal.textContent = "Copier tout";
  }, TEMPS_COPIE);
}

/* Vider efface la seule trace de ce qui vient de se passer. Un deuxième
   clic est demandé, et l'attente retombe d'elle-même : c'est assez pour
   qu'un clic de travers ne coûte rien, et trop peu pour gêner celui qui
   voulait vraiment vider. */
let videEnAttente = null;

function reposeLeVidage() {
  clearTimeout(videEnAttente);
  videEnAttente = null;
  vue.viderJournal.textContent = "Vider";
  vue.viderJournal.classList.remove("attention");
}

async function viderJournal() {
  if (videEnAttente === null) {
    vue.viderJournal.textContent = "Confirmer";
    vue.viderJournal.classList.add("attention");
    videEnAttente = setTimeout(reposeLeVidage, TEMPS_CONFIRMATION);
    return;
  }
  reposeLeVidage();

  try {
    await invoke("clear_journal");
  } catch (raison) {
    // Montré dans le journal lui-même : c'est là que regarde la personne
    // qui vient de cliquer, et il est sur le point d'être relu.
    vue.journalTexte.textContent = String(raison);
    return;
  }
  await rafraichirJournal();
}

/* ---- Réglages ---------------------------------------------------------- */

/* Ce que le service a retenu. La fenêtre ne décide de rien ici non plus :
   elle montre ce qui revient et renvoie ce qui a été cliqué. Un réglage
   choisi ici survit donc à la fenêtre, et vaut pour la prochaine session
   comme pour toutes les suivantes. */
let reglages = null;

async function rafraichirReglages() {
  reglages = await invoke("settings");
  dessineReglages();
}

function dessineReglages() {
  if (reglages === null) {
    return;
  }

  // Ce que la qualité veut dire, dit par le produit et non recalculé
  // ici : une seconde table de qualités s'écarterait de la vraie.
  const debit = Math.round(reglages.bitrateKbps / 1000);
  vue.qualiteDetail.textContent = `${reglages.width} x ${reglages.height}, ${reglages.fps} images par seconde, ${debit} Mb/s`;

  marque("quality", reglages.quality);
  marque("codec", reglages.codec);
  marque("display", reglages.display);
  marque("mouse", reglages.absoluteMouse ? "desktop" : "game");
  vue.stats.setAttribute(
    "aria-checked",
    reglages.statsOverlay ? "true" : "false",
  );
}

function marque(nom, valeur) {
  for (const bouton of document.querySelectorAll(
    `[data-reglage="${nom}"] [data-valeur]`,
  )) {
    bouton.setAttribute(
      "aria-pressed",
      bouton.dataset.valeur === valeur ? "true" : "false",
    );
  }
}

/* Un réglage change, les autres ne bougent pas : le service reçoit
   l'ensemble pour n'avoir jamais à deviner ce qui est resté. */
async function change(comment) {
  if (reglages === null) {
    return;
  }
  const veut = {
    quality: reglages.quality,
    codec: reglages.codec,
    display: reglages.display,
    absoluteMouse: reglages.absoluteMouse,
    statsOverlay: reglages.statsOverlay,
  };
  comment(veut);

  // Pris en compte tout de suite : deux clics rapprochés doivent
  // s'ajouter au lieu de s'annuler, et le bouton doit répondre sans
  // attendre l'aller-retour. Ce qui fait foi revient juste après.
  reglages = { ...reglages, ...veut };
  dessineReglages();

  montre(vue.reglagesProbleme, false);
  try {
    await invoke("choose", { chosen: veut });
  } catch (raison) {
    soucis(String(raison));
  }
  // Redemandé plutôt que supposé : ce qui s'affiche est ce qui a été
  // retenu, y compris quand rien ne l'a été.
  await rafraichirReglages();
}

/* La confiance au réseau local ne vit pas dans les mêmes réglages que
   l'image : elle appartient à cette machine, comme l'accès distant, et
   c'est l'état de la machine qui la porte. */
let basculeConfiance = false;

function dessineConfiance() {
  if (etat === null) {
    return;
  }
  vue.confiance.disabled = etat.unreachable !== null || basculeConfiance;
  if (!basculeConfiance) {
    vue.confiance.setAttribute(
      "aria-checked",
      etat.trusting ? "true" : "false",
    );
  }
}

async function basculerConfiance() {
  const veut = vue.confiance.getAttribute("aria-checked") !== "true";
  basculeConfiance = true;
  vue.confiance.setAttribute("aria-checked", veut ? "true" : "false");
  vue.confiance.disabled = true;
  montre(vue.reglagesProbleme, false);

  try {
    await invoke("set_trust", { on: veut });
  } catch (raison) {
    vue.confiance.setAttribute("aria-checked", veut ? "false" : "true");
    soucis(String(raison));
  } finally {
    basculeConfiance = false;
    await rafraichirEtat();
    dessineConfiance();
  }
}

function soucis(texte) {
  vue.reglagesProblemeTexte.textContent = texte;
  montre(vue.reglagesProbleme, true);
}

async function ouvrirReglages() {
  montre(vue.reglagesProbleme, false);
  vue.reglages.showModal();
  dessineConfiance();
  await rafraichirReglages();
}

async function ouvrirLesJournaux() {
  montre(vue.reglagesProbleme, false);
  try {
    await invoke("open_folder", { which: "logs" });
  } catch (raison) {
    soucis(String(raison));
  }
}

for (const bouton of vue.reglages.querySelectorAll(
  "[data-reglage] [data-valeur]",
)) {
  bouton.addEventListener("click", () => {
    const nom = bouton.closest("[data-reglage]").dataset.reglage;
    const valeur = bouton.dataset.valeur;
    change((veut) => {
      if (nom === "mouse") {
        veut.absoluteMouse = valeur === "desktop";
      } else {
        veut[nom] = valeur;
      }
    });
  });
}

vue.stats.addEventListener("click", () => {
  const actif = vue.stats.getAttribute("aria-checked") !== "true";
  change((veut) => {
    veut.statsOverlay = actif;
  });
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
vue.ouvrirJournal.addEventListener("click", ouvrirJournal);
vue.fermerJournal.addEventListener("click", () => {
  reposeLeVidage();
  vue.journal.close();
});
vue.rafraichirJournal.addEventListener("click", rafraichirJournal);
vue.copierJournal.addEventListener("click", copierJournal);
vue.viderJournal.addEventListener("click", viderJournal);
vue.ouvrirJournaux.addEventListener("click", () => ouvreDossier("logs"));
vue.ouvrirReglages.addEventListener("click", ouvrirReglages);
vue.fermerReglages.addEventListener("click", () => vue.reglages.close());
vue.confiance.addEventListener("click", basculerConfiance);
vue.ouvrirDossier.addEventListener("click", ouvrirLesJournaux);

marqueLeChoix();
invoke("set_theme", {
  clair: document.documentElement.dataset.theme === "clair",
}).catch(() => {});

// Ce qui ne bouge pas de toute la vie du programme : demandé une fois.
invoke("logs_folder").then((dossier) => {
  vue.dossierJournaux.textContent = dossier;
});
invoke("build").then((version) => {
  versionFenetre = version;
  dessineLaVersion();
});

rafraichirEtat();
rafraichirLeReseau();
rafraichirLesMoteurs();
setInterval(() => {
  rafraichirEtat();
  rafraichirLeReseau();
  rafraichirLesMoteurs();
}, RYTHME_ETAT);
