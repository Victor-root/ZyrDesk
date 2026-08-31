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
  clavier: document.getElementById("clavier"),
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

/* De quel côté le menu s'ouvre. Le coeur le décide, parce que lui seul
   sait où cette fenêtre a été posée sur l'écran ; la page le lui demande
   en lui disant ce qu'elle dessine, qui est la seule conversation que
   les deux ont sur la forme de cette fenêtre.

   Quand il s'ouvre vers le haut, le logo reste où la main l'a laissé et
   le menu pousse au-dessus. La page est alors collée au bas de la
   fenêtre, et tout ce qu'elle mesure se compte depuis ce bas-là : c'est
   le haut de la fenêtre qui bouge quand elle grandit. */
let versLeHaut = false;

function poseLeSens(veut) {
  if (versLeHaut === veut) {
    return false;
  }
  versLeHaut = veut;
  vue.paquet.classList.toggle("vers-le-haut", veut);
  return true;
}

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

/* Ce que la découpe prend au-delà de ce que la page peint, en vrais
   pixels. Voir `pose`. */
const MARGE = 1;

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

   Rien d'autre, et c'est une règle à tenir. Il y avait autrefois trois
   sous-menus à rattraper ici, sortis du flux et donc invisibles à la
   mesure du bloc. Celui de la résolution est dans le flux, dans la
   rangée, à côté du menu : le bloc l'enferme, donc cette mesure le voit
   sans rien savoir de lui. Un panneau qu'on poserait un jour hors du
   flux serait à rattraper ici, et c'est exactement ce qu'il ne faut pas
   refaire. */
