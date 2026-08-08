/*
  Le bouton flottant. Il ne sait rien de la session : il demande, le
  coeur Rust traduit en langage du moteur, et l'image obéit.

  La fenêtre est retaillée à chaque changement d'état, parce qu'elle
  n'est rien d'autre que ce qui se voit : plus grande, elle mangerait
  des clics destinés à l'image.
*/

const invoke = window.__TAURI__.core.invoke;

const vue = {
  paquet: document.getElementById("paquet"),
  logo: document.getElementById("logo"),
  menu: document.getElementById("menu"),
  souci: document.getElementById("souci"),
};

/* Le temps qu'un refus reste lisible avant de laisser la place. */
const TEMPS_SOUCI = 4000;

let ouvert = false;
let effacement = null;

function montre(element, visible) {
  element.classList.toggle("cache", !visible);
}

/* La fenêtre suit ce que la page occupe, mesuré et non deviné : le menu
   n'a pas la même hauteur selon ce qu'il contient. */
function ajusteLaFenetre() {
  const boite = vue.paquet.getBoundingClientRect();
  invoke("floating_size", {
    width: Math.ceil(boite.width),
    height: Math.ceil(boite.height),
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

/* ---- Déplacer le bouton ------------------------------------------------ */

/* Le bouton se prend et se pose où on veut sur l'image. Un clic net
   ouvre le menu ; dès que la souris s'écarte un peu, c'est un
   déplacement et plus un clic. Sans ce seuil, une main qui tremble
   ouvrirait le menu à chaque fois qu'on veut bouger le bouton, ou
   l'inverse. */
const SEUIL = 4;

let prise = null;
let deplace = false;

vue.logo.addEventListener("pointerdown", (evenement) => {
  prise = { x: evenement.screenX, y: evenement.screenY };
  deplace = false;
  vue.logo.setPointerCapture(evenement.pointerId);
});

vue.logo.addEventListener("pointermove", (evenement) => {
  if (prise === null) {
    return;
  }
  const dx = evenement.screenX - prise.x;
  const dy = evenement.screenY - prise.y;
  if (!deplace && Math.abs(dx) < SEUIL && Math.abs(dy) < SEUIL) {
    return;
  }
  if (!deplace) {
    deplace = true;
    // Une fenêtre qui change de taille pendant qu'on la déplace se
    // dérobe sous la souris : le menu se referme d'abord.
    ouvre(false);
  }
  prise = { x: evenement.screenX, y: evenement.screenY };
  invoke("floating_move", { dx, dy }).catch(() => {});
});

vue.logo.addEventListener("pointerup", (evenement) => {
  vue.logo.releasePointerCapture(evenement.pointerId);
  if (prise !== null && !deplace) {
    ouvre(!ouvert);
  }
  prise = null;
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

ajusteLaFenetre();
