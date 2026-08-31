# Captures gardées

Les défauts d'affichage qui se cherchent au pixel et qui coûtent une manipulation à refaire. Une capture rangée ici est une pièce à conviction : on la relit, on la mesure, on ne redemande pas à Victor de la reprendre.

## `bouton-flottant-croix.png`

Le bouton flottant pendant une session, sur PC-SAV, 91 x 98 pixels réels. C'est « la fameuse croix par-dessus le FAB », vue et revue depuis juillet, et elle est particulièrement pénible à attraper : elle disparaît dès qu'on approche la souris du bouton, et le voile de l'outil de capture de Windows la fait disparaître aussi. Elle se produit à l'identique sur les trois machines.

**Ce que les pixels disent.** L'or du logo vaut `239,181,54` dans le dessin. Là où le fond derrière est noir, la capture donne `131,99,30`, soit 131 / 239 = **0,55**, et le contour donne `5,7,12` pour `9,13,22`, soit 0,55 encore. C'était l'opacité du bouton au repos.

**Et ce n'est pas l'écran d'en face qu'on voit au travers**, ce qui a été cru d'abord et à tort. La preuve tient en une phrase : la chose claire **s'arrête pile au bord du logo**, et le noir tout autour d'elle est pur. Ce qui serait derrière la fenêtre se verrait aussi à côté du bouton. Donc elle est **dans la fenêtre du bouton, sous la page**.

En retirant le logo par le calcul, pixel par pixel, ce qui reste est sans ambiguïté : **une barre de titre claire avec un bouton de fermeture**, carré blanc et croix noire, et la zone client sombre en dessous. Ce n'est pas le cadre de cette fenêtre-ci, piste essayée et démentie par le journal lui-même : c'est **de la mémoire tampon que personne n'a peinte**, et ce qu'elle contenait était l'image d'une autre fenêtre.

Sans couleur de fond, la boîte à outils n'efface rien, et ce qui n'est jamais effacé garde ce qui s'y trouvait avant. Tant que cette fenêtre était opaque le fond était repeint par-dessus et personne ne l'a jamais vu ; devenue vraiment transparente, elle ne repeignait plus rien. La fenêtre reçoit depuis un fond **noir pur**, effacé sur toute sa surface et rendu entièrement transparent par le compositeur.

D'où les deux choses que Victor rapportait sans qu'elles s'expliquent : ça part quand la souris arrive, et la capture d'écran ne la prend pas. Le survol rendait le logo entier, donc opaque, donc il la couvrait.

Voir [D101](../../DECISIONS.md) pour la décision, et [R66](../M4-PROTOCOLE.md) pour l'essai.

## `bouton-flottant-liseret-gauche.png`

Le bouton flottant sur PC-VICTOR pendant une session, 106 x 105 pixels, image extraite d'un enregistrement d'écran au moment d'un clic. C'est « l'artefact blanc à gauche » : celui qui apparaît en cliquant et qui fait un éclair.

**Deux choses s'y voient, et une seule est un défaut.**

La tache blanche ronde posée sur le coin bas droit de l'écran or est **le pointeur de la souris**, la main fermée que la page demande sur le logo pendant qu'on le tient (`cursor: grabbing`). Victor l'avait déjà dit d'une capture précédente. C'est aussi pourquoi les photos que le bouton prenait de lui-même n'en montraient jamais rien : aucune copie d'une fenêtre ne prend le pointeur.

Le défaut est **le liseré pâle le long du bord gauche**, dehors, avec du noir pur au-delà. Il suit toute la silhouette gauche du dessin, les deux écrans et l'arrondi entre eux compris.

**Ce que les pixels disent.** Sur une ligne au milieu de l'écran or : noir jusqu'à x = 28 (1,5,10), puis 117,122,129, puis **203,209,216**, puis 115,121,129, puis le contour sombre du dessin à 9,13,20. La bande claire est donc dehors, collée au dessin, large de deux pixels.

**Et la même marge, sur les trois autres bords, est noire** : 5,8,15 en haut, 6,9,15 à droite, 8,11,15 en bas. Mesuré sur la même image.

Cette marge est celle que `formeOccupee` prend au-delà de ce que la page peint (`MARGE = 1`), pour que la découpe ne morde jamais dans le bord lissé du dessin. C'est le seul endroit de cette fenêtre que la page ne peint pas, et le commentaire qui l'installe affirme qu'elle ne coûte rien depuis que la fenêtre est transparente. La mesure dit le contraire, à gauche seulement. Le pourquoi du « à gauche seulement » n'est pas su : c'est le seul bord qui bouge, la fenêtre étant accrochée par son coin haut droit et ne grandissant que vers la gauche.

Voir [D101](../../DECISIONS.md) et [R66](../M4-PROTOCOLE.md).

## `bouton-flottant-saut-menu.png`

Les photos 7 et 8 prises par le bouton lui-même sur PC-VICTOR, côte à côte et agrandies cinq fois. Ce sont deux images consécutives, recopiées de l'écran, au moment où le menu s'ouvre pour la première fois de la session.

Ce qu'elles montrent sans qu'il y ait rien à déduire : **le bouton entier saute de dix-huit pixels vers la gauche pendant une image**. Le trait bleu est au même endroit sur les deux et marque le bord gauche du logo sur la photo 7. Les deux photos sont prises au même endroit de l'écran, les deux fenêtres ayant le même bord droit (3827), donc le déplacement est réel et non un effet du cadrage.

**Ce que le journal en dit au même instant** : `1423x1353 demandés, 1405x1353 avant, 1423x1353 après`. La fenêtre s'élargit de dix-huit pixels à la première ouverture du menu, parce que c'est là que la barre des mesures est bâtie et que c'est elle qui décide de la largeur de la carte. La fenêtre étant accrochée par son coin haut droit, l'élargir déplace son bord gauche, et la vue web garde le temps d'une image son dessin d'avant collé au nouveau bord.

La barre est désormais posée dès le chargement, remplie de tirets : mesuré dans un navigateur, dix-huit pixels d'écart avant, zéro après.

## `bouton-flottant-decoupe-decalee.png`

La fenêtre du bouton flottant **photographiée par le produit lui-même**, découpe non appliquée, à l'instant exact où le menu s'ouvre. C'est la septième des huit que le bouton prend quand sa fenêtre change de taille, et c'est celle qui a donné la réponse après trois explications fausses.

Ce qu'elle montre : ouvrir le menu élargit cette fenêtre de dix-huit pixels, et **la carte du menu y est dessinée dix-huit pixels à gauche de la forme gardée pour elle**. La découpe était faite contre la largeur que la fenêtre allait avoir, alors que la page avait mesuré dans celle qu'elle avait encore. D'où une bande de fenêtre que personne n'a peinte le long d'un bord de la carte, et le logo rogné de l'autre : c'est le flash blanc.

Une capture d'écran ne pouvait pas le montrer, puisqu'elle ne montre que ce que la découpe laisse passer, c'est-à-dire précisément la partie qui a l'air juste.
