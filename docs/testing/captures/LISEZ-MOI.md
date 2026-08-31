# Captures gardées

Les défauts d'affichage qui se cherchent au pixel et qui coûtent une manipulation à refaire. Une capture rangée ici est une pièce à conviction : on la relit, on la mesure, on ne redemande pas à Victor de la reprendre.

## `bouton-flottant-croix.png`

Le bouton flottant pendant une session, sur PC-SAV, 91 x 98 pixels réels. C'est « la fameuse croix par-dessus le FAB », vue et revue depuis juillet, et elle est particulièrement pénible à attraper : elle disparaît dès qu'on approche la souris du bouton, et le voile de l'outil de capture de Windows la fait disparaître aussi. Elle se produit à l'identique sur les trois machines.

**Ce que les pixels disent.** L'or du logo vaut `239,181,54` dans le dessin. Là où le fond derrière est noir, la capture donne `131,99,30`, soit 131 / 239 = **0,55**, et le contour donne `5,7,12` pour `9,13,22`, soit 0,55 encore. C'était l'opacité du bouton au repos.

**Et ce n'est pas l'écran d'en face qu'on voit au travers**, ce qui a été cru d'abord et à tort. La preuve tient en une phrase : la chose claire **s'arrête pile au bord du logo**, et le noir tout autour d'elle est pur. Ce qui serait derrière la fenêtre se verrait aussi à côté du bouton. Donc elle est **dans la fenêtre du bouton, sous la page**.

En retirant le logo par le calcul, pixel par pixel, ce qui reste est sans ambiguïté : **une barre de titre claire avec un bouton de fermeture**, carré blanc et croix noire, et la zone client sombre en dessous. C'est le cadre que le système peint dans une fenêtre à sa naissance. Tant que cette fenêtre était opaque, la boîte à outils repeignait son fond par-dessus à chaque effacement et personne ne l'a jamais vu ; devenue vraiment transparente, elle ne repeint plus rien, et un logo à 55 % ne le couvre pas.

D'où les deux choses que Victor rapportait sans qu'elles s'expliquent : ça part quand la souris arrive, et la capture d'écran ne la prend pas. Le survol rendait le logo entier, donc opaque, donc il la couvrait.

Voir [D101](../../DECISIONS.md) pour la décision, et [R66](../M4-PROTOCOLE.md) pour l'essai.
