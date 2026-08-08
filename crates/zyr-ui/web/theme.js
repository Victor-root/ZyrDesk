/*
  Le thème, décidé avant le premier affichage.

  Trois choix possibles : suivre le système, forcer le clair, forcer le
  sombre. Suivre le système est le défaut, et il suit pour de bon : si
  Windows bascule pendant que la fenêtre est ouverte, elle bascule avec.

  Ce fichier est chargé dans l'en-tête, sans attendre : il pose le thème
  avant que quoi que ce soit ne soit dessiné. Chargé plus tard, la
  fenêtre s'ouvrirait dans le mauvais thème le temps d'un battement.
*/

const CLE = "zyrdesk.theme";
const CHOIX = ["systeme", "clair", "sombre"];

const systemeEstClair = window.matchMedia("(prefers-color-scheme: light)");

function choisi() {
  const garde = localStorage.getItem(CLE);
  return CHOIX.includes(garde) ? garde : "systeme";
}

/* Ce que le choix donne concrètement, une fois le système consulté. */
function resolu(choix) {
  if (choix === "systeme") {
    return systemeEstClair.matches ? "clair" : "sombre";
  }
  return choix;
}

function applique() {
  const theme = resolu(choisi());
  document.documentElement.dataset.theme = theme;
  // Les décorations de la fenêtre appartiennent au système, pas à la
  // page : sans ce mot au coeur Rust, une application claire garderait
  // une barre de titre sombre.
  window.dispatchEvent(new CustomEvent("theme-pose", { detail: theme }));
}

function poser(choix) {
  localStorage.setItem(CLE, CHOIX.includes(choix) ? choix : "systeme");
  applique();
}

systemeEstClair.addEventListener("change", applique);
applique();

window.theme = { choisi, poser, CHOIX };
