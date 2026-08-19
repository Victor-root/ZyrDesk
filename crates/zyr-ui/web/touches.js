/*
  Écrire une combinaison de touches telle qu'elle est gravée sur le
  clavier de la personne.

  Le programme retient la place d'une touche et non le signe dessus : la
  touche à gauche des chiffres porte « ² » en France et « ` » ailleurs,
  et c'est la même touche sous le même doigt. Pour l'afficher il faut
  refaire le chemin en sens inverse, et le navigateur est le seul à
  savoir quel clavier est branché.

  Partagé par l'accueil, qui les fait choisir, et par le bouton flottant,
  qui dit lequel le ramène.
*/

const MOT_DES_TENUES = { Ctrl: "Ctrl", Alt: "Alt", Shift: "Maj", Win: "Win" };

let planDuClavier = null;

async function litLePlanDuClavier() {
  if (planDuClavier === null && navigator.keyboard?.getLayoutMap) {
    planDuClavier = await navigator.keyboard.getLayoutMap().catch(() => null);
  }
  return planDuClavier;
}

/* « Alt+Backquote » devient « Alt + ² ». Faute de plan, la place reste
   affichée telle quelle : illisible mais jamais fausse. */
function ecritLaCombinaison(texte) {
  const morceaux = texte.split("+");
  const place = morceaux.pop();
  const grave = planDuClavier?.get(place);
  const tenues = morceaux.map((tenue) => MOT_DES_TENUES[tenue] ?? tenue);
  return [...tenues, grave ? grave.toUpperCase() : place].join(" + ");
}
