# FlashGet v2 — Refonte de l'interface et mise à niveau produit

**Date :** 2026-11-10
**Statut :** design en relecture, maquettes validées
**Maquettes :** [`2026-11-10-flashget-v2-maquettes/`](2026-11-10-flashget-v2-maquettes/) (pages HTML statiques, à ouvrir dans un navigateur)

## 1. Intention

FlashGet a aujourd'hui un moteur de téléchargement solide (multi-segments, reprise, limiteur, HLS/DASH en VOD et en direct, ClearKey, yt-dlp, extension navigateur), mais l'application autour donne une impression de prototype : un seul écran, des réglages dans une boîte de dialogue, pas de choix de format confortable, pas de bibliothèque, pas de mises à jour.

L'objectif est que FlashGet **donne l'impression d'un produit fini** : une application organisée en pages, agréable, qui s'entretient toute seule, au niveau de finition des meilleurs gestionnaires de téléchargement modernes. Le propriétaire accepte de **refaire entièrement l'interface** et de **changer la logique** là où c'est nécessaire.

## 2. Objectifs, non-objectifs, anti-objectifs

**Objectifs**
- Nouvelle interface organisée en pages, avec menu latéral (section 4).
- Choix du format avant téléchargement, pour tous les liens média.
- Bibliothèque des téléchargements terminés, avec lecteur intégré.
- File d'attente réordonnable et planificateur par plage horaire.
- Paramètres complets en 8 sections.
- Thèmes sombre (par défaut), clair et « Système ».
- Mise à jour automatique de yt-dlp et de FlashGet.
- Installeur Windows propre.

**Non-objectifs**
- macOS et Linux (Windows d'abord, comme dans le design d'origine).
- Changer le cœur du moteur (`download-core`) au-delà de ce qu'exigent les nouvelles fonctions.

**Anti-objectifs** (ce qui rendrait la refonte ratée même si elle « marche »)
- **Perdre une capacité du moteur actuel** : multi-segments, reprise après crash, HLS/DASH VOD et direct, ClearKey, découpe d'extrait, extraction MP3, ajout groupé, capture par l'extension, connexion YouTube. Tout doit rester accessible dans la nouvelle interface.
- **Reprendre du code d'un autre projet.** NeoDLP a servi uniquement de référence visuelle. Aucune ligne de code, aucun composant, aucune ressource graphique n'en est reprise, et FlashGet ne mentionne aucun tiers.
- Une interface plus jolie mais plus lente ou moins fiable que l'actuelle.

## 3. Contraintes

- Pile actuelle conservée : Tauri 2, React 19, TypeScript, Tailwind, Rust (crate `download-core` + couche `commands`), SQLite (`rusqlite`).
- Identité conservée : nom FlashGet, icône ambre actuelle, polices Archivo (texte) et JetBrains Mono (chiffres, tailles, vitesses).
- Toute l'interface est en français.
- Les binaires embarqués (ffmpeg, yt-dlp) restent dans l'installeur.

## 4. Interface validée

Toutes les décisions de cette section ont été prises par le propriétaire sur les maquettes.

