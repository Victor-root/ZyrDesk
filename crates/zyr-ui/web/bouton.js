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
  chiffres: document.getElementById("chiffres"),
  flux: document.getElementById("flux"),
  souris: document.getElementById("souris"),
  son: document.getElementById("son"),
};

/* Les quatre chiffres de la barre, dans l'ordre où ils se lisent : ce que
   coûte une image ici, ce qu'elle a coûté là-bas, ce qu'il y a entre les
   deux, et ce que le fil porte vraiment.

   Le mot et l'unité vivent ici et nulle part ailleurs. Le moteur envoie
   des nombres, la page décide comment on les lit. */
const MESURES = [
  { cle: "decodeMs", mot: "Décodage", unite: "ms", apres: 2 },
  { cle: "hostMs", mot: "Encodage", unite: "ms", apres: 2 },
  { cle: "networkMs", mot: "Réseau", unite: "ms", apres: 0 },
  { cle: "bitrateMbps", mot: "Débit", unite: "Mb/s", apres: 2 },
];

/* Une mesure absente s'écrit ainsi. Le moteur ne dit rien plutôt que zéro
   quand il n'a rien mesuré, et zéro serait un mensonge : une seconde sans
   image décodée n'a pas un temps de décodage nul. */
const RIEN = "-";

/* Le rythme du moteur, qui écrit une fois par seconde. Demander plus
   souvent relirait le même fichier pour le même nombre. */
const RYTHME = 1000;

let mesure = null;

/* Le temps qu'un refus reste lisible avant de laisser la place. */
const TEMPS_SOUCI = 4000;

let ouvert = false;
let effacement = null;

/* La barre se remplit tant que le menu est ouvert, et pas une seconde de
   plus : des chiffres que personne ne regarde ne valent ni le fichier ni
   le réveil. */
function suisLesMesures(veut) {
  clearInterval(mesure);
  mesure = null;
  if (!veut) {
    return;
  }
  litLesMesures();
  mesure = setInterval(litLesMesures, RYTHME);
}

async function litLesMesures() {
  let dit = {};
  try {
    dit = await invoke("session_measures");
  } catch {
    dit = {};
  }
  vue.chiffres.replaceChildren(
    ...MESURES.map((quoi) => {
      const bloc = document.createElement("span");
      const mot = document.createElement("b");
      mot.textContent = quoi.mot;
      const valeur = document.createElement("em");
      const nombre = dit[quoi.cle];
      valeur.textContent =
        typeof nombre === "number"
          ? `${nombre.toFixed(quoi.apres)} ${quoi.unite}`
          : RIEN;
      bloc.append(mot, valeur);
      return bloc;
    }),
  );
  vue.flux.textContent = leFlux(dit);
  // Les chiffres ont une largeur fixe et la ligne du dessous est la plus
  // courte du menu, donc rien ne bouge d'ordinaire ; ceci est le filet
  // pour le jour où quelque chose bougera, la fenêtre étant taillée sur
  // ce que la page dessine et pas sur ce qu'elle dessinait avant.
  suisLeDessin();
}

/* La ligne grise sous les chiffres : de quoi est faite l'image. Ce qui
   manque ne laisse pas de trou, il ne s'écrit pas. */
function leFlux(dit) {
  const bouts = [];
  if (dit.codec) {
    bouts.push(dit.codec);
  }
  if (dit.width && dit.height) {
    bouts.push(`${dit.width}x${dit.height}`);
  }
  if (typeof dit.fps === "number") {
    bouts.push(`${dit.fps.toFixed(0)} images/s`);
  }
  return bouts.join(" · ");
}

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* Ce que le logo dessine, dans son propre dessin (zyrdesk.svg) : deux
   écrans aux coins arrondis, décalés en diagonale, et rien entre eux. Le
   dessin fait foi, la fenêtre est découpée dessus : ces nombres se
   relisent dans le SVG à chaque fois qu'il change, sinon la découpe passe
   à côté de ce qu'elle est censée épouser.

   Ce sont les rectangles contour compris, donc le rectangle du SVG élargi
   de la moitié de son trait de chaque côté, et rapportés au coin de la
   vue plutôt qu'à l'origine du dessin. Pour l'écran du fond : x 118 - 14
   - 36, y 70 - 14 - 36, largeur 328 + 28, coins 68 + 14. */
const LOGO = {
  boite: 440,
  ecrans: [
    { x: 68, y: 20, large: 356, haute: 274, rayon: 82 },
    { x: 16, y: 146, large: 356, haute: 274, rayon: 82 },
  ],
};

