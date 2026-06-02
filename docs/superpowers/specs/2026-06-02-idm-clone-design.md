# Clone d'IDM (Internet Download Manager) — Document de design

**Date :** 2026-06-02
**Statut :** Validé (design) — en attente de relecture finale avant plan d'implémentation
**Nom de code provisoire :** FlashGet (nom définitif à confirmer)

## 1. Objectif et contexte

Construire un gestionnaire de téléchargement complet, équivalent fonctionnel d'Internet Download Manager (IDM), en tant que **vrai produit distribuable** : installeur Windows signé, mises à jour automatiques, gestion d'erreurs robuste. Cible principale Windows, avec des variantes Mac/Linux prévues plus tard.

On part de zéro. La priorité produit : interface **propre, épurée et moderne**, et un cœur de téléchargement **performant et fiable**.

### Périmètre fonctionnel final (toutes les fonctionnalités d'IDM)

- Accélération par téléchargement **multi-segments** (multi-connexions parallèles).
- **Reprise** des téléchargements après coupure réseau, pause ou crash.
- **Intégration navigateur** : capture des liens de téléchargement (Chrome, Edge, Firefox).
- **Capture vidéo / streaming** : détection et grab des flux HLS (`.m3u8`) et DASH (`.mpd`).
- **File d'attente** et planification (programmer des téléchargements).
- **Limiteur de bande passante** (throttling) et limite de téléchargements simultanés.
- **Catégorisation automatique** des fichiers par type avec dossiers de destination.
- Recherche, filtres, historique.
- Notifications et actions de fin (ouvrir le dossier, éteindre le PC…).
- Thème clair/sombre, raccourcis clavier.

## 2. Choix technologique

**Tauri** (cœur Rust + frontend web React/TypeScript).

Justification face aux alternatives :
- **Tauri (retenu)** — interfaces web modernes (le « design épuré » le plus accessible), Rust idéal pour les téléchargements concurrents multi-segments, installeurs légers, auto-updater et multi-plateforme natifs.
- **Electron (écarté)** — UI facile mais binaires lourds (~80–150 Mo) et forte conso RAM, peu adapté à un outil système léger.
- **.NET + Avalonia (écarté)** — langage unique solide et multi-plateforme, mais design moderne plus laborieux à obtenir que le web.

**Décision produit :** **ffmpeg est embarqué** dans l'application (installeur plus lourd d'environ 30 Mo) pour une expérience « ça marche tout seul », plutôt que téléchargé à la demande.

## 3. Architecture générale

Architecture en couches, validée :

```
┌─────────────────────────────────────────────────────────┐
│  FRONTEND (React + TypeScript) — fenêtre & design         │
│  • Liste des téléchargements, barres de progression       │
│  • File d'attente, catégories, paramètres                 │
│  • Reçoit les events temps réel (vitesse, %, état)        │
└───────────────────────┬───────────────────────────────────┘
                        │  commandes Tauri (invoke) + events
┌───────────────────────┴───────────────────────────────────┐
│  CŒUR (Rust) — moteur invisible et performant              │
│  • Moteur de téléchargement multi-segments                 │
│  • Reprise pause / coupure / crash                         │
│  • Ordonnanceur de file + limite de bande passante         │
│  • Persistance locale (SQLite)                             │
│  • Hôte native messaging pour l'extension navigateur       │
│  • Grab vidéo (HLS/DASH) + assemblage via ffmpeg embarqué  │
└───────────────────────┬───────────────────────────────────┘
                        │
┌───────────────────────┴───────────────────────────────────┐
│  EXTENSION NAVIGATEUR (Chrome / Edge / Firefox)            │
│  • Capture liens & flux vidéo → envoie au cœur Rust        │
└────────────────────────────────────────────────────────────┘
```

**Principe clé :** le frontend ne fait que l'affichage. Toute la logique lourde (réseau, fichiers, segments) vit dans Rust → app rapide, légère et fiable même avec de nombreux téléchargements simultanés.

**Communication frontend ↔ cœur :**
- **Commandes** : le frontend appelle Rust (démarrer, pause, reprendre, annuler…).
- **Events** : Rust pousse vers le frontend en temps réel (progression, vitesse, état, erreurs).

## 4. Composants du cœur (Rust)

Modules à responsabilité unique, testables isolément, exposés au frontend via une couche `commands` (pont Tauri).

