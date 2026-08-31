# Captures gardées

Les défauts d'affichage qui se cherchent au pixel et qui coûtent une manipulation à refaire. Une capture rangée ici est une pièce à conviction : on la relit, on la mesure, on ne redemande pas à Victor de la reprendre.

## `bouton-flottant-croix.png`

Le bouton flottant pendant une session, sur PC-SAV, 91 x 98 pixels réels. C'est « la fameuse croix par-dessus le FAB », vue et revue depuis juillet, et elle est particulièrement pénible à attraper : l'outil de capture de Windows la fait disparaître le temps de son propre voile, donc Win+Maj+S ne la prend pas.

**Ce que les pixels disent, et c'est la réponse.** L'or du logo vaut `239,181,54` dans le dessin. Dans cette capture il vaut `131,99,30` là où le fond derrière est noir, et `228,201,139` là où il est clair. Le premier donne 131 / 239 = 0,548, le deuxième le confirme : **le bouton est composé à 0,55**, exactement l'opacité au repos écrite dans `bouton.css`.

Donc ce n'est pas une croix posée sur le bouton, c'est **l'écran de l'ordinateur d'en face vu au travers**. La croix est le bouton de fermeture d'une fenêtre qui se trouvait derrière. Ce sera autre chose la fois d'après.

Voir [D101](../../DECISIONS.md) pour la décision, et [R66](../M4-PROTOCOLE.md) pour l'essai.
