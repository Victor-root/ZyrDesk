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
  appliquer: document.querySelector("[data-appliquer]"),
};

/* Le temps qu'un refus reste lisible avant de laisser la place. */
const TEMPS_SOUCI = 4000;

let ouvert = false;
let effacement = null;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* Le sous-menu ouvert, s'il y en a un : c'est le seul des trois qui se
   dessine, donc le seul qui entre dans la découpe. */
function laListeOuverte() {
  return document.querySelector(".liste:not(.repliee)");
}

/* Tout ce que la page peut occuper : le bloc, et les trois sous-menus,
   ouverts ou non.

   Tous les trois, et pas seulement celui qui est ouvert. La fenêtre prend
   cette taille-là une fois pour toutes et n'en change plus de la session :
   ouvrir ou fermer une liste ne la redimensionne donc pas. Mesurée sur la
   seule liste ouverte, elle changeait de taille à chaque clic dans le
   menu, et une fenêtre qui change de taille fait remettre la page en page,
   pendant quoi rien n'est dessiné. C'était le clignotement.

   Sortis du flux, les sous-menus n'entrent pas dans la mesure de
   « paquet » : c'est ici qu'on les rattrape. */
function laBoite() {
  const boites = [vue.paquet, ...document.querySelectorAll(".liste")].map(
    (element) => element.getBoundingClientRect(),
  );
  // Depuis le coin haut droit, puisque c'est par là que la fenêtre est
  // accrochée : ce qui manque est ce qui déborde vers la gauche et vers
  // le bas.
  const right = Math.max(...boites.map((une) => une.right));
  return {
    width: right - Math.min(...boites.map((une) => une.left)),
    height: Math.max(...boites.map((une) => une.bottom)),
  };
}

/* La forme que la page dessine vraiment, en vrais pixels : la plaque du
   logo, la carte du menu quand il est ouvert, et celle du sous-menu quand
   il l'est. Le reste de la fenêtre n'est dessiné par personne, et c'est
   exactement ce qu'il faut découper.

   Mesurée depuis le bord droit de la fenêtre, jamais depuis son bord
   gauche. La page est collée à droite et en haut : ce sont les deux seuls
   bords qui ne bougent pas quand la fenêtre change de largeur, puisque
   c'est par son coin haut droit qu'elle est accrochée. Le dessin est
   mesuré dans la fenêtre telle qu'elle est et découpé dans celle qu'elle
   devient ; compté depuis la gauche, il s'y retrouvait décalé de toute la
   différence, et le menu perdait son bord droit. */
function formeOccupee(echelle) {
  const morceaux = [];
  const droite = document.documentElement.clientWidth;
  const pose = (gauche, haut, large, haute, rayon) =>
    morceaux.push({
      x: Math.round((gauche - droite) * echelle),
      y: Math.round(haut * echelle),
      width: Math.round(large * echelle),
      height: Math.round(haute * echelle),
      radius: Math.round(rayon * echelle),
    });
  const carte = (element) => {
    const ou = element.getBoundingClientRect();
    const rayon = parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0;
    // Le rayon vient de la feuille de style, qui ne sait rien des
    // agrandissements ; la boîte, elle, est mesurée agrandissement
    // compris. Le logo grandit au survol, donc ses coins aussi, et un
    // rayon laissé à sa valeur de repos rognait le blanc de la plaque
    // pendant tout le mouvement. Rapporté à la largeur que l'élément a
    // sans être agrandi, il suit.
    const part = element.offsetWidth ? ou.width / element.offsetWidth : 1;
    pose(ou.left, ou.top, ou.width, ou.height, rayon * part);
  };

  // La plaque du logo telle qu'elle est rendue, agrandissement du survol
  // compris : la découpe épouse ce qui est dessiné à cet instant, et rien
  // d'autre.
  carte(vue.logo);

  if (ouvert) {
    carte(vue.menu);
    const liste = laListeOuverte();
    if (liste !== null) {
      carte(liste);
    }
  }
  return morceaux;
}

/* La fenêtre suit ce que la page peut occuper, mesuré et non deviné : le
   menu n'a pas la même hauteur selon ce qu'il contient, et les listes
   n'ont pas toutes la même largeur.

   En vrais pixels et non en pixels de page : sur un écran agrandi les
   deux ne valent pas la même chose, et une fenêtre taillée dans la
   mauvaise unité laisse voir son propre fond tout autour du bouton. */
function ajusteLaFenetre() {
  const boite = laBoite();
  const echelle = window.devicePixelRatio || 1;
  invoke("floating_size", {
    width: Math.ceil(boite.width * echelle),
    height: Math.ceil(boite.height * echelle),
    shape: formeOccupee(echelle),
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
  // Cliquer dans cette fenêtre donne le clavier à sa page, et cette
  // fenêtre n'est jamais celle que le système considère comme active :
  // rien ne le rend à l'image quand le menu se referme, et la session
  // restait sourde jusqu'à ce qu'on la rouvre. Le coeur s'en charge dès
  // qu'il sait que le menu est fermé.
  invoke("floating_menu", { open: veut }).catch(() => {});
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

/* ---- Ce que la session demande ------------------------------------------ */

/* Trois lignes, chacune ouvrant sur le côté la liste de ses valeurs, et
   une quatrième qui n'apparaît que quand ce qui est choisi n'est plus ce
   qui est à l'écran : le moteur apprend ces trois nombres au démarrage et
   jamais après, donc les poser veut dire relancer l'image.

   Les valeurs viennent du produit, les mots sont écrits ici : la liste
   des tailles et des débits vit dans zyr-proto, et la façon de les dire
   en français vit là où se lit tout ce qu'une personne lit.

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
  // Ce qui a été choisi n'est pas forcément ce qui est à l'écran : le
  // moteur apprend ces trois nombres au démarrage et jamais après. La
  // ligne qui relance l'image n'apparaît donc que quand les deux
  // diffèrent, et on peut changer plusieurs valeurs avant de la cliquer.
  montre(vue.appliquer, choix.toApply);
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

/* Relancer l'image avec ce qui est choisi. Le menu se referme dès que
   c'est parti : ce qui suit est l'image qui s'en va et revient, et un
   menu resté ouvert par-dessus serait une nappe posée sur elle. */
async function applique() {
  try {
    await invoke("apply_session");
    ouvre(false);
  } catch (raison) {
    souci(String(raison));
  }
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

vue.appliquer.addEventListener("click", applique);

/* Tout ce qui n'est ni le bouton ni le menu est du vide transparent
   posé sur l'image. Un clic dedans ne peut vouloir dire qu'une chose :
   refermer et rendre l'image. */
document.addEventListener("click", (evenement) => {
  if (ouvert && !vue.paquet.contains(evenement.target)) {
    ouvre(false);
  }
});

/* Le raccourci clavier, qui va dans les deux sens : ouvrir avec, refermer
   avec. Une combinaison qui ouvre et ne referme pas oblige à aller
   chercher la souris pour défaire ce que le clavier vient de faire.

   À l'ouverture, la fenêtre a déjà été remontrée quand ceci arrive, il ne
   reste que le menu. C'est le seul chemin de retour après avoir masqué le
   bouton. */
listen("floating-toggle", () => ouvre(!ouvert));

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