function laBoite() {
  const boite = vue.paquet.getBoundingClientRect();
  // Compté depuis le bord auquel la page est collée. Vers le haut, le
  // bloc touche le bas de la fenêtre et déborde par le sommet tant
  // qu'elle est trop courte : la distance du haut du bloc au bas de la
  // fenêtre est alors exactement sa hauteur, débordement compris.
  const haut = versLeHaut
    ? document.documentElement.clientHeight - boite.top
    : boite.bottom;
  return { width: boite.width, height: haut };
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
   différence, et le menu perdait son bord droit.

   Ce qui est mesuré ici est ce qui va être dessiné, et rien d'autre. La
   découpe passait auparavant par l'intersection avec le dessin de l'image
   d'avant, pour ne jamais réclamer un pixel que la vue web n'avait pas
   encore peint : ces pixels-là étaient blancs. Ils ne le sont plus, la
   fenêtre étant vraiment transparente, et ce qui restait était le coût de
   cette prudence : au survol, le logo grandit de six pour cent depuis son
   coin haut droit, donc de plusieurs pixels vers la gauche et vers le bas,
   et la découpe restait une image en arrière tout du long. Elle coupait
   donc en plein milieu du dessin, ce qui enlevait le contour noir de ces
   deux bords-là et laissait à la place l'escalier du pochoir. */
function formeOccupee(echelle) {
  const morceaux = [];
  const droite = document.documentElement.clientWidth;
  // Le bord horizontal ne bouge jamais, le vertical dépend du sens : vers
  // le haut, c'est le sommet de la fenêtre qui se déplace quand elle
  // grandit, donc c'est depuis son bas qu'il faut compter.
  const bas = versLeHaut ? document.documentElement.clientHeight : 0;
  // Chaque bord arrondi vers l'extérieur, et les quatre de la même façon.
  //
  // C'est ce qui rend la bordure régulière. La découpe est un pochoir à un
  // bit par pixel : ce qu'elle laisse dehors disparaît. Arrondie vers
  // l'intérieur, elle rognait le bord lissé du dessin d'une fraction de
  // pixel différente sur chaque côté, et comme un pixel ne se coupe pas en
  // deux, ça faisait un contour épais d'un pixel ici et de deux là. C'est
  // le « pas du tout homogène, plus épais à gauche » rapporté sur fond
  // blanc.
  //
  // Arrondie vers le dehors, elle contient tout ce que la page a dessiné,
  // sur les quatre bords : le contour qu'on voit est celui que la page a
  // peint, avec son lissage, et il est le même partout.
  //
  // Ce que ça réclame en plus est une frange d'un pixel que personne n'a
  // peinte, et elle ne coûte plus rien depuis que la fenêtre est vraiment
  // transparente : un pixel non peint n'y est plus blanc, il n'y est rien.
  // C'était l'inverse avant, et c'est pour ça que ce calcul arrondissait
  // dans l'autre sens : le liseré clair sur la gauche du bouton, surtout
  // en le déplaçant, était cette frange remplie au pinceau du système.
  //
  // Les bords sont arrondis, pas l'origine et la taille chacune de leur
  // côté : arrondir l'origine vers le dehors et la taille vers le haut
  // décale le bord opposé de la somme des deux, et c'est reparti pour une
  // bordure inégale, dans l'autre sens.
  //
  // Et un pixel de plus par-dessus l'arrondi, qui n'est pas du luxe : sans
  // lui, il reste zéro dans le cas où un bord tombe pile sur un pixel, et
  // ce zéro-là se paye dans les coins. Un coin arrondi n'est pas un bord :
  // au plus loin de l'angle, un rayon r ne dépasse de la boîte que de
  // 0,29 r, et la découpe et le dessin n'ont pas exactement le même rayon
  // puisque celui de la découpe est arrondi au pixel. La marge y tombait à
  // un dixième de pixel, donc sous le lissage du dessin, donc le pochoir
  // coupait dedans : c'est le tour pixelisé qui restait au repos.
  //
  // Ce pixel ne se voit pas : la fenêtre est transparente, donc il n'y est
  // rien. Il élargit d'autant ce qui attrape les clics, ce qui à cette
  // taille-là ne se sent pas non plus.
  const pose = (gauche, haut, large, haute, rayon) => {
    // Le dessin en vrais pixels, sans arrondi : c'est ce que la page
    // peint, et c'est à ça que le journal compare la découpe.
    const dessin = [
      (gauche - droite) * echelle,
      (haut - bas) * echelle,
      (gauche + large - droite) * echelle,
      (haut + haute - bas) * echelle,
    ];
    const x = Math.floor(dessin[0]) - MARGE;
    const y = Math.floor(dessin[1]) - MARGE;
    morceaux.push({
      x,
      y,
      width: Math.ceil(dessin[2]) + MARGE - x,
      height: Math.ceil(dessin[3]) + MARGE - y,
      // Arrondi vers le bas et non au plus proche : un coin moins rond
      // que celui du dessin déborde de sa boîte, un coin plus rond y
      // rentre. Au plus proche, une fois sur deux il rentrait, et il
      // mangeait dans le coin la marge que le pixel ci-dessus venait de
      // donner. Vers le bas, la marge du coin ne peut plus être plus
      // petite que celle des bords.
      radius: Math.floor(rayon * echelle),
      drawn: dessin,
      drawnRadius: rayon * echelle,
    });
  };
  // Une carte, rognée à ce que la page peut avoir dessiné.
  //
  // Rognée, et c'est tout le sujet de l'éclair vu en ouvrant la liste
  // pour la première fois. La fenêtre est accrochée par son coin haut
  // droit et grandit vers la gauche : tant qu'elle ne l'a pas fait, une
  // carte qui s'ouvre de ce côté est posée en dehors de la page, et une
  // vue web ne peint pas ce qui est dehors. Découpée quand même, la
  // fenêtre montrait son propre fond à cette place, le temps d'une
  // image, avant de grandir. Et une seule fois, parce que cette
  // fenêtre-là ne rétrécit jamais : la fois d'après elle est déjà assez
  // large et il n'y a plus rien à découvrir.
  //
  // Ce qui est rogné revient au tour suivant : le suivi ne s'arrête que
  // sur deux images identiques, et la fenêtre aura grandi entre-temps.
  const dessous = document.documentElement.clientHeight;
  const carte = (element) => {
    const ou = element.getBoundingClientRect();
    const gauche = Math.max(ou.left, 0);
    const haut = Math.max(ou.top, 0);
    const large = Math.min(ou.right, droite) - gauche;
    const haute = Math.min(ou.bottom, dessous) - haut;
    if (large <= 0 || haute <= 0) {
      return;
    }
    const rayon = parseFloat(getComputedStyle(element).borderTopLeftRadius);
    pose(gauche, haut, large, haute, rayon || 0);
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
    // La carte du sous-menu, sans quoi la page la dessine et la fenêtre
    // la découpe aussitôt : la liste s'ouvrait bel et bien, elle était
    // simplement taillée hors de la fenêtre. De dehors, le menu
    // disparaissait et rien n'arrivait, ce qui ressemble trait pour
    // trait à un menu qui se ferme tout seul.
    const panneau = lePanneau();
    if (panneau) {
      carte(panneau);
    }
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
    // De quel côté ce dessin-là a été mesuré. Sans ce mot, le coeur
    // découpe et pose la fenêtre selon le sens qu'il voudrait plutôt que
    // selon celui qui est à l'écran, et les deux diffèrent le temps que
    // la page entende la réponse : le logo se retrouvait alors à une
    // hauteur de menu de la main qui le tenait, et la découpe laissait
    // un trou que le système remplissait de son propre fond.
    upward: versLeHaut,
  })
    // Le coeur répond de quel côté le menu doit s'ouvrir. Un changement
    // remet la page en page, donc tout ce qui vient d'être mesuré est
    // à refaire : le suivi ci-dessous s'en charge, puisqu'il ne s'arrête
    // que quand deux images de suite dessinent la même chose.
    .then(poseLeSens)
    .catch(() => {});
  // Ce qui vient d'être envoyé, en un mot : c'est à ça que l'appelant
  // voit si le dessin bouge encore.
  return JSON.stringify([versLeHaut, boite.width, boite.height, forme]);
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

/* Le sous-menu ouvert, ou rien.

   Il s'ouvre à côté du menu et non à sa place. À sa place, ouvrir la
   liste effaçait le menu, ce qui ne se lit pas comme une liste qui
   s'ouvre mais comme un menu qui se ferme : on croyait avoir raté le
   clic. Côte à côte, on voit d'où l'on vient, on choisit, et le menu est
   toujours là. */
let panneauOuvert = null;

function lePanneau() {
  return panneauOuvert === null
    ? null
    : document.getElementById(`panneau-${panneauOuvert}`);
}

/* Air laissé entre la carte et le bord de l'écran, et plus petite liste
   qui vaille encore la peine d'être ouverte. */
const AIR = 32;
const MOINS_QUE_CA = 200;

/* Borne la liste à ce que l'écran peut porter.

   L'écran et non la fenêtre : cette fenêtre-ci fait exactement la taille
   de ce que la page dessine, donc la borner à une fraction d'elle-même
   la bornait à une fraction de ce qu'elle allait devenir. La liste
   défilait avec la moitié de la carte vide en dessous, sur un écran où
   il y avait toute la place du monde.

   Ce que la carte occupe autour de la liste est mesuré et non deviné :
   un titre, un trait et des marges, et rien ne dit d'avance combien ça
   fait sur un écran agrandi. */
function borneLaListe(panneau) {
  const liste = panneau.querySelector(".sous-liste");
  const ecran = window.screen?.availHeight ?? 0;
  if (!liste || ecran <= 0) {
    return;
  }
  liste.style.maxHeight = "none";
  const autour =
    panneau.getBoundingClientRect().height -
    liste.getBoundingClientRect().height;
  liste.style.maxHeight = `${Math.max(ecran - autour - AIR, MOINS_QUE_CA)}px`;
}

function montrePanneau(nom) {
  panneauOuvert = nom;
  for (const panneau of document.querySelectorAll(".panneau")) {
    const ici = panneau.id === `panneau-${nom}`;
    // Caché et non rangé : il garde sa place, donc la fenêtre a sa
    // taille définitive dès l'ouverture du menu et n'a plus jamais à
    // grandir. Une fenêtre qui grandit découvre une bande que la page
    // n'a pas encore peinte, et c'est de là que vient le clignotement.
    panneau.classList.toggle("repliee", !ici);
    if (ici) {
      borneLaListe(panneau);
    }
  }
  for (const item of document.querySelectorAll("[data-ouvre-panneau]")) {
    item.setAttribute(
      "aria-expanded",
      item.dataset.ouvrePanneau === nom ? "true" : "false",
    );
  }
  suisLeDessin();
}

function ouvre(veut) {
  ouvert = veut;
  // Un menu qu'on rouvre s'ouvre sur lui-même. Rester dans une liste
  // choisie il y a deux sessions serait un menu qui a l'air d'un autre.
  if (!veut || panneauOuvert !== null) {
    montrePanneau(null);
  }
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
    // Les trois ont pu bouger pendant que le menu était fermé : le
    // raccourci du produit bascule la souris, et le mélangeur de Windows
    // est ouvert à tout le monde. Les interrupteurs doivent dire où l'on
    // en est, pas où l'on en était la dernière fois qu'on a regardé.
    litLaSouris();
    litLeSon();
    litLeClavier();
    // Et le reste avec, pour la même raison exactement. Résolution,
    // débit, codec et cadence d'en face sont lus une fois au chargement
    // de la page, et la page vit toute la session : appliquer un
    // changement rouvre l'image avec, mais le menu continuait de
    // montrer ce qu'il avait lu au début, donc « Appliquer les
    // changements » restait affiché pour toujours, sur des réglages
    // déjà appliqués.
    litLeMenu();
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
    // Une liste à elle, ouverte par un chevron. Les deux premières
    // entrées ne sont pas des tailles : elles disent lequel des deux
    // ordinateurs décide, ce qu'aucune barre ne sait dire, et les quinze
    // qui suivent feraient des crans qu'on ne vise plus.
    liste: true,
    // Dans l'ordre du produit, qui est celui du menu : les deux façons de
    // décider d'abord, les nombres ensuite du plus grand au plus petit.
    valeurs: (menu) => menu.sizes.map((taille) => taille.value),
    dit: (menu, valeur) => {
      if (valeur === "client") {
        return "Résolution du client";
      }
      if (valeur === "host") {
        return "Résolution de l'hôte";
      }
      const taille = menu.sizes.find((une) => une.value === valeur);
      return taille ? `${taille.width}x${taille.height}` : valeur;
    },
    // Ce que la ligne du menu affiche à droite du mot : ce à quoi le
    // choix revient réellement ici, puisque « client » ne dit pas si on
    // demande du 4K ou du 1080p et que c'est justement ce qu'on veut
    // savoir avant d'ouvrir la session.
    resume: (menu, valeur) => {
      if (valeur === "host") {
        return "hôte";
      }
      const taille = menu.sizes.find((une) => une.value === valeur);
      const nombres = taille ? `${taille.width}x${taille.height}` : valeur;
      return valeur === "client" ? `client, ${nombres}` : nombres;
    },
    // Le rapport de la taille, dit comme les écrans se vendent. Une
    // colonne à droite dans la liste : deux nombres se comparent mal, et
    // 21:9 à côté de 16:9 dit tout de suite ce qui va être coupé.
    aparte: (menu, valeur) => {
      // Rien pour les deux premières : ce à quoi elles reviennent dépend
      // de l'écran qu'on a en face, et un rapport écrit là serait celui
      // de cet ordinateur-ci donné pour celui d'un autre.
      if (valeur === "client" || valeur === "host") {
        return "";
      }
      const taille = menu.sizes.find((une) => une.value === valeur);
      return taille && taille.width > 0
        ? rapport(taille.width, taille.height)
        : "";
    },
    ou: (choix) => choix.asked,
  },
  screen: {
    // Une liste à elle, comme la résolution, et pour la même raison : ce
    // sont des noms d'écrans, sans ordre entre eux, et une machine peut
    // en avoir plus de deux. Ils viennent de la machine d'en face et
    // d'elle seule : c'est elle qui nomme ses écrans, et rien d'ici ne
    // sait ce qui y est branché.
    liste: true,
    valeurs: (menu) => menu.screens.map((ecran) => ecran.id),
    dit: (menu, valeur) => {
      const ecran = menu.screens.find((un) => un.id === valeur);
      if (!ecran) {
        return valeur;
      }
      return ecran.main ? `${ecran.name} (principal)` : ecran.name;
    },
    // Le nom seul sur la ligne du menu : « (principal) » y prendrait la
    // place du nom sans rien apprendre, la liste le disant déjà.
    resume: (menu, valeur) => {
      const ecran = menu.screens.find((un) => un.id === valeur);
      return ecran ? ecran.name : "";
    },
    // Sa taille en colonne à droite, comme le rapport l'est pour la
    // résolution : deux écrans se distinguent d'abord par là, et un nom
    // de modèle ne dit rien à qui ne l'a pas acheté.
    aparte: (menu, valeur) => {
      const ecran = menu.screens.find((un) => un.id === valeur);
      return ecran ? `${ecran.wide}x${ecran.high}` : "";
    },
    ou: (choix) => choix.screen,
  },
  bitrate: {
    valeurs: (menu) => menu.rates.map(String),
    dit: (_menu, valeur) => `${Math.round(Number(valeur) / 1000)} Mb/s`,
    ou: (choix) => String(choix.bitrateKbps),
  },
  codec: {
    // Des boutons et non une barre. Le codec n'est pas une échelle : ce
    // sont quelques noms sans ordre entre eux, dont un « Automatique »
    // qui n'est pas une valeur mais un renoncement, et pousser un
    // curseur promettait un plus et un moins qui n'existent pas.
    boutons: true,
    valeurs: (menu) => menu.codecs,
    dit: (_menu, valeur) => (valeur === "auto" ? "Automatique" : valeur),
    ou: (choix) => choix.codec,
  },
  steady: {
    // Deux boutons pour la même raison que le codec : deux mots, pas une
    // échelle. Et ici, contrairement aux trois interrupteurs du haut,
    // c'est un réglage du moteur d'en face, qui ne le lit qu'à son
    // démarrage : il se range avec la taille, le débit et le codec, et
    // part avec eux quand on applique.
    boutons: true,
    valeurs: () => ["off", "on"],
    dit: (_menu, valeur) => (valeur === "on" ? "Fluide" : "Économe"),
    ou: (choix) => (choix.steady ? "on" : "off"),
  },
};

