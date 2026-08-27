/*
  L'accueil. Il ne décide de rien : il demande au coeur Rust, qui demande
  au service, et il dessine ce qui revient.

  Le vocabulaire suit celui du produit : « ordinateur » et non « hôte »,
  « accès distant » et non « service ».
*/

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

/* La page d'accueil elle-même, pour l'éteindre sous l'écran
   d'ouverture. */
const page = document.querySelector("main");

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
  nomAjout: document.getElementById("nom-ajout"),
  empreinteDistante: document.getElementById("empreinte-distante"),
  motEmpreinte: document.getElementById("mot-empreinte"),
  connecter: document.getElementById("connecter"),
  ecrits: document.getElementById("ecrits"),
  listeEcrits: document.getElementById("liste-ecrits"),
  ouverture: document.getElementById("ouverture"),
  ouvertureVers: document.getElementById("ouverture-vers"),
  ouvertureDetail: document.getElementById("ouverture-detail"),
  ouvertureCode: document.getElementById("ouverture-code"),
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
  auDemarrage: document.getElementById("au-demarrage"),
  cadenceContinue: document.getElementById("cadence-continue"),
  stats: document.getElementById("stats"),
  sonDistant: document.getElementById("son-distant"),
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

/* Le temps qu'une bonne nouvelle reste à l'écran avant de s'effacer. */
const TEMPS_ANNONCE = 6000;

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
  // Les interrupteurs des réglages lisent le même état : redessinés
  // ici, un service qui s'arrête pendant que les réglages sont ouverts
  // les fige au lieu de les laisser mentir.
  dessineConfiance();
  dessineAuDemarrage();
  dessineFaconDeServir();

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
    annonce(String(raison), true);
  } finally {
    bascule = false;
    await rafraichirEtat();
  }
}

async function copierEmpreinte() {
  await copie(vue.empreinte.textContent, vue.copier, "Copier");
}

/* Le presse-papiers peut refuser, et un bouton qui dit « Copié » sur un
   refus enverrait quelqu'un coller du vide sur l'autre ordinateur. */