/* Tout ce que la page peut occuper, qui est le bloc et rien d'autre.
   Il y avait trois sous-menus à rattraper ici, sortis du flux et donc
   invisibles à la mesure du bloc, et ils s'ouvraient vers la gauche parce
   que c'était le seul côté où la fenêtre pouvait grandir sans que le logo
   bouge. Les curseurs qui les remplacent sont dans le menu, à leur place,
   et il n'y a plus rien qui déborde. */
function laBoite() {
  const boite = vue.paquet.getBoundingClientRect();
  return { width: boite.width, height: boite.bottom };
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
  // Arrondi vers l'intérieur, jamais vers l'extérieur : les bords sont
  // remontés et les tailles rabotées. Un morceau arrondi vers le dehors
  // réclame une colonne de pixels que la page n'a pas peinte, et le
  // système la remplit de son propre blanc avant que la vue web ait
  // repeint. C'est le liseré clair qu'on voyait sur la gauche du bouton,
  // surtout en le déplaçant : à chaque pas, la fenêtre bouge, le système
  // recopie ce qu'il peut et efface la bande découverte au pinceau blanc.
  // Un pixel peint en moins ne se voit pas ; un pixel blanc de trop, si.
  const pose = (gauche, haut, large, haute, rayon) =>
    morceaux.push({
      x: Math.ceil((gauche - droite) * echelle),
      y: Math.ceil(haut * echelle),
      width: Math.floor(large * echelle),
      height: Math.floor(haute * echelle),
      radius: Math.round(rayon * echelle),
    });
  const carte = (element) => {
    const ou = element.getBoundingClientRect();
    const rayon = parseFloat(getComputedStyle(element).borderTopLeftRadius);
    pose(ou.left, ou.top, ou.width, ou.height, rayon || 0);
  };

  // Les deux écrans tels qu'ils sont rendus, agrandissement du survol
  // compris : la découpe épouse ce qui est dessiné à cet instant, et rien
  // d'autre. Le vide entre les deux n'est pas dessiné, donc il n'est pas
  // découpé, donc les clics y passent jusqu'à l'image.
  const logo = vue.logo.getBoundingClientRect();
  const part = logo.width / LOGO.boite;
  for (const ecran of LOGO.ecrans) {
    pose(
      logo.left + ecran.x * part,
      logo.top + ecran.y * part,
      ecran.large * part,
      ecran.haute * part,
      ecran.rayon * part,
    );
  }

  if (ouvert) {
    carte(vue.menu);
  }
  return morceaux;
}

/* La fenêtre suit ce que la page peut occuper, mesuré et non deviné : le
   menu n'a pas la même hauteur selon ce qu'il contient, et les valeurs
   écrites au-dessus des curseurs n'ont pas toutes la même longueur.

   En vrais pixels et non en pixels de page : sur un écran agrandi les
   deux ne valent pas la même chose, et une fenêtre taillée dans la
   mauvaise unité laisse voir son propre fond tout autour du bouton. */
function ajusteLaFenetre() {
  const boite = laBoite();
  const echelle = window.devicePixelRatio || 1;
  const forme = formeOccupee(echelle);
  invoke("floating_size", {
    width: Math.ceil(boite.width * echelle),
    height: Math.ceil(boite.height * echelle),
    shape: forme,
  }).catch(() => {});
  // Ce qui vient d'être envoyé, en un mot : c'est à ça que l'appelant
  // voit si le dessin bouge encore.
  return JSON.stringify([boite.width, boite.height, forme]);
}

/* Ce que la page dessine, suivi image par image jusqu'à ce que plus rien
   ne bouge.

   Jamais sur une seule image, et c'est la règle : la fenêtre est
   découpée sur ce que la page dessine, donc la découpe doit être posée
   après que la page l'a dessiné, pas avant. Une image d'avance et
   Windows redessine la fenêtre alors que la page en est encore à celle
   d'avant, ce qui laisse un morceau de son fond dans la découpe jusqu'au
   prochain coup de pinceau. C'était la trace blanche vue derrière le
   bouton après avoir cliqué une entrée du menu : le curseur était loin
   du logo, donc plus rien ne redessinait, et elle restait là jusqu'au
   survol suivant.

   Le logo change aussi de taille par une animation, au survol et quand
   une main le prend, et la suivre est exactement la même chose. On
   s'arrête dès que deux images de suite dessinent la même forme. */
let animation = null;

