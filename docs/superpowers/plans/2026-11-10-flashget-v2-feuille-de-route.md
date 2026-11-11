# FlashGet v2 — Feuille de route

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md)

Le design couvre plusieurs sous-systèmes indépendants. Chaque phase a son propre plan détaillé, écrit au début de la phase, à partir du code tel qu'il est alors. Chaque phase laisse l'application utilisable et se termine par une fusion dans `main`.

| Phase | Contenu | Sections du spec | Plan |
|---|---|---|---|
| **1. Coque et page Téléchargements** | Fenêtre sans bordure et barre de titre, menu latéral, barre d'état, thèmes sombre/clair/système, fichier de textes FR, nouvelle page Téléchargements (onglets, barre segmentée par connexion), « Terminés » provisoire | 4.1, 4.2, 4.3, 8.1, 8.3 | [`2026-11-10-flashget-v2-phase-1-coque.md`](2026-11-10-flashget-v2-phase-1-coque.md) |
| **2. Page Nouveau** | Analyse du lien, Vidéo+audio / Audio seul / Vidéo seule, liste des formats, sous-titres, découpe, ajout groupé, fichier simple. État « en attente de la file » et boutons Télécharger / Ajouter à la file. L'extension ouvre la page Nouveau. Vignettes et durée enregistrées. | 4.4, 7.1, 7.4, 8.5 | à écrire |
| **3. Terminés (bibliothèque)** | Liste groupée par jour et grille, filtres par catégorie, panneau de détail, lecteur intégré, Retirer / Supprimer vers la corbeille, Re-télécharger | 4.5, 8.2 | à écrire |
| **4. File d'attente et Planificateur** | Rang de file persistant, glisser-déposer, Suspendre / Tout démarrer, plage horaire et jours, limite pendant la plage, action en fin de plage, action de fin de file (veille, arrêt…) avec compte à rebours | 4.6, 4.7, 7.2, 8.4, 8.6 | à écrire |
| **5. Paramètres** | Page en 8 sections, et les réglages moteur : connexions par fichier, nouvelles tentatives, reprise auto, conflit de nom, formats par défaut, sous-titres | 4.8, 5 | à écrire |
| **6. Mises à jour et installeur** | Mise à jour de yt-dlp (dossier de données en priorité), updater Tauri + GitHub Releases + clé de signature, installeur NSIS par utilisateur, nouvel identifiant d'application avec migration des données | 7.3, 8.7, 8.8, 5 | [plan](2026-11-11-flashget-v2-phase-6-mises-a-jour.md) |
| **7. Extension navigateur** | Ouverture de la page Nouveau à la capture, sites ignorés, état de connexion réel, aide à l'installation | 7.4, 8.9, 4.8 §5 | [plan](2026-11-11-flashget-v2-phase-7-extension.md) |

**Ordre :** les phases 2 à 5 dépendent toutes de la coque (phase 1). La phase 4 utilise l'état « en attente de la file » créé en phase 2. La phase 6 peut se faire à tout moment après la phase 1. La phase 7 suit la phase 2.
