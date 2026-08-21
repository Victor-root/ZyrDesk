/*
  Le bouton flottant. Il ne sait rien de la session : il demande, le
  coeur Rust traduit en langage du moteur, et l'image obéit.

  La fenêtre est retaillée à chaque changement d'état, parce qu'elle
  n'est rien d'autre que ce qui se voit : plus grande, elle mangerait
  des clics destinés à l'image.
*/

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const vue = {
  paquet: document.getElementById("paquet"),
  logo: document.getElementById("logo"),
  menu: document.getElementById("menu"),
  souci: document.getElementById("souci"),
  retour: document.getElementById("retour"),
  touchePleinEcran: document.getElementById("touche-plein-ecran"),
  toucheFin: document.getElementById("touche-fin"),
  valeurs: {
    asked: document.getElementById("valeur-asked"),
    bitrate: document.getElementById("valeur-bitrate"),
    codec: document.getElementById("valeur-codec"),
  },
};

/* Le temps qu'un refus reste lisible avant de laisser la place. */
const TEMPS_SOUCI = 4000;

let ouvert = false;
let effacement = null;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* La plaque du logo dans son dessin (zyrdesk.svg) : une boîte de 64, une
   plaque de 60 posée à 2 du bord, aux coins arrondis de 15. Le dessin
   fait foi, la fenêtre est découpée dessus. */
const PLAQUE = { boite: 64, marge: 2, cote: 60, rayon: 15 };

/* La forme que la page occupe vraiment, en vrais pixels depuis le coin
   de la fenêtre : la plaque du logo, et la carte du menu quand il est
   ouvert. Le reste de la fenêtre n'est dessiné par personne, et c'est
   exactement ce qu'il faut découper. */
function formeOccupee(paquet, echelle) {
  const morceaux = [];
  const pose = (gauche, haut, large, haute, rayon) =>
    morceaux.push({
      x: Math.round((gauche - paquet.left) * echelle),
      y: Math.round((haut - paquet.top) * echelle),
      width: Math.round(large * echelle),
      height: Math.round(haute * echelle),
      radius: Math.round(rayon * echelle),
    });

  // Le rectangle rendu, agrandissement du survol compris : la découpe
  // épouse ce qui est dessiné à cet instant, et rien d'autre.
  const logo = vue.logo.getBoundingClientRect();
  const part = logo.width / PLAQUE.boite;
  pose(
    logo.left + PLAQUE.marge * part,
    logo.top + PLAQUE.marge * part,
    PLAQUE.cote * part,
    PLAQUE.cote * part,
    PLAQUE.rayon * part,
  );

  if (ouvert) {
    const menu = vue.menu.getBoundingClientRect();
    const rayon = parseFloat(getComputedStyle(vue.menu).borderTopLeftRadius);
    pose(menu.left, menu.top, menu.width, menu.height, rayon || 0);
  }
  return morceaux;
}

/* La fenêtre suit ce que la page occupe, mesuré et non deviné : le menu
   n'a pas la même hauteur selon ce qu'il contient.

   En vrais pixels et non en pixels de page : sur un écran agrandi les
   deux ne valent pas la même chose, et une fenêtre taillée dans la
   mauvaise unité laisse voir son propre fond tout autour du bouton. */
function ajusteLaFenetre() {
  const boite = vue.paquet.getBoundingClientRect();
  const echelle = window.devicePixelRatio || 1;
  invoke("floating_size", {
    width: Math.ceil(boite.width * echelle),
    height: Math.ceil(boite.height * echelle),
    shape: formeOccupee(boite, echelle),
  }).catch(() => {});
}

/* Le logo change de taille par une animation : au survol, et quand une
   main le prend. La découpe la suit image par image, sinon la fenêtre
   laisse voir son fond autour du logo pendant tout le mouvement, ou le
   rogne. On s'arrête dès que plus rien ne bouge. */
let animation = null;