/* Le rapport d'une taille, réduit comme on le lit sur une fiche d'écran.
   Calculé plutôt qu'écrit à côté de chaque nombre : une deuxième table
   s'écarterait de la première le jour où une taille s'ajoute. Les deux
   rapports que personne n'écrit sous leur forme réduite sont dits comme
   tout le monde les dit. */
function rapport(large, haut) {
  const pgcd = (a, b) => (b === 0 ? a : pgcd(b, a % b));
  const par = pgcd(large, haut) || 1;
  const [x, y] = [large / par, haut / par];
  if (x === 8 && y === 5) {
    return "16:10";
  }
  if (x === 683 && y === 384) {
    return "16:9";
  }
  return `${x}:${y}`;
}

/* Ce que le produit propose, demandé une fois : les crans ne changent pas
   d'un clic à l'autre, et les refaire à chaque fois ferait clignoter la
   fenêtre à chaque ouverture. */
let leMenu = null;

/* Ce que la machine d'en face a dit ne pas savoir encoder.

   Rien du tout veut dire qu'elle n'a rien dit, jamais qu'elle ne sait
   rien faire : hors session, ou pendant que son moteur démarre, la
   question n'a pas de réponse, et une question sans réponse doit laisser
   le menu exactement comme il était. */
function horsDePortee(nom, valeur) {
  return nom === "codec" && (leMenu?.beyondIt ?? []).includes(valeur);
}

