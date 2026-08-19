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
};

/* Le temps qu'un refus reste lisible avant de laisser la place. */
const TEMPS_SOUCI = 4000;

let ouvert = false;
let effacement = null;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
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
  }).catch(() => {});
}

function ouvre(veut) {
  ouvert = veut;
  montre(vue.menu, veut);
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

/* ---- Prendre et déplacer le bouton ------------------------------------- */

/* Le bouton se prend et se pose où on veut sur l'image. Tout le geste
   est suivi côté Rust, et non ici : cette fenêtre fait cinquante
   pixels, la souris en sort au premier mouvement, et ce qu'une vue web
   rapporte de la position d'un pointeur n'est pas toujours l'endroit où
   ce pointeur se trouve à l'écran. La page dit seulement quand le
   bouton est pris, et apprend à la fin si c'était un déplacement ou un
   simple clic. */
let pris = false;

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
}

ditLesRaccourcis();

ajusteLaFenetre();
