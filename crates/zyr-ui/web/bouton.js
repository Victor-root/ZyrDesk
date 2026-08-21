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

/* Le sous-menu ouvert, s'il y en a un. Sorti du flux, il n'entre pas
   dans la mesure de « paquet » : c'est ici qu'on le rattrape. */
function laListeOuverte() {
  return document.querySelector(".liste:not(.repliee)");
}

/* Tout ce que la page occupe : le bloc, et le sous-menu ouvert qui en
   déborde par la gauche. La fenêtre est accrochée par son coin haut
   droit, donc déborder par la gauche et par le bas est la seule chose
   qu'elle sache absorber sans que le logo bouge. */
function laBoite() {
  const paquet = vue.paquet.getBoundingClientRect();
  const liste = laListeOuverte();
  if (liste === null) {
    return paquet;
  }
  const carte = liste.getBoundingClientRect();
  const left = Math.min(paquet.left, carte.left);
  const top = Math.min(paquet.top, carte.top);
  const right = Math.max(paquet.right, carte.right);
  const bottom = Math.max(paquet.bottom, carte.bottom);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

/* La forme que la page occupe vraiment, en vrais pixels depuis le coin
   de la fenêtre : la plaque du logo, la carte du menu quand il est
   ouvert, et celle du sous-menu quand il l'est. Le reste de la fenêtre
   n'est dessiné par personne, et c'est exactement ce qu'il faut
   découper. */
function formeOccupee(boite, echelle) {
  const morceaux = [];
  const pose = (gauche, haut, large, haute, rayon) =>
    morceaux.push({
      x: Math.round((gauche - boite.left) * echelle),
      y: Math.round((haut - boite.top) * echelle),
      width: Math.round(large * echelle),
      height: Math.round(haute * echelle),
      radius: Math.round(rayon * echelle),
    });
  const carte = (element) => {
    const ou = element.getBoundingClientRect();
    const rayon = parseFloat(getComputedStyle(element).borderTopLeftRadius);
    pose(ou.left, ou.top, ou.width, ou.height, rayon || 0);
  };

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
    carte(vue.menu);
    const liste = laListeOuverte();
    if (liste !== null) {
      carte(liste);
    }
  }
  return morceaux;
}

/* La fenêtre suit ce que la page occupe, mesuré et non deviné : le menu
   n'a pas la même hauteur selon ce qu'il contient, et un sous-menu
   ouvert l'élargit.

   En vrais pixels et non en pixels de page : sur un écran agrandi les
   deux ne valent pas la même chose, et une fenêtre taillée dans la
   mauvaise unité laisse voir son propre fond tout autour du bouton. */