function suisLeLogo() {
  cancelAnimationFrame(animation);
  let avant = "";
  let immobile = 0;
  const pas = () => {
    const boite = vue.logo.getBoundingClientRect();
    const ici = `${boite.width}x${boite.height}`;
    ajusteLaFenetre();
    immobile = ici === avant ? immobile + 1 : 0;
    avant = ici;
    animation = immobile < 2 ? requestAnimationFrame(pas) : null;
  };
  pas();
}

function ouvre(veut) {
  ouvert = veut;
  vue.menu.classList.toggle("repliee", !veut);
  vue.logo.setAttribute("aria-expanded", veut ? "true" : "false");
  if (!veut) {
    montre(vue.souci, false);
  }
  // Après le dessin : une fenêtre taillée sur l'état d'avant serait
  // trop petite d'un menu.
  requestAnimationFrame(ajusteLaFenetre);
}

function souci(texte) {
  vue.souci.textContent = texte;
  montre(vue.souci, true);
  requestAnimationFrame(ajusteLaFenetre);
  clearTimeout(effacement);
  effacement = setTimeout(() => {
    montre(vue.souci, false);
    requestAnimationFrame(ajusteLaFenetre);
  }, TEMPS_SOUCI);
}

async function demande(acte) {
  try {
    await invoke("floating_act", { what: acte });
    ouvre(false);
  } catch (raison) {
    souci(String(raison));
  }
}

/* ---- Ce que la prochaine session demandera ------------------------------ */

/* Les valeurs viennent du produit, les mots sont écrits ici : la liste
   des tailles et des débits vit dans zyr-proto, et la façon de les dire
   en français vit là où se lit tout ce qu'une personne lit.

   « Écran » se dit avec ce à quoi il revient sur cet ordinateur-ci : le
   mot seul ne dit pas si on demande du 4K ou du 1080p, et c'est
   exactement ce qu'on veut savoir avant d'ouvrir la session. */
function ditLaTaille(choix) {
  const taille = `${choix.width} x ${choix.height}`;
  return choix.asked === "screen" ? `Écran, ${taille}` : taille;
}

function ditLeDebit(choix) {
  return `${Math.round(choix.bitrateKbps / 1000)} Mb/s`;
}

function ditLeCodec(choix) {
  return choix.codec === "auto" ? "Automatique" : choix.codec;
}

function poseLesValeurs(choix) {
  vue.valeurs.asked.textContent = ditLaTaille(choix);
  vue.valeurs.bitrate.textContent = ditLeDebit(choix);
  vue.valeurs.codec.textContent = ditLeCodec(choix);
  // Les valeurs n'ont pas toutes la même longueur : la fenêtre suit ce
  // que la page occupe, sinon elle rogne la plus longue.
  requestAnimationFrame(ajusteLaFenetre);
}

/* Un cran à la fois, dans l'ordre des clics. Deux demandes parties
   ensemble voyagent chacune de leur côté, et la première écrite en
   dernier annulerait le clic le plus récent sans un mot. */
let cranEnCours = Promise.resolve();

function avance(lequel) {
  cranEnCours = cranEnCours.then(async () => {
    try {
      poseLesValeurs(await invoke("step_session_choice", { which: lequel }));
    } catch (raison) {
      souci(String(raison));
    }
  });
}

async function litLesValeurs() {
  try {
    poseLesValeurs(await invoke("session_choice"));
  } catch {
    /* Le service ne répond pas : la session qui s'ouvre le dira bien
       mieux que trois lignes de menu. */
  }
}

for (const item of document.querySelectorAll("[data-reglage]")) {
  // Le menu reste ouvert, à la différence des actions : on essaie
  // plusieurs crans de suite en regardant l'image.
  item.addEventListener("click", () => avance(item.dataset.reglage));
}

/* ---- Prendre et déplacer le bouton ------------------------------------- */

/* Le bouton se prend et se pose où on veut sur l'image. Tout le geste
   est suivi côté Rust, et non ici : cette fenêtre fait cinquante
   pixels, la souris en sort au premier mouvement, et ce qu'une vue web
   rapporte de la position d'un pointeur n'est pas toujours l'endroit où
   ce pointeur se trouve à l'écran. La page dit seulement quand le
   bouton est pris, et apprend à la fin si c'était un déplacement ou un
   simple clic. */