async function copie(texte, bouton, motDeRepos) {
  try {
    await navigator.clipboard.writeText(texte);
    bouton.textContent = "Copié";
  } catch {
    bouton.textContent = "Copie refusée";
  }
  setTimeout(() => {
    bouton.textContent = motDeRepos;
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
  return `Ouverte depuis ${duree(session.since)}. Fermer la fenêtre termine la session.`;
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
  // Le nom en fait partie : il vient de la liste des voisins, qui peut
  // répondre après la session. Sans lui, la carte née sur l'adresse
  // gardait l'adresse pour toute la session.
  const signature = sessions
    .map((session) => `${session.fingerprint} ${nomDe(session)}`)
    .join("|");
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
  pastille.className = ordinateur.seen ? "pastille vivante" : "pastille";
  const texte = document.createElement("span");
  texte.textContent = ordinateur.name;
  nom.append(pastille, texte);

  const adresse = document.createElement("p");
  adresse.className = "legende";
  /* La pastille grise ne dit rien à elle seule : ce qui l'explique est
     écrit à côté. Cet ordinateur n'est pas absent, c'est ce réseau qui ne
     porte pas les annonces. */
  adresse.textContent = ordinateur.seen
    ? ordinateur.address
    : `${ordinateur.address} · ajouté à la main`;

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
  // Vidé à chaque ouverture : rouvert plein de la machine précédente,
  // le dialogue laissait ajouter deux fois le même ordinateur d'un
  // simple double clic sur « Se connecter ».
  vue.adresse.value = "";
  vue.nomAjout.value = "";
  vue.empreinteDistante.value = "";
  vue.motEmpreinte.textContent = "";
  ajusterAjout();
  dessineEcrits();
  vue.ajout.showModal();
  vue.empreinteDistante.focus();
}

/* Les ordinateurs écrits à la main, et de quoi les retirer. */
function dessineEcrits() {
  const ecrits = voisins.filter((ordinateur) => ordinateur.written);
  montre(vue.ecrits, ecrits.length > 0);
  vue.listeEcrits.replaceChildren(...ecrits.map(ligneEcrite));
}

function ligneEcrite(ordinateur) {
  const ligne = document.createElement("li");

  const nom = document.createElement("span");
  nom.className = "legende ecrit-nom";
  nom.textContent = `${ordinateur.name} · ${ordinateur.address}`;

  const oubli = document.createElement("button");
  oubli.type = "button";
  oubli.className = "bouton discret";
  oubli.textContent = "Oublier";
  oubli.addEventListener("click", () => oublier(ordinateur));

  ligne.append(nom, oubli);
  return ligne;
}

/* Oublier retire des deux listes : celle de l'accueil et celle des
   ordinateurs admis. Un ordinateur disparu de l'écran mais toujours
   capable d'entrer serait une promesse non tenue. */
async function oublier(ordinateur) {
  try {
    await invoke("forget", { fingerprint: ordinateur.fingerprint });
  } catch (raison) {
    vue.ajout.close();
    echoue(String(raison));
    return;
  }
  await rafraichirLeReseau();
  dessineEcrits();
}

/* Dit ce qui manque plutôt que de laisser un bouton éteint sans raison,
   et dit ce que le bouton va faire : écrire l'ordinateur pour qu'il
   puisse venir, et s'y connecter dans la foulée si une adresse est là. */
function ajusterAjout() {
  const longueur = vue.empreinteDistante.value.trim().length;
  const versLui = vue.adresse.value.trim().length > 0;

  if (longueur === 0 || longueur === TAILLE_EMPREINTE) {
    vue.motEmpreinte.textContent = "";
  } else {
    vue.motEmpreinte.textContent = `${longueur} caractères sur ${TAILLE_EMPREINTE}`;
  }

  vue.connecter.textContent = versLui ? "Se connecter" : "Autoriser";
  vue.connecter.disabled =
    longueur !== TAILLE_EMPREINTE || (versLui && occupe());
}

/* Écrire l'empreinte va dans les deux sens : elle laisse entrer cet
   ordinateur-là, et elle sert de repère pour aller vers lui. Sans le
   premier des deux, la machine d'en face serait refusée à l'arrivée et
   on n'aurait fait que la moitié du chemin. */
async function connecter(evenement) {
  evenement.preventDefault();
  vue.ajout.close();
  montre(vue.probleme, false);

  // Taillée comme elle a été comptée : une espace collée en trop ferait
  // chercher l'ordinateur sous une empreinte que personne ne porte.
  const empreinte = vue.empreinteDistante.value.trim();
  const adresse = vue.adresse.value.trim();
  try {
    await invoke("authorize", {
      fingerprint: empreinte,
      host: adresse.length > 0 ? adresse : null,
      name: vue.nomAjout.value.trim() || null,
    });
  } catch (raison) {
    echoue(String(raison));
    return;
  }
  await rafraichirLeReseau();

  if (adresse.length === 0) {
    // Autoriser ne se voit nulle part ailleurs : sans un mot, le geste
    // ferait exactement le même effet à l'écran que ne rien faire.
    annonce("Cet ordinateur est autorisé à venir sur celui-ci.");
    return;
  }
  lance(adresse, empreinte);
}

async function lance(adresse, empreinte) {
  if (occupe()) {
    return;
  }
  ouverture = true;
  dessine();
  montre(vue.probleme, false);
  // Le nom plutôt que l'adresse quand on l'a : personne ne reconnaît son
  // ordinateur portable à ses quatre nombres.
  const vise = voisins.find((v) => v.fingerprint === empreinte);
  vue.ouvertureVers.textContent = vise?.name || adresse.trim();
  etape("Ouverture du tunnel…", null);

  try {
    await invoke("connect", { host: adresse, fingerprint: empreinte });
  } catch (raison) {
    echoue(String(raison));
  }
}

/* ---- Ce qui se passe pendant l'ouverture ------------------------------ */

/* Le titre de cet écran ne bouge pas : ce qui s'y passe est toujours la
   même chose, et un titre qui change à chaque étape se lit comme des
   nouvelles alors que ce n'en est pas. Seule la ligne du bas suit. */
function etape(detail, code) {
  vue.ouvertureDetail.textContent = detail;
  vue.ouvertureCode.textContent = code ?? "";
  montre(vue.ouvertureCode, code !== null);
  montre(vue.ouverture, true);
  // La page derrière est éteinte pour de bon : l'écran d'ouverture la
  // recouvre des yeux, mais le clavier savait encore y entrer à la
  // tabulation et cliquer des boutons que personne ne voyait.
  page.inert = true;
}

/* La fenêtre n'a plus rien à raconter : ce qui se passe maintenant se lit
   dans ce que tient le service. Le bandeau ne s'efface qu'une fois la
   réponse arrivée, sinon la page se vide le temps d'un aller-retour. */
async function rangeOuverture() {
  ouverture = false;
  await rafraichirLeReseau();
  montre(vue.ouverture, false);
  page.inert = false;
}

/* Le bandeau du haut. Il sert aux deux : ce qui a échoué, et ce qui a
   réussi sans laisser de trace ailleurs à l'écran. Un message rouge pour
   dire que tout va bien se lirait comme une panne. */
let effacementAnnonce = null;

function annonce(texte, ennui = false) {
  vue.problemeTexte.textContent = texte;
  vue.probleme.classList.toggle("alerte", ennui);
  montre(vue.probleme, true);
  // Une bonne nouvelle s'efface toute seule : restée à l'écran, elle
  // finit par se lire comme un état. Un ennui reste jusqu'au geste
  // suivant, puisqu'il attend qu'on y réponde.
  clearTimeout(effacementAnnonce);
  effacementAnnonce = null;
  if (!ennui) {
    effacementAnnonce = setTimeout(() => {
      montre(vue.probleme, false);
    }, TEMPS_ANNONCE);
  }
}

function echoue(texte) {
  annonce(texte, true);
  rangeOuverture();
}

listen("session-step", ({ payload }) => {
  // L'image se relance avec de nouveaux réglages : personne n'a cliqué
  // pour ouvrir celle-là, donc c'est ici que l'écran d'ouverture revient.
  // Il porte déjà le nom de l'ordinateur, posé à la première ouverture.
  if (payload.kind === "again") {
    ouverture = true;
    dessine();
    etape("Nouveaux réglages, l'image se relance…", null);
    return;
  }
  // Une étape n'a de sens que pendant une ouverture. Un événement en
  // retard, arrivé après l'échec ou après la fin, remettait l'écran
  // d'ouverture par-dessus l'accueil, et plus rien ne l'enlevait.
  if (!ouverture) {
    return;
  }
  switch (payload.kind) {
    case "reached":
      etape(`Tunnel établi, paquets de ${payload.packet} octets.`, null);
      break;
    case "pairing":
      etape(
        payload.again
          ? "Cet ordinateur ne nous reconnaît plus : les deux font connaissance à nouveau. Rien à faire."
          : "Premier accès à cet ordinateur : les deux font connaissance. Rien à faire.",
        null,
      );
      break;
    case "pairingNeeded":
      etape(
        "Tapez ce code sur l'ordinateur que vous voulez contrôler :",
        payload.pin,
      );
      break;
    case "paired":
      etape("Les deux ordinateurs se connaissent.", null);
      break;
    case "starting":
      etape("Démarrage de l'image…", null);
      break;
    case "showing":
      etape("L'image arrive…", null);
      break;
    case "live":
      // À partir d'ici le service tient la session, et n'importe quelle
      // fenêtre la retrouve, y compris une autre que celle-ci.
      rangeOuverture();
      break;
  }
});

listen("session-ended", ({ payload }) => {
  if (payload.ok) {
    rangeOuverture();
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
  await copie(vue.journalTexte.textContent, vue.copierJournal, "Copier tout");
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

  // Ce qu'une session demanderait maintenant, dit par le produit et non
  // recalculé ici. Ces trois nombres se règlent dans le menu de la
  // session ; cette ligne ne fait que les rappeler.
  const debit = Math.round(reglages.bitrateKbps / 1000);
  vue.qualiteDetail.textContent = `${reglages.width} x ${reglages.height}, ${reglages.fps} images par seconde, ${debit} Mb/s`;

  marque("codec", reglages.codec);
  marque("display", reglages.display);
  marque("mouse", reglages.absoluteMouse ? "desktop" : "game");
  vue.stats.setAttribute(
    "aria-checked",
    reglages.statsOverlay ? "true" : "false",
  );
  vue.sonDistant.setAttribute(
    "aria-checked",
    reglages.muteFarSpeakers ? "true" : "false",
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
   l'ensemble pour n'avoir jamais à deviner ce qui est resté.

   Un choix à la fois, dans l'ordre des clics. Deux demandes parties
   ensemble voyagent chacune de leur côté, et la première écrite en
   dernier annulerait le clic le plus récent sans un mot. */
let choixEnCours = Promise.resolve();

function change(comment) {
  choixEnCours = choixEnCours.then(() => envoieLeChoix(comment));
}

async function envoieLeChoix(comment) {
  if (reglages === null) {
    return;
  }
  const veut = {
    codec: reglages.codec,
    display: reglages.display,
    absoluteMouse: reglages.absoluteMouse,
    statsOverlay: reglages.statsOverlay,
    muteFarSpeakers: reglages.muteFarSpeakers,
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
/* Un interrupteur qui appartient à la machine : il se dessine d'après ce
   que le service dit, et il se fige le temps d'une réponse plutôt que de
   montrer un état qui n'a pas encore pris. Rend de quoi le redessiner. */
function interrupteurMachine(element, commande, lu) {
  let bascule = false;

  function dessine() {
    if (etat === null) {
      return;
    }
    element.disabled = etat.unreachable !== null || bascule;
    if (!bascule) {
      element.setAttribute("aria-checked", lu(etat) ? "true" : "false");
    }
  }

  async function basculer() {
    const veut = element.getAttribute("aria-checked") !== "true";
    bascule = true;
    element.setAttribute("aria-checked", veut ? "true" : "false");
    element.disabled = true;
    montre(vue.reglagesProbleme, false);

    try {
      await invoke(commande, { on: veut });
    } catch (raison) {
      element.setAttribute("aria-checked", veut ? "false" : "true");
      soucis(String(raison));
    } finally {
      bascule = false;
      await rafraichirEtat();
      dessine();
    }
  }

  element.addEventListener("click", basculer);
  return dessine;
}

const dessineConfiance = interrupteurMachine(
  vue.confiance,
  "set_trust",
  (machine) => machine.trusting,
);

const dessineAuDemarrage = interrupteurMachine(
  vue.auDemarrage,
  "set_at_boot",
  (machine) => machine.atBoot,
);

/* Ce que cet ordinateur fait quand c'est LUI qu'on regarde. Les deux
   voyagent ensemble parce que le service les écrit ensemble : envoyer
   l'un sans l'autre remettrait le second à ce qu'il était.

   Changer l'un redémarre son moteur, donc coupe une session que
   quelqu'un aurait en cours vers cette machine. C'est dit dans la
   légende de chaque réglage plutôt qu'ici. */
async function envoieLaFaconDeServir(quoi) {
  montre(vue.reglagesProbleme, false);
  const avant = {
    steadyRate: etat !== null && etat.steadyRate,
    capture: etat === null ? "ddx" : etat.capture,
  };
  try {
    await invoke("set_serving", { ...avant, ...quoi });
  } catch (raison) {
    soucis(String(raison));
  } finally {
    await rafraichirEtat();
    dessineFaconDeServir();
  }
}

function dessineFaconDeServir() {
  if (etat === null) {
    return;
  }
  vue.cadenceContinue.disabled = etat.unreachable !== null;
  vue.cadenceContinue.setAttribute(
    "aria-checked",
    etat.steadyRate ? "true" : "false",
  );
  marque("capture", etat.capture);
}

vue.cadenceContinue.addEventListener("click", () => {
  const veut = vue.cadenceContinue.getAttribute("aria-checked") !== "true";
  vue.cadenceContinue.setAttribute("aria-checked", veut ? "true" : "false");
  envoieLaFaconDeServir({ steadyRate: veut });
});

function soucis(texte) {
  vue.reglagesProblemeTexte.textContent = texte;
  montre(vue.reglagesProbleme, true);
  // Le bandeau vit en bas d'un dialogue qui défile : amené sous les
  // yeux, sinon un refus prononcé en haut du dialogue reste invisible.
  vue.reglagesProbleme.scrollIntoView({ block: "nearest" });
}

async function ouvrirReglages() {
  montre(vue.reglagesProbleme, false);
  vue.reglages.showModal();
  dessineConfiance();
  dessineAuDemarrage();
  await Promise.all([rafraichirReglages(), rafraichirRaccourcis()]);
}

/* ---- Raccourcis clavier ------------------------------------------------ */

/* Lire et écrire une combinaison vit dans « touches.js » : le bouton
   flottant en a besoin aussi, pour dire lequel le ramène. */

async function rafraichirRaccourcis() {
  await litLePlanDuClavier();
  const raccourcis = await invoke("shortcuts");
  for (const raccourci of raccourcis) {
    const bouton = vue.reglages.querySelector(
      `[data-raccourci="${raccourci.doing}"]`,
    );
    if (bouton !== null) {
      dessineRaccourci(bouton, raccourci.combination);
    }
  }
}

function dessineRaccourci(bouton, combinaison) {
  bouton.dataset.combinaison = combinaison;
  bouton.classList.remove("ecoute");
  bouton.classList.toggle("vide", combinaison.length === 0);
  bouton.textContent =
    combinaison.length === 0 ? "Aucune" : ecritLaCombinaison(combinaison);
}

/* Une seule écoute à la fois : deux boutons qui attendent la même touche
   se la partageraient. */
let ecoute = null;

function ecouteUneCombinaison(bouton) {
  if (ecoute !== null) {
    arreteDEcouter();
  }
  ecoute = bouton;
  bouton.classList.add("ecoute");
  bouton.textContent = "Tapez la combinaison…";
  document.addEventListener("keydown", surLaTouche, true);
}

function arreteDEcouter() {
  document.removeEventListener("keydown", surLaTouche, true);
  const bouton = ecoute;
  ecoute = null;
  if (bouton !== null) {
    dessineRaccourci(bouton, bouton.dataset.combinaison ?? "");
  }
}

/* Les touches tenues seules ne valent rien : on les laisse passer et on
   attend celle qui vient avec. */
const TENUES = new Set(["Control", "Alt", "Shift", "Meta", "AltGraph"]);

function surLaTouche(evenement) {
  evenement.preventDefault();
  evenement.stopPropagation();
  if (TENUES.has(evenement.key)) {
    return;
  }

  const bouton = ecoute;
  if (evenement.key === "Escape") {
    arreteDEcouter();
    return;
  }
  if (evenement.key === "Backspace" || evenement.key === "Delete") {
    poseLaCombinaison(bouton, "");
    return;
  }

  const morceaux = [];
  if (evenement.ctrlKey) morceaux.push("Ctrl");
  if (evenement.altKey) morceaux.push("Alt");
  if (evenement.shiftKey) morceaux.push("Shift");
  if (evenement.metaKey) morceaux.push("Win");
  morceaux.push(evenement.code);
  poseLaCombinaison(bouton, morceaux.join("+"));
}

async function poseLaCombinaison(bouton, combinaison) {
  document.removeEventListener("keydown", surLaTouche, true);
  ecoute = null;
  montre(vue.reglagesProbleme, false);
  try {
    await invoke("bind", {
      doing: bouton.dataset.raccourci,
      combination: combinaison,
    });
    dessineRaccourci(bouton, combinaison);
  } catch (raison) {
    soucis(String(raison));
    dessineRaccourci(bouton, bouton.dataset.combinaison ?? "");
  }
}

for (const bouton of vue.reglages.querySelectorAll("[data-raccourci]")) {
  bouton.addEventListener("click", () => ecouteUneCombinaison(bouton));
}

/* Un clic ailleurs pendant qu'un bouton attend veut dire qu'on a changé
   d'avis. */
vue.reglages.addEventListener("click", (evenement) => {
  if (ecoute !== null && !ecoute.contains(evenement.target)) {
    arreteDEcouter();
  }
});

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
    // Celui-ci ne décrit pas ce qu'on demande aux autres mais ce que cet
    // ordinateur fait : il ne passe pas par les mêmes réglages.
    if (nom === "capture") {
      envoieLaFaconDeServir({ capture: valeur });
      return;
    }
    change((veut) => {
      if (nom === "mouse") {
        veut.absoluteMouse = valeur === "desktop";
      } else {
        veut[nom] = valeur;
      }
    });
  });
}

/* Les deux interrupteurs de la session marchent pareil : ce qui est
   affiché est ce qui est écrit, et le clic pousse l'inverse. */
for (const [bouton, cle] of [
  [vue.stats, "statsOverlay"],
  [vue.sonDistant, "muteFarSpeakers"],
]) {
  bouton.addEventListener("click", () => {
    const actif = bouton.getAttribute("aria-checked") !== "true";
    change((veut) => {
      veut[cle] = actif;
    });
  });
}

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
vue.fermerJournal.addEventListener("click", () => vue.journal.close());
// Sur la fermeture du dialogue et non sur son bouton : la touche Échap
// ferme aussi, et laissait la confirmation de vidage armée derrière un
// dialogue clos. Rouvert dans les quatre secondes, « Vider » vidait au
// premier clic.
vue.journal.addEventListener("close", reposeLeVidage);
vue.reglages.addEventListener("close", () => arreteDEcouter());
vue.rafraichirJournal.addEventListener("click", rafraichirJournal);
vue.copierJournal.addEventListener("click", copierJournal);
vue.viderJournal.addEventListener("click", viderJournal);
vue.ouvrirJournaux.addEventListener("click", () => ouvreDossier("logs"));
vue.ouvrirReglages.addEventListener("click", ouvrirReglages);
vue.fermerReglages.addEventListener("click", () => vue.reglages.close());
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