### 4.1 Thème — maquette `01-fenetre-principale.html`
- **Deux thèmes**, choisis dans Paramètres › Général : **Sombre (par défaut)**, Clair, Système (suit Windows).
- Sombre = direction A de la maquette (fond bleu nuit, accent ambre). Clair = direction B (fond clair, l'ambre réservé aux éléments importants).

### 4.2 Structure de la fenêtre
- Barre de titre personnalisée (logo + « FlashGet », boutons de fenêtre).
- **Menu latéral**, dans cet ordre : bouton **Nouveau** (mis en avant), Téléchargements, Terminés, File d'attente, Planificateur, puis Paramètres en bas. Compteurs à droite des entrées.
- **Barre d'état** en bas : nombre d'actifs et en file, limite de vitesse, état de l'extension, vitesse totale.
- Les catégories (Vidéos, Musique, Documents, Programmes, Archives) sont des **filtres** dans les pages concernées, pas des entrées du menu (version finale de la maquette 04).

### 4.3 Page « Téléchargements » (en cours) — maquette 01
- En-tête : titre, recherche, « Tout suspendre », « Ajouter un lien » (qui mène à la page Nouveau).
- Onglets de filtre : Tous, Actifs, En pause, Erreurs (avec compteurs).
- Une ligne par téléchargement : vignette (avec durée pour les vidéos), nom, badge de format/catégorie, site, mode (yt-dlp, nombre de connexions).
- **Barre de progression découpée en segments** pour les téléchargements multi-connexions (une sous-barre par segment) ; barre simple sinon.
- États visuels distincts : en cours, en direct (badge LIVE, durée enregistrée), en file, en erreur (rouge + action Reprendre), terminé (vert).
- Actions rapides par ligne : pause/reprise (ou arrêt pour un direct), ouvrir le dossier, menu « … ».

### 4.4 Page « Nouveau » — maquette `02-ajout-lien.html`, **option 2 retenue**
- Champ de lien en haut (bouton Coller) + bouton **Plusieurs liens** (ajout groupé).
- Pour un lien média, après analyse :
  - à gauche : grand aperçu, titre, chaîne/site, date, badges (nombre de formats, sous-titres disponibles), puis les options : intégrer les sous-titres, découper un extrait ;
  - à droite : sélecteur **Vidéo + audio / Audio seul / Vidéo seule**, liste des formats (résolution, conteneur et codecs, taille estimée, images/s), le format recommandé est présélectionné et marqué ; puis dossier de destination, nom du fichier, boutons « Ajouter à la file » et « Télécharger · taille ».
- Pour un fichier simple (non média) : icône de catégorie, nom, taille, catégorie, indication « reprise possible » et « accélération possible », dossier, nom, nombre de connexions.

### 4.5 Page « Terminés » — maquette `04-termines-file-planif.html`, **les deux présentations retenues**
- Bouton de bascule **Liste / Grille** ; **Liste par défaut**.
- Filtres par catégorie (puces avec compteurs) et recherche.
- **Liste** : regroupée par jour (Aujourd'hui, Hier, date), avec un **panneau de détail à droite**.
- **Grille** : vignettes ; un clic ouvre le même panneau de détail.
- Panneau de détail : **lecteur intégré** (vidéo et audio) pour les fichiers lisibles, infos (format, taille, durée, date et durée du téléchargement, dossier, source), actions Ouvrir, Dossier, Copier le lien, Re-télécharger, Supprimer (avec confirmation).

### 4.6 Page « File d'attente » — maquette 04 (proposition unique, sans remarque)
- Liste numérotée, **réordonnable par glisser-déposer**.
- Par ligne : poignée, rang, vignette, nom, site, priorité (haute, normale, basse), taille, retrait.
- En-tête : résumé (nombre, taille totale, règle de démarrage), « Suspendre la file », « Tout démarrer maintenant ».
- Un élément programmé l'indique (« programmé à 02:00 »).

### 4.7 Page « Planificateur » — maquette 04 (proposition unique, sans remarque)
- Interrupteur Activé, et rappel du prochain démarrage.
- **Quand** : heure de début et de fin, jours de la semaine, frise de 24 h qui visualise la plage.
- **Pendant la plage** : limite de vitesse optionnelle ; action à l'heure de fin si ce n'est pas terminé (mettre en pause par défaut).
- **Quand tout est terminé** : action (ne rien faire, veille, arrêt…), avec un compte à rebours de 60 s pour annuler.

### 4.8 Page « Paramètres » — maquette `03-parametres.html` (validée)
Sous-menu de 8 sections ; un point ambre signale une section qui demande l'attention (ex. mise à jour disponible).
1. **Général** : thème (Sombre/Clair/Système), lancement au démarrage de Windows, fermer = réduire dans la zone de notification, langue (voir 8.3).
2. **Téléchargements** : simultanés max, connexions par fichier (curseur), limite de vitesse globale, nouvelles tentatives, reprise automatique au démarrage, conduite si le fichier existe (renommer par défaut), action quand la file est terminée.
3. **Dossiers** : un dossier par catégorie (existant).
4. **Vidéo & yt-dlp** : version de yt-dlp + bouton de mise à jour, mise à jour automatique, qualité vidéo préférée, format audio (MP3/M4A/Opus) et débit, langues de sous-titres, compte YouTube (connexion intégrée existante), cookies d'un navigateur en repli (existant).
5. **Extension navigateur** : état de la connexion, jeton (régénérer), installation, sites ignorés.
6. **Notifications** : fin de téléchargement (existant), erreurs.
7. **Mises à jour** : version de FlashGet, vérification et installation des mises à jour.
8. **À propos**.

## 5. Changements de logique nécessaires

Ce que l'interface validée exige côté moteur et application (le « comment » est laissé au builder, sauf mention).

- **Ordre manuel de la file** : aujourd'hui l'ordonnanceur choisit par priorité puis par ordre d'arrivée. Il faut un rang de file persistant, modifiable par glisser-déposer, utilisé à priorité égale.
- **Planificateur horaire** : nouvelle notion (plage, jours, limite pendant la plage, action en fin de plage, action en fin de file). Persisté, survit au redémarrage.
- **Nouveaux réglages persistés** : connexions par fichier, nouvelles tentatives, reprise automatique, conduite en cas de conflit de nom, action de fin de file, qualité/format audio/débit/sous-titres par défaut, thème, mise à jour auto de yt-dlp.
- **Sous-titres** via yt-dlp (téléchargement et intégration).
- **Mise à jour de yt-dlp** depuis l'application (vérifier, télécharger, remplacer le binaire), manuelle et automatique.
- **Mise à jour de FlashGet** via le mécanisme de mise à jour de Tauri.
- **Lecteur intégré** : lecture de fichiers locaux dans la fenêtre.
- **Métadonnées pour la bibliothèque** : vignette, durée, date de fin, durée de téléchargement, regroupement par jour.
- **Identifiant de l'application** : `com.steph.tauri-app` est un reste du gabarit ; à remplacer par un identifiant propre à FlashGet avant la première version publique, car il fixe le dossier des données.

## 6. Laissé au builder

- Découpage en lots et ordre de livraison, tant que l'application reste utilisable à chaque étape.
- Choix des bibliothèques front (routage, état, composants), à condition qu'elles ne viennent pas de NeoDLP et que l'identité visuelle des maquettes soit respectée.
- Les maquettes fixent l'organisation, les contenus et l'ambiance ; les détails pixel (espacements, tailles exactes) sont libres.
- Le rendu clair (thème B) des pages autres que la fenêtre principale, en suivant la direction B de la maquette 01.
- La structure du schéma SQLite et ses migrations, en préservant les données existantes.
- Le format des événements entre Rust et le front.

## 7. Décisions du propriétaire après relecture

1. **« Télécharger » et « Ajouter à la file » sont différents, comme dans IDM.**
   - « Télécharger » démarre dès qu'une place se libère (dans la limite des téléchargements simultanés).
   - « Ajouter à la file » met l'élément **en attente** : il ne démarre que quand l'utilisateur lance la file (« Tout démarrer maintenant ») ou que le planificateur le fait. Cela demande un état « en attente de la file » distinct de l'état actuel `Queued`.
2. **Le planificateur contrôle toute la file.** Pendant la plage horaire, il démarre les éléments en attente de la file. En dehors de la plage, ils ne démarrent pas seuls. « Télécharger » reste immédiat, et un démarrage manuel passe toujours avant le planificateur.
3. **Mises à jour de FlashGet publiées sur GitHub Releases**, sur le dépôt GitHub de FlashGet. Il faudra rendre ces versions publiques et générer une paire de clés de signature des mises à jour, que le propriétaire garde.
4. **Un lien capturé par l'extension ouvre la page « Nouveau »**, avec FlashGet au premier plan et le lien prérempli. L'utilisateur confirme le format, le dossier et le nom. La capture ne démarre plus de téléchargement sans confirmation.

## 8. Décisions prises par Claude (à vérifier par le propriétaire)

Points non tranchés pendant les échanges, réglés ici avec une option raisonnable. Le propriétaire peut les changer.

1. **Frontière Téléchargements / Terminés.** Un téléchargement terminé quitte la page Téléchargements et apparaît dans Terminés. Le compteur de Terminés augmente et une notification le signale. (La ligne « terminé » de la maquette 01 n'était là que pour montrer l'état visuel.)
2. **« Supprimer » dans Terminés.** La confirmation propose deux choix : « Retirer de la liste » (le fichier reste sur le disque) ou « Supprimer aussi le fichier » (envoyé à la corbeille de Windows, pas effacé définitivement).
3. **Langue.** Interface en français uniquement, mais tous les textes sont regroupés dans un fichier de traduction dès maintenant, pour pouvoir ajouter une langue plus tard sans tout reprendre. Le sélecteur de langue reste masqué tant qu'il n'y a qu'une langue.
4. **Action de fin de file.** Un seul réglage, partagé entre Paramètres › Téléchargements et le Planificateur (les deux écrans modifient la même valeur). Choix : ne rien faire, quitter FlashGet, mettre en veille, mettre en veille prolongée, éteindre.
5. **Case « Démarrer tout de suite » supprimée** de la page Nouveau : elle ferait double emploi avec les deux boutons (voir 7.1).
6. **« Tout démarrer maintenant »** lance les éléments en attente de la file en respectant la limite de téléchargements simultanés. **« Suspendre la file »** met en pause ceux qui sont en cours et empêche les autres de démarrer.
7. **Installeur** : installeur `.exe` (NSIS) installé pour l'utilisateur courant, sans droits administrateur. La version de yt-dlp mise à jour est rangée dans le dossier de données de l'application, et elle est utilisée en priorité sur celle fournie avec l'installeur.
8. **Pas de certificat de signature de code Windows** pour l'instant : SmartScreen avertira à l'installation. On pourra en ajouter un plus tard sans changer le reste.
9. **Extension** : son évolution fait partie du projet (ouverture de la page Nouveau, sites ignorés, état de connexion réel). Pour l'instant, elle est installée depuis le dossier fourni avec FlashGet ; Paramètres › Extension navigateur explique comment faire.