let pris = false;

for (const quand of ["pointerenter", "pointerleave", "pointerdown"]) {
  vue.logo.addEventListener(quand, suisLeLogo);
}

vue.logo.addEventListener("pointerdown", async (evenement) => {
  if (evenement.button !== 0) {
    return;
  }
  // Un geste à la fois. Le relâchement se produit souvent hors de cette
  // fenêtre, donc la page ne le voit pas : sans ce verrou, deux prises
  // se superposaient et le bouton finissait par ne plus répondre.
  if (pris) {
    return;
  }
  pris = true;
  vue.logo.classList.add("pris");
  try {
    if (await invoke("floating_grab")) {
      ouvre(!ouvert);
    }
  } catch {
    /* Une prise refusée n'est pas un déplacement : rien à dire. */
  } finally {
    pris = false;
    vue.logo.classList.remove("pris");
    suisLeLogo();
  }
});

/* Le clavier ne passe pas par le pointeur : la touche Entrée produit un
   clic sans lui, et c'est le seul qui arrive jusqu'ici. */
vue.logo.addEventListener("click", (evenement) => {
  if (evenement.detail === 0) {
    ouvre(!ouvert);
  }
});

for (const item of document.querySelectorAll("[data-acte]")) {
  item.addEventListener("click", () => demande(item.dataset.acte));
}

for (const item of document.querySelectorAll("[data-cacher]")) {
  item.addEventListener("click", () => {
    ouvre(false);
    invoke("floating_hide").catch(() => {});
  });
}

/* Tout ce qui n'est ni le bouton ni le menu est du vide transparent
   posé sur l'image. Un clic dedans ne peut vouloir dire qu'une chose :
   refermer et rendre l'image. */
document.addEventListener("click", (evenement) => {
  if (ouvert && !vue.paquet.contains(evenement.target)) {
    ouvre(false);
  }
});

/* Le raccourci clavier : la fenêtre a déjà été remontrée quand ceci
   arrive, il ne reste que le menu à ouvrir. C'est le seul chemin de
   retour après avoir masqué le bouton. */
listen("floating-open", () => ouvre(true));

/* Une nouvelle session reprend cette fenêtre telle que la précédente
   l'a laissée. Ce qui restait d'elle part : le menu ouvert surtout, qui
   gardait la fenêtre à sa taille de menu, une nappe invisible posée sur
   l'image qui avalait les clics. */
listen("floating-reset", () => {
  clearTimeout(effacement);
  montre(vue.souci, false);
  ouvre(false);
});

/* Les combinaisons se lisent dans le menu, à côté de ce qu'elles font.
   Lues à chaque ouverture de session plutôt que gravées, puisqu'elles se
   choisissent dans les réglages. Celle qui ramène le bouton compte
   double : masquer sans savoir comment revenir est un aller simple. */
async function ditLesRaccourcis() {
  const raccourcis = await invoke("shortcuts").catch(() => []);
  await litLePlanDuClavier();
  const dit = (quoi, ou, sinon) => {
    const trouve = raccourcis.find((raccourci) => raccourci.doing === quoi);
    ou.textContent = trouve?.combination
      ? ecritLaCombinaison(trouve.combination)
      : sinon;
  };
  dit("menu", vue.retour, "jusqu'à la fin");
  dit("fullscreen", vue.touchePleinEcran, "");
  dit("end", vue.toucheFin, "rend le bureau distant");
  // Ces combinaisons allongent les entrées du menu, donc élargissent la
  // fenêtre. Mesurée ici, une fois, plutôt qu'au premier clic : la
  // fenêtre garde ensuite sa taille pour toute la session, ce qui est
  // ce qui fait que cliquer sur le bouton ne clignote pas.
  requestAnimationFrame(ajusteLaFenetre);
}

ditLesRaccourcis();
litLesValeurs();

/* Une nouvelle session peut avoir été ouverte après un changement fait
   depuis une autre fenêtre : les trois lignes se relisent à chaque fois
   plutôt que gardées de la session d'avant. */
listen("floating-reset", litLesValeurs);

ajusteLaFenetre();