function suisLeDessin() {
  cancelAnimationFrame(animation);
  let avant = "";
  let immobile = 0;
  const pas = () => {
    const ici = ajusteLaFenetre();
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
  suisLesMesures(veut);
  if (!veut) {
    montre(vue.souci, false);
  } else {
    // Les deux ont pu bouger pendant que le menu était fermé : le
    // raccourci du produit bascule la souris, et le mélangeur de Windows
    // est ouvert à tout le monde. Les interrupteurs doivent dire où l'on
    // en est, pas où l'on en était la dernière fois qu'on a regardé.
    litLaSouris();
    litLeSon();
  }
  // Et la fenêtre suit ce que la page dessine maintenant, jusqu'à ce
  // qu'elle ait fini de le dessiner : taillée sur l'état d'avant, elle
  // serait trop petite d'un menu, et taillée avant que la page ait posé
  // le nouvel état elle garde une trace de l'ancien.
  suisLeDessin();
}

function souci(texte) {
  vue.souci.textContent = texte;
  montre(vue.souci, true);
  suisLeDessin();
  clearTimeout(effacement);
  effacement = setTimeout(() => {
    montre(vue.souci, false);
    suisLeDessin();
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

/* Trois curseurs, et une quatrième ligne qui n'apparaît que quand ce qui
   est choisi n'est plus ce qui est à l'écran : le moteur apprend ces
   trois nombres au démarrage et jamais après, donc les poser veut dire
   relancer l'image.

   Les valeurs viennent du produit, les mots sont écrits ici : la liste
   des tailles et des débits vit dans zyr-proto, et la façon de les dire
   en français vit là où se lit tout ce qu'une personne lit.

   « Écran » se dit avec ce à quoi il revient sur cet ordinateur-ci : le
   mot seul ne dit pas si on demande du 4K ou du 1080p, et c'est
   exactement ce qu'on veut savoir avant d'ouvrir la session. */
const LIGNES = {
  asked: {
    // Du plus petit au plus grand, de gauche à droite. Le produit les
    // offre dans son ordre à lui, qui n'est pas celui-là : sur une barre,
    // pousser vers la droite veut dire demander plus, et l'inverse se lit
    // comme une panne.
    valeurs: (menu) =>
      [...menu.sizes]
        .sort(
          (une, autre) => une.width * une.height - autre.width * autre.height,
        )
        .map((taille) => taille.value),
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

/* Ce que le produit propose, demandé une fois : les crans ne changent pas
   d'un clic à l'autre, et les refaire à chaque fois ferait clignoter la
   fenêtre à chaque ouverture. */
let leMenu = null;

function bloc(nom) {
  return document.querySelector(`.reglage[data-reglage="${nom}"]`);
}

/* Chaque curseur va de zéro au nombre de valeurs moins une : ce sont des
   crans nommés et non des nombres, et les débits ne sont pas espacés
   régulièrement. La valeur en place est écrite au-dessus, parce qu'un
   curseur seul ne dit pas ce qu'il vaut. */
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
    const cran = ligne.valeurs(leMenu).indexOf(ou);
    ici.querySelector("[data-valeur]").textContent = ligne.dit(leMenu, ou);
    const curseur = ici.querySelector("[data-curseur]");
    // Une valeur que le produit n'offre pas ne se pose sur aucun cran :
    // le curseur reste où il est plutôt que de sauter au premier et de
    // faire croire que c'est là qu'on en est.
    if (cran >= 0) {
      curseur.value = String(cran);
    }
  }
  // Ce qui a été choisi n'est pas forcément ce qui est à l'écran : le
  // moteur apprend ces trois nombres au démarrage et jamais après. La
  // ligne qui relance l'image n'apparaît donc que quand les deux
  // diffèrent, et on peut changer plusieurs valeurs avant de la cliquer.
  montre(vue.appliquer, choix.toApply);
  // Les valeurs n'ont pas toutes la même longueur : la fenêtre suit ce
  // que la page occupe, sinon elle rogne la plus longue.
  suisLeDessin();
}

/* Un choix à la fois, dans l'ordre des gestes. Deux demandes parties
   ensemble voyagent chacune de leur côté, et la première écrite en
   dernier annulerait le geste le plus récent sans un mot. */
let choixEnCours = Promise.resolve();

function choisis(nom, valeur) {
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

function batisLesCurseurs() {
  for (const [nom, ligne] of Object.entries(LIGNES)) {
    const ici = bloc(nom);
    if (!ici) {
      continue;
    }
    const valeurs = ligne.valeurs(leMenu);
    const curseur = ici.querySelector("[data-curseur]");
    curseur.min = "0";
    curseur.max = String(Math.max(valeurs.length - 1, 0));
    curseur.step = "1";
    // Le mot suit le pouce pendant qu'on le pousse, et le choix ne part
    // qu'une fois lâché. Sans le premier, on pousse à l'aveugle ; sans
    // le second, traverser toute la barre enverrait une demande par cran
    // au service, et la dernière écrite ne serait pas la dernière voulue.
    curseur.addEventListener("input", () => {
      const valeur = valeurs[Number(curseur.value)];
      if (valeur !== undefined) {
        ici.querySelector("[data-valeur]").textContent = ligne.dit(
          leMenu,
          valeur,
        );
      }
    });
    curseur.addEventListener("change", () => {
      const valeur = valeurs[Number(curseur.value)];
      if (valeur !== undefined) {
        choisis(nom, valeur);
      }
    });
  }
}

/* ---- Les deux interrupteurs du menu ------------------------------------ */

/* Deux mots côte à côte, celui qui est en place allumé. La ligne d'avant
   disait « souris bureau ou jeu » et basculait à l'aveugle : elle
   annonçait ce que le clic ferait, jamais où l'on en était, et les deux
   modes ne se distinguent pas à l'oeil sur un bureau immobile.

   Souris et son marchent pareil, donc se construisent pareil. Le côté de
   droite est celui qui vaut « oui » : jeu pour la souris, coupé pour le
   son. L'état se relit à chaque ouverture du menu plutôt que retenu ici,
   parce qu'il peut changer sans passer par cette page. Et cliquer le
   côté où l'on est déjà ne fait rien, comme tout interrupteur qu'on
   pousse du côté où il est déjà. */
function interrupteur(element, cle, lis, bascule) {
  const cotes = [...element.querySelectorAll(`[data-${cle}]`)];
  const oui = cotes[cotes.length - 1].dataset[cle];
  let ou = null;

  const pose = (vrai) => {
    ou = vrai;
    for (const cote of cotes) {
      cote.setAttribute(
        "aria-checked",
        (cote.dataset[cle] === oui) === vrai ? "true" : "false",
      );
    }
  };

  for (const cote of cotes) {
    cote.addEventListener("click", async () => {
      const vrai = cote.dataset[cle] === oui;
      if (ou === vrai) {
        return;
      }
      try {
        await bascule();
        pose(vrai);
      } catch (raison) {
        souci(String(raison));
      }
    });
  }

  /* Ce qu'il faut appeler pour que l'interrupteur dise où l'on en est.
     Sans session il n'y a rien à montrer, et le menu ne s'ouvre pas sans
     session. */
  return async () => {
    try {
      pose(await lis());
    } catch {
      /* Rien à montrer, donc rien de montré. */
    }
  };
}

const litLaSouris = interrupteur(
  vue.souris,
  "souris",
  () => invoke("floating_mouse"),
  () => invoke("floating_act", { what: "mouse" }),
);

const litLeSon = interrupteur(
  vue.son,
  "son",
  () => invoke("floating_sound"),
  () => invoke("floating_act", { what: "sound" }),
);

async function litLeMenu() {
  try {
    leMenu = await invoke("session_menu");
  } catch {
    /* Le service ne répond pas : la session qui s'ouvre le dira bien
       mieux que trois lignes de menu. */
    return;
  }
  batisLesCurseurs();
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
  vue.logo.addEventListener(quand, suisLeDessin);
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
    suisLeDessin();
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

/* Et un clic sur la session referme aussi, ce qui est ce que fait tout
   menu ouvert depuis que les menus existent. Il fallait jusqu'ici
   recliquer le logo.

   Le clavier suffit à le savoir, sans rien guetter et sans crochet posé
   sur la machine : cette fenêtre n'est jamais activée, mais cliquer
   dedans donne le clavier à sa page, et cliquer ailleurs le lui reprend.
   « Ailleurs » veut dire l'image, une autre application, le bureau : tous
   les endroits où un menu resté ouvert n'a plus rien à faire. */
window.addEventListener("blur", () => {
  if (ouvert) {
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
  suisLeDessin();
}

ditLesRaccourcis();
litLeMenu();

/* Une nouvelle session peut avoir été ouverte après un changement fait
   depuis une autre fenêtre, et l'écran sur lequel elle s'ouvre peut ne
   pas être le même : les trois lignes se relisent à chaque fois plutôt
   que gardées de la session d'avant. */
listen("floating-reset", litLeMenu);

suisLeDessin();