function bloc(nom) {
  return document.querySelector(`[data-reglage="${nom}"]`);
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
    const ou = ligne.ou(choix);
    // Cherchée là où elle vit, comme au-dessus.
    if (ligne.liste) {
      // La ligne du menu dit ce qui est choisi, la liste marque la
      // même valeur : les deux lisent le même endroit, donc elles ne
      // peuvent pas se contredire.
      const dit = document.querySelector(`[data-valeur="${nom}"]`);
      if (dit) {
        dit.textContent = (ligne.resume ?? ligne.dit)(leMenu, ou);
      }
      for (const entree of document.querySelectorAll(
        `[data-liste="${nom}"] [data-choix]`,
      )) {
        entree.setAttribute(
          "aria-checked",
          entree.dataset.choix === ou ? "true" : "false",
        );
      }
      continue;
    }
    const ici = bloc(nom);
    if (!ici) {
      continue;
    }
    if (ligne.boutons) {
      for (const bouton of ici.querySelectorAll("[data-choix]")) {
        bouton.setAttribute(
          "aria-checked",
          bouton.dataset.choix === ou ? "true" : "false",
        );
      }
      continue;
    }
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
  // Ce qui vient d'être choisi n'est pas forcément encore écrit : un
  // curseur lâché part au service et met un aller-retour à y arriver,
  // et la ligne « Appliquer » est déjà à l'écran depuis le choix
  // d'avant. Relancer sans attendre relisait les réglages tels
  // qu'ils étaient, et l'image revenait identique.
  await choixEnCours;
  // Refermé avant de partir, et non après. L'image s'en va et revient,
  // ce qui prend des secondes et met un écran de chargement à sa place ;
  // le menu attendait la fin pour se replier, donc il restait posé
  // dessus tout du long, avec la fenêtre découpée à sa taille à lui.
  // C'était une nappe de menu par-dessus le chargement, et elle ne
  // partait qu'une fois l'image revenue.
  ouvre(false);
  try {
    await invoke("apply_session");
  } catch (raison) {
    // Rouvert pour porter le refus : la ligne qui le dit vit dans le
    // menu, et un menu fermé la dirait à personne.
    ouvre(true);
    souci(String(raison));
  }
}

