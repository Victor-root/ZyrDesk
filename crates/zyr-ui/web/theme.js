/*
  Le thème, décidé avant le premier affichage.

  Trois choix possibles : suivre le système, forcer le clair, forcer le
  sombre. Suivre le système est le défaut, et il suit pour de bon : si
  Windows bascule pendant que la fenêtre est ouverte, elle bascule avec.

  Ce fichier est chargé dans l'en-tête, sans attendre : il pose le thème
  avant que quoi que ce soit ne soit dessiné. Chargé plus tard, la
  fenêtre s'ouvrirait dans le mauvais thème le temps d'un battement.

  Ce que veut Windows n'est pas demandé à la page. Une page demande
  d'ordinaire « prefers-color-scheme » à son navigateur, mais le
  navigateur est ici une vue web posée dans notre fenêtre, et la boîte à
  outils fige la réponse de cette vue au moment où la fenêtre est
  construite. Elle est juste à la première image et gelée ensuite :
  Windows peut basculer, la page ne verra rien et l'événement qu'elle
  guetterait ne se déclenchera jamais. C'est le coeur Rust qui écoute
  Windows et qui le dit ici.

  La question figée reste bonne pour une chose, et une seule : la toute
  première image, avant que le coeur ait pu répondre. Elle était juste au
  moment où la fenêtre a été bâtie, il y a quelques millisecondes.
*/

const CLE = "zyrdesk.theme";
const CHOIX = ["systeme", "clair", "sombre"];

let systemeEstClair = window.matchMedia(
  "(prefers-color-scheme: light)",
).matches;

function choisi() {
  const garde = localStorage.getItem(CLE);
  return CHOIX.includes(garde) ? garde : "systeme";
}

/* Ce que le choix donne concrètement, une fois le système consulté. */
function resolu(choix) {
  if (choix === "systeme") {
    return systemeEstClair ? "clair" : "sombre";
  }
  return choix;
}

function applique() {
  const choix = choisi();
  document.documentElement.dataset.theme = resolu(choix);
  // Les décorations de la fenêtre appartiennent au système, pas à la
  // page : sans ce mot au coeur Rust, une application claire garderait
  // une barre de titre sombre. C'est le choix qui part et non la couleur
  // qu'il donne : dire « clair » à une fenêtre qui ne fait que suivre la
  // fige, et fait cesser jusqu'aux avis de bascule.
  window.dispatchEvent(new CustomEvent("theme-pose", { detail: choix }));
}

function poser(choix) {
  localStorage.setItem(CLE, CHOIX.includes(choix) ? choix : "systeme");
  applique();
}

/* Ce que Windows veut, dit par le coeur : demandé une fois au chargement
   et redit à chaque bascule. Rien n'est réappliqué quand la réponse ne
   change pas, sans quoi la fenêtre se verrait redemander son thème pour
   rien à chaque ouverture. */
function veutDuClair(clair) {
  if (systemeEstClair === clair) {
    return;
  }
  systemeEstClair = clair;
  applique();
}

const tauri = window.__TAURI__;
if (tauri) {
  tauri.core
    .invoke("system_theme")
    .then(veutDuClair)
    .catch(() => {});
  tauri.event.listen("system-theme", ({ payload }) => veutDuClair(payload));
}

applique();

window.theme = { choisi, poser, CHOIX };