function ajusteLaFenetre() {
  const boite = laBoite();
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
    // Un sous-menu laissé ouvert rouvrirait le menu avec lui, donc une
    // nappe invisible posée sur l'image, qui avale les clics.
    ouvreLaListe(null, false);
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

/* Trois lignes, chacune ouvrant sur le côté la liste de ses valeurs.
   Les valeurs
   viennent du produit, les mots sont écrits ici : la liste des tailles
   et des débits vit dans zyr-proto, et la façon de les dire en français
   vit là où se lit tout ce qu'une personne lit.

   « Écran » se dit avec ce à quoi il revient sur cet ordinateur-ci : le
   mot seul ne dit pas si on demande du 4K ou du 1080p, et c'est
   exactement ce qu'on veut savoir avant d'ouvrir la session. */
const COCHE =
  '<svg class="valeur-coche" viewBox="0 0 24 24" aria-hidden="true"><path d="m5 13 4 4L19 7"/></svg>';

const LIGNES = {
  asked: {
    valeurs: (menu) => menu.sizes.map((taille) => taille.value),
    dit: (menu, valeur) => {
      const taille = menu.sizes.find((une) => une.value === valeur);
      if (!taille) {
        return valeur;
      }
      const nombres = `${taille.width} x ${taille.height}`;
      return valeur === "screen" ? `Écran, ${nombres}` : nombres;
    },
    ou: (choix) => choix.asked,
  },
  bitrate: {
    valeurs: (menu) => menu.rates.map(String),
    dit: (_menu, valeur) => `${Math.round(Number(valeur) / 1000)} Mb/s`,
    ou: (choix) => String(choix.bitrateKbps),
  },
  codec: {
    valeurs: (menu) => menu.codecs,
    dit: (_menu, valeur) => (valeur === "auto" ? "Automatique" : valeur),
    ou: (choix) => choix.codec,
  },
};

/* Ce que le produit propose, demandé une fois : les listes ne changent
   pas d'un clic à l'autre, et les rebâtir à chaque fois ferait clignoter
   la fenêtre à chaque ouverture. */
let leMenu = null;

function bloc(nom) {
  return document.querySelector(`.reglage[data-reglage="${nom}"]`);
}

/* La ligne dit où elle en est, et sa liste marque la valeur en place.
   Une liste sans marque obligerait à se souvenir de ce qu'on avait mis. */
function poseLesValeurs(choix) {
  if (leMenu === null) {
    return;
  }
  leMenu.now = choix;
  for (const [nom, ligne] of Object.entries(LIGNES)) {
    const ici = bloc(nom);
    if (!ici) {
      continue;
    }
    const ou = ligne.ou(choix);
    ici.querySelector("[data-valeur]").textContent = ligne.dit(leMenu, ou);
    for (const bouton of ici.querySelectorAll(".valeur")) {
      bouton.setAttribute(
        "aria-checked",
        bouton.dataset.valeurBrute === ou ? "true" : "false",
      );
    }
  }
  // Les valeurs n'ont pas toutes la même longueur : la fenêtre suit ce
  // que la page occupe, sinon elle rogne la plus longue.
  requestAnimationFrame(ajusteLaFenetre);
}

/* Une seule à la fois, comme n'importe quel sous-menu : deux ouvertes
   se recouvriraient, puisqu'elles s'ouvrent toutes du même côté. */
function ouvreLaListe(nom, veut) {
  for (const [autre, _] of Object.entries(LIGNES)) {
    const ici = bloc(autre);
    if (!ici) {
      continue;
    }
    const ouverte = autre === nom && veut;
    ici.querySelector(".liste").classList.toggle("repliee", !ouverte);
    ici
      .querySelector("[data-ouvre]")
      .setAttribute("aria-expanded", ouverte ? "true" : "false");
  }
  requestAnimationFrame(ajusteLaFenetre);
}

/* Un choix à la fois, dans l'ordre des clics. Deux demandes parties
   ensemble voyagent chacune de leur côté, et la première écrite en
   dernier annulerait le clic le plus récent sans un mot. */
let choixEnCours = Promise.resolve();

function choisis(nom, valeur) {
  ouvreLaListe(nom, false);
  choixEnCours = choixEnCours.then(async () => {
    try {
      poseLesValeurs(
        await invoke("choose_session", { which: nom, value: valeur }),
      );
    } catch (raison) {
      souci(String(raison));
    }
  });
}

function batisLesListes() {
  for (const [nom, ligne] of Object.entries(LIGNES)) {
    const ici = bloc(nom);
    if (!ici) {
      continue;
    }
    const liste = ici.querySelector(".liste");
    liste.replaceChildren();
    for (const valeur of ligne.valeurs(leMenu)) {
      const bouton = document.createElement("button");
      bouton.type = "button";
      bouton.className = "valeur";
      bouton.setAttribute("role", "menuitemradio");
      bouton.dataset.valeurBrute = valeur;
      bouton.innerHTML = `${COCHE}<span class="valeur-mot"></span>`;
      bouton.querySelector(".valeur-mot").textContent = ligne.dit(
        leMenu,
        valeur,
      );
      bouton.addEventListener("click", () => choisis(nom, valeur));
      liste.append(bouton);
    }
    ici
      .querySelector("[data-ouvre]")
      .addEventListener("click", () =>
        ouvreLaListe(
          nom,
          ici.querySelector(".liste").classList.contains("repliee"),
        ),
      );
  }
}

async function litLeMenu() {
  try {
    leMenu = await invoke("session_menu");
  } catch {
    /* Le service ne répond pas : la session qui s'ouvre le dira bien
       mieux que trois lignes de menu. */
    return;
  }
  batisLesListes();
  poseLesValeurs(leMenu.now);
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
litLeMenu();

/* Une nouvelle session peut avoir été ouverte après un changement fait
   depuis une autre fenêtre, et l'écran sur lequel elle s'ouvre peut ne
   pas être le même : les trois lignes se relisent à chaque fois plutôt
   que gardées de la session d'avant. */
listen("floating-reset", litLeMenu);

ajusteLaFenetre();