function batisLesChoix() {
  for (const [nom, ligne] of Object.entries(LIGNES)) {
    const valeurs = ligne.valeurs(leMenu);
    // Une ligne qui ouvre un panneau est cherchée là où un panneau vit,
    // et jamais parmi les réglages. Elle n'en est pas un : sur le menu
    // elle est un bouton avec un chevron, et son contenu est ailleurs,
    // dans la page du panneau. Cherchée parmi les réglages, elle n'était
    // trouvée nulle part, la liste ne se remplissait jamais, et ouvrir
    // le sous-menu remplaçait le menu par une page vide.
    if (ligne.liste) {
      const liste = document.querySelector(`[data-liste="${nom}"]`);
      if (!liste) {
        continue;
      }
      const dedans = [];
      for (const valeur of valeurs) {
        const entree = document.createElement("button");
        entree.type = "button";
        entree.className = "item";
        entree.dataset.choix = valeur;
        entree.setAttribute("role", "menuitemradio");
        entree.setAttribute("aria-checked", "false");

        const coche = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "svg",
        );
        coche.setAttribute("viewBox", "0 0 24 24");
        coche.setAttribute("aria-hidden", "true");
        coche.setAttribute("class", "coche");
        const trait = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "path",
        );
        trait.setAttribute("d", "M4 12.5l5.5 5.5L20 6");
        coche.append(trait);

        const mot = document.createElement("span");
        mot.className = "item-mot";
        mot.textContent = ligne.dit(leMenu, valeur);

        const cote = document.createElement("span");
        cote.className = "item-touche";
        cote.textContent = ligne.aparte ? ligne.aparte(leMenu, valeur) : "";

        entree.append(coche, mot, cote);
        entree.addEventListener("click", () => {
          choisis(nom, valeur);
          montrePanneau(null);
        });
        dedans.push(entree);
      }
      liste.replaceChildren(...dedans);
      // Une liste vide est une ligne qui ne peut mener nulle part : la
      // machine d'en face n'a qu'un écran, ou son moteur n'a pas encore
      // dit lesquels. La ligne du menu s'efface avec elle plutôt que
      // d'ouvrir un panneau vide.
      const laLigne = document.querySelector(`[data-ouvre-panneau="${nom}"]`);
      if (laLigne) {
        montre(laLigne, valeurs.length > 0);
      }
      // Rebâtie, elle n'a plus la même hauteur : ce qui la borne est à
      // remesurer, et seulement quand elle est là pour être mesurée.
      const panneau = lePanneau();
      if (panneau?.contains(liste)) {
        borneLaListe(panneau);
      }
      continue;
    }
    const ici = bloc(nom);
    if (!ici) {
      continue;
    }
    if (ligne.boutons) {
      ici.querySelector(".bascule").replaceChildren(
        ...valeurs.map((valeur) => {
          const bouton = document.createElement("button");
          bouton.type = "button";
          bouton.className = "bascule-cote";
          bouton.dataset.choix = valeur;
          bouton.setAttribute("role", "menuitemradio");
          bouton.setAttribute("aria-checked", "false");
          bouton.textContent = ligne.dit(leMenu, valeur);
          // Ce que la machine d'en face ne sait pas faire ne se clique
          // pas. Elle est la seule à le savoir : c'est elle qui encode,
          // et un codec qu'elle ne peut pas produire n'échoue nulle
          // part, les deux moteurs s'entendent sur un autre en silence.
          // Le menu continuait alors d'afficher un choix qui n'était plus
          // honoré depuis le début de la session.
          if (horsDePortee(nom, valeur)) {
            bouton.disabled = true;
            bouton.title = "Cet ordinateur ne sait pas encoder ce format";
          }
          bouton.addEventListener("click", () => choisis(nom, valeur));
          return bouton;
        }),
      );
      continue;
    }
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

