# Jalon M4 : le produit se pilote, et les moteurs deviennent les nôtres

Ce document se remplit au fur et à mesure du jalon. Chaque partie se teste dès qu'elle est écrite, sur les deux mêmes PC Windows qu'aux jalons précédents.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte.

---

## Partie 1 : le service tient les sessions, la ligne de commande ne tient plus rien

### Ce qui change, et pourquoi

Jusqu'ici, la fenêtre de commande qui lançait `zyr-cli connect` portait elle-même le tunnel. La fermer, ou la perdre, coupait la session. C'est tenable pour un outil de diagnostic, pas pour un produit : l'interface graphique du jalon M4 doit pouvoir être fermée, mise à jour ou plantée sans que l'image s'arrête.

Le service porte donc maintenant les deux bouts du tunnel, y compris celui des sessions sortantes. La ligne de commande lui demande une voie, lance le lecteur sur les adresses locales qu'il lui rend, et lui indique quel processus cette voie sert. Le service referme la voie tout seul quand ce processus disparaît.

**Conséquence à connaître : le service est désormais nécessaire sur le PC client aussi.** C'était prévu (décision D2), c'est le prix d'une interface qu'on peut fermer sans conséquence.

### Préparation

Sur les **deux PC**, en fenêtre **administrateur** :

```
git pull && cargo build --release && zyrdeskd install && zyrdeskd start && zyrdeskd status
```

Attendu : « En marche » des deux côtés. Si le service tournait déjà, `zyrdeskd stop` d'abord.

> **M4-R1 (le service répond)**
>
> Sur le **PC client**, sans rien d'autre de lancé :
>
> ```
> zyr-cli connect 1.2.3.4 --pair 0000000000000000000000000000000000000000000000000000000000000000
> ```
>
> Attendu : un refus qui parle d'une adresse injoignable, **et surtout pas** « le service ZyrDesk ne tourne pas ». Ce dernier message voudrait dire que la ligne de commande n'atteint pas le service, et rien d'autre ne marchera.

### Session normale

Sur le **PC client**, comme au jalon M3 :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote> --stats
```

> **M4-R2 (rien n'a régressé)**
>
> Attendu : le bureau du PC hôte s'affiche, exactement comme au jalon M3. La ligne « Taille de paquet » remplace l'ancienne, qui ne s'affichait que si le chemin l'avait réduite.
>
> Noter : la session s'ouvre-t-elle aussi vite ? L'image est-elle aussi fluide ?

### Le vrai test de cette partie

Session ouverte et bien en cours, **sur le PC client** : fermer la fenêtre de commande d'où est parti `zyr-cli connect`, à la croix, sans rien arrêter d'autre.

> **M4-R3 (la session survit à la fenêtre)**
>
> Attendu : **l'image continue, sans coupure ni saccade**. La souris et le clavier répondent toujours. C'est le critère principal de cette partie.
>
> Fermer ensuite la session normalement (la combinaison de touches habituelle du lecteur, ou la croix de la fenêtre vidéo).

Puis, toujours sur le **PC client** :

```
notepad data\logs\service.log
```

> **M4-R4 (la voie se referme toute seule)**
>
> Attendu, dans l'ordre, vers la fin du journal : `way 1 open towards ...`, puis `way 1 now serves process ...`, puis, après la fermeture du lecteur, `way 1 has nothing left to serve` suivi de `way 1 closed`.
>
> C'est ce qui garantit qu'une fenêtre fermée brutalement ne laisse pas un tunnel ouvert derrière elle. Sans ces lignes, chaque session abandonnée laisserait une fuite.

### Si quelque chose ne va pas

**« le service ZyrDesk ne tourne pas ».** Le service n'est pas démarré sur le PC **client** (`zyrdeskd status`), ou il n'a jamais été installé de ce côté. C'est nouveau à ce jalon.

**Le journal du service dit `control channel unavailable`.** Le service tourne mais n'a pas pu ouvrir son canal de commande. Un autre programme occupe le nom `\\.\pipe\ZyrDesk`, ou un service ZyrDesk plus ancien tourne encore. Vérifier qu'il n'y a qu'un seul `zyrdeskd.exe` dans le gestionnaire des tâches.

**« réponse incompréhensible du service ».** Les deux moitiés du produit ne datent pas du même jour : le service tourne sur un exécutable plus ancien que `zyr-cli`. Recompiler et relancer le service.

**La session se coupe quand même en fermant la fenêtre.** Regarder si un avertissement est apparu au lancement (« le service n'a pas pris la session en charge »). Il signifie que le service n'a pas pu être prévenu du processus à surveiller, et que la session reste attachée à la fenêtre.