1. **`download-engine`** — découpe en N segments via requêtes HTTP *Range* (`bytes=…`), téléchargement parallèle, écriture de chaque segment à sa position dans le fichier final. Vérifie `Accept-Ranges` ; bascule en mono-flux si le serveur ne supporte pas le multi-segments.
2. **`resume-manager`** — sauvegarde régulière de la progression de chaque segment ; reprise exacte après coupure/crash (re-télécharge seulement les octets manquants).
3. **`scheduler`** — limite de simultanés (paramétrable), file d'attente, priorités, démarrage/arrêt programmés, limiteur de débit global (throttling).
4. **`storage`** — base **SQLite** locale : téléchargements, état, historique, catégories, paramètres. Survit aux redémarrages.
5. **`categorizer`** — classement automatique par type (Vidéos, Documents, Programmes, Archives, Musique…) selon extension/MIME, dossiers de destination configurables.
6. **`browser-host`** — serveur de native messaging recevant les liens capturés par l'extension et créant les téléchargements.
7. **`media-grabber`** — détection et reconstitution des flux HLS (`.m3u8`) et DASH (`.mpd`) : téléchargement des segments puis assemblage en un fichier unique via **ffmpeg embarqué**.

## 5. Design de l'interface

**Direction retenue : Option B — onglets en haut + tableau dense.**

- Barre d'actions en haut (Ajouter, Pause, Reprendre, recherche).
- Onglets de catégories (Tous, Vidéos, Docs, Programmes…).
- Tableau dense scannable : Nom · Taille · État (% / Terminé / En pause / Erreur) · Vitesse.
- Style épuré et moderne, proche de l'IDM classique mais plus propre ; accent indigo.
- Thèmes clair **et** sombre (sélectionnables par l'utilisateur — finition Phase 5).

Les maquettes de référence sont conservées dans `.superpowers/brainstorm/`.

## 6. Robustesse & gestion d'erreurs

- **Coupure réseau / serveur lent** : retentatives automatiques avec back-off exponentiel par segment ; en cas d'échec, statut « erreur » mais reste reprenable.
- **Serveur sans multi-segments** : bascule transparente en mono-flux.
- **Disque plein / chemin invalide** : détection avant écriture + message clair.
- **Lien expiré / 403** : message explicite + bouton « rafraîchir l'URL ».
- **Crash de l'app** : reprise complète à la réouverture via `resume-manager`.

## 7. Stratégie de tests

- **Cœur Rust** : tests unitaires par module (logique de segments, calcul de reprise, ordonnanceur) + tests d'intégration du moteur contre un faux serveur HTTP local. **Approche TDD** sur les modules à logique délicate (segments, reprise).
- **Frontend React** : tests de composants sur les états d'affichage (en cours, pause, erreur, terminé).
- **Bout en bout** : scénario complet (ajouter → pause → reprendre → terminer) automatisé sur les phases clés.

## 8. Feuille de route par phases

L'objectif final reste **toutes** les fonctionnalités. Construction incrémentale ; chaque phase est utilisable et testable, et fera l'objet de son propre plan d'implémentation détaillé.

- **Phase 1 — Cœur qui télécharge (MVP solide)** : moteur multi-segments + reprise (coupure/crash) ; ajouter une URL, démarrer/pause/reprendre/annuler ; persistance SQLite ; progression temps réel ; UI de base (tableau + barres de progression).
- **Phase 2 — Gestion complète** : file d'attente, priorités, limite de simultanés ; limiteur de bande passante ; catégories automatiques + dossiers de destination ; recherche/filtres ; historique.
- **Phase 3 — Intégration navigateur** : extension Chrome/Edge/Firefox + hôte native messaging ; capture automatique des liens.
- **Phase 4 — Vidéo & streaming** : détection des flux HLS/DASH ; grab + assemblage via ffmpeg embarqué.
- **Phase 5 — Confort & finitions** : planification ; notifications + action de fin ; thème clair/sombre ; raccourcis clavier.
- **Phase 6 — Distribution** : installeur Windows signé + auto-update ; (plus tard) builds Mac/Linux.

**Prochaine étape :** plan d'implémentation détaillé de la **Phase 1**.

## 9. Questions ouvertes / à décider plus tard

- Nom définitif du produit (provisoire : FlashGet).
- Choix de la bibliothèque de style frontend (Tailwind + composants maison, ou autre) — à trancher au plan de Phase 1.
- Détails de signature de code Windows et fournisseur de certificat (Phase 6).