/* ---- Les interrupteurs du menu ----------------------------------------- */

/* Deux mots côte à côte, celui qui est en place allumé. La ligne d'avant
   disait « souris bureau ou jeu » et basculait à l'aveugle : elle
   annonçait ce que le clic ferait, jamais où l'on en était, et les deux
   modes ne se distinguent pas à l'oeil sur un bureau immobile.

   Souris, son et clavier marchent pareil, donc se construisent pareil.
   Le côté de droite est celui qui vaut « oui » : jeu pour la souris,
   coupé pour le son, immersif pour le clavier. L'état se
   relit à chaque ouverture du menu plutôt que retenu ici, parce qu'il
   peut changer sans passer par cette page. Et cliquer le côté où l'on
   est déjà ne fait rien, comme tout interrupteur qu'on pousse du côté où
   il est déjà. */
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

const litLeClavier = interrupteur(
  vue.clavier,
  "clavier",
  () => invoke("floating_keys"),
  () => invoke("floating_act", { what: "keys" }),
);

async function litLeMenu() {
  try {
    leMenu = await invoke("session_menu");
  } catch {
    /* Le service ne répond pas : la session qui s'ouvre le dira bien
       mieux que trois lignes de menu. */
    return;
  }
  batisLesChoix();
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

/* Et les animations elles-mêmes, ce qui n'est pas la même chose que la
   souris qui arrive.

   Le suivi s'arrête dès que deux images de suite dessinent la même forme,
   ce qui est juste au repos et faux au démarrage d'une animation : entre
   le moment où la souris entre et celui où le navigateur crée vraiment la
   transition, il passe une image ou deux pendant lesquelles rien n'a
   encore bougé. Le suivi s'y arrêtait, et la découpe restait celle du
   repos pendant tout le grandissement : le logo grandissait hors de sa
   propre découpe et perdait son contour à gauche et en bas. Ça ne se
   voyait qu'au premier survol, les suivants trouvant le style déjà calculé
   et la transition démarrée à l'image d'après.

   « transitionrun » est envoyé au moment où la transition est créée, donc
   exactement là où le suivi doit repartir, et « transitionend » à la fin,
   pour poser la découpe sur la forme définitive. */
for (const quand of [
  "transitionrun",
  "transitionstart",
  "transitionend",
  "transitioncancel",
]) {
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

for (const item of document.querySelectorAll("[data-ouvre-panneau]")) {
  // La même ligne ouvre et referme : une liste ouverte à côté du menu se
  // referme là où on l'a ouverte, et pas seulement par son titre.
  item.addEventListener("click", () =>
    montrePanneau(
      panneauOuvert === item.dataset.ouvrePanneau
        ? null
        : item.dataset.ouvrePanneau,
    ),
  );
}

for (const item of document.querySelectorAll("[data-ferme-panneau]")) {
  item.addEventListener("click", () => montrePanneau(null));
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
