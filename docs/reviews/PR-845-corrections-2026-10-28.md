# PR 845 — corrections et validation du 28 octobre 2026

## Référence examinée

La [PR pnn64/deadsync#845](https://github.com/pnn64/deadsync/pull/845) a été fusionnée le 28 octobre 2026 à 08:57, heure de Paris. Son dernier commit est `ebc8eaa636da6746235dafc650b40645588a3195`; son commit de fusion est `fdfa2375d0430a5bbb792c60a3ca4e5fb0632d3b`.

Ce suivi part du dernier commit de la PR, sur la branche locale `codex/pr845-course-atomicity`. Les deux commentaires de revue de pnn64 et la réponse de zydon69 ont été lus dans GitHub. L'API ne renvoie aucune revue formelle ni discussion de revue en ligne supplémentaire. La première recherche de cette conversation utilisait le fork `zydon69/deadsync`, qui n'héberge pas cette PR : son erreur 404 ne signifiait pas que les échanges étaient inaccessibles.

## Demandes de pnn64

| Demande | État dans le code examiné | Preuve exécutée |
| --- | --- | --- |
| Jugements indépendants et score Routine commun | Déjà corrigé par la PR fusionnée | `routine_participants_judge_independently_into_one_shared_score` ; test des notes simultanées renforcé pour Dance et Pump |
| Les collisions Routine ne doivent pas rejeter le morceau et ses autres charts | Déjà corrigé | `routine_collision_keeps_the_song_and_its_single_chart` et tests de collisions de `partner_charts` |
| Navigation et noms Simply Love | Déjà corrigé | `two_joined_players_change_style_like_simply_love_sort_menu`, suites de sélection et d'évaluation |
| HUD ITG normal et prédictif commun aux deux participants | Déjà corrigé par `b5275ea` | `routine_hud_shows_the_shared_score_for_both_participants` |
| Cumul Routine des chansons précédentes du cours, distinct du score de la chanson | Déjà corrigé par `b5275ea` et `ebc8eaa` | `routine_shared_course_score_keeps_earlier_songs`, `routine_shared_course_player_scores_both_participants_over_the_course`, `routine_course_saves_and_highlights_only_the_shared_master_result` |
| Vitesse et Mini du maître appliqués aux deux champs, préférences sauvegardées préservées | Déjà corrigé | `routine_starts_both_fields_with_the_master_speed_and_mini`, qui vérifie également les options de replay et le contrôle Versus |

Les références exactes demandées par la revue ont aussi été consultées : ITGmania `8a3bf8884acdcb69363f7f0ea592138e398c71e4` pour le score partagé et Simply Love `dd06138b15492f4136796dfe4b6708ced0f7b9eb` pour le [choix du score Routine dans le HUD](https://github.com/Simply-Love/Simply-Love-SM5/blob/dd06138b15492f4136796dfe4b6708ced0f7b9eb/BGAnimations/ScreenGameplay%20underlay/PerPlayer/Score.lua) et l'[application de vitesse et Mini du maître](https://github.com/Simply-Love/Simply-Love-SM5/blob/dd06138b15492f4136796dfe4b6708ced0f7b9eb/BGAnimations/ScreenGameplay%20underlay/default.lua).

## Corrections supplémentaires effectuées

### Cours Endless : annuler le cycle entier si un chargement échoue

Auparavant, si la première nouvelle chanson était chargée puis la deuxième échouait, la première restait ajoutée au cours malgré le retour d'échec. Les totaux et la liste des étapes décrivaient alors un cycle partiel.

Les totaux restent maintenant provisoires jusqu'à la réussite de toutes les étapes. En cas d'erreur, les étapes ajoutées pendant cette tentative sont retirées. Les étapes antérieures, leurs buffers, le curseur et les totaux restent inchangés. La capacité du vecteur peut être conservée après un agrandissement ; cela n'ajoute aucune étape au cours et évite de réallouer au prochain essai.

Cette méthode ne crée aucune collection temporaire et ne recharge pas les chansons pour valider. Le test existant `reserved_endless_append_needs_no_transient_allocations` passe sans assouplir ses assertions.

### Erreurs de totaux explicites et calcul atomique par étape

Un chart manquant ou une erreur de lecture contient désormais le chemin du simfile, son type, l'index de difficulté et le motif de chargement. Le journal conserve ce contexte avant que les interfaces existantes renvoient `None` ou `false`. Un retour du chargeur avec zéro ou plusieurs payloads est rejeté proprement au lieu de provoquer un accès au premier élément d'une liste vide.

Les totaux d'une étape sont appliqués ensemble après validation des deux participants : une erreur sur P2 n'applique pas les totaux de P1. Le filtrage existant des entrées du plan dont le hash ne se résout pas reste identique.

### Un chargement par chart partenaire et prise en compte de la casse

Quand les deux participants jouent le même chart, le payload déjà chargé est utilisé pour calculer les deux parts. Auparavant, le chart était chargé séparément pour chaque participant. La reconnaissance des types partenaires ignore aussi la casse, comme les recherches de charts de `SongData`.

### Entrées Pump Routine jusqu'à la colonne 19

Le test des notes simultanées passe maintenant par `queue_live_input_edge`, et vérifie Dance et Pump. Il couvre le remappage de P2Right, la diffusion aux deux champs, deux jugements distincts et le relâchement des colonnes 0/10 et 9/19 pour Pump. Les tests précédents de jugement Routine utilisaient principalement Dance et ses 16 colonnes.

Quatre nouveaux tests de cours utilisent de vrais simfiles SM synthétiques et les fonctions du parser/cache du projet. Ils couvrent les totaux avec tap/hold/roll/mine, les quatre styles partenaires, le chargement unique, l'échec après une première étape réussie, l'annulation puis une nouvelle tentative réussie, le payload invalide et le chart P2 manquant.

## Résultats exécutés

| Composant | Réussis | Échecs | Ignorés |
| --- | ---: | ---: | ---: |
| deadsync-chart | 43 | 0 | 4 |
| deadsync-core | 11 | 0 | 0 |
| deadsync-gameplay | 838 | 0 | 2 |
| partner_modes, intégration | 3 | 0 | 0 |
| deadsync-input | 78 | 0 | 5 |
| deadsync-lights | 42 | 0 | 1 |
| deadsync-notefield | 487 | 0 | 2 |
| deadsync-profile-gameplay | 25 | 0 | 0 |
| deadsync-profile | 236 | 0 | 4 |
| deadsync-rules | 128 | 0 | 4 |
| deadsync-score | 254 | 0 | 4 |
| deadsync-shell, exécution finale | 415 | 0 | 11 |
| deadsync-simfile | 289 | 0 | 21 |
| deadsync-smx | 104 | 0 | 1 |
| deadsync-theme-simply-love | 1453 | 14 | 54 |

Au total, ces contrôles comptent **4 406 tests réussis, 14 échecs préexistants et 113 tests ignorés**. Les répétitions ciblées et les comparaisons ne sont pas ajoutées à ce total.

Les 14 échecs du thème sont exactement les mêmes dans une exécution de comparaison du code fusionné `ebc8eaa`, sans le correctif de cours. Ils concernent les projections de téléchargements, les textes/allocations du navigateur de contenu et les statistiques d'entrée. Ils ne sont pas corrigés par ce suivi des modes partenaires. Le dépôt complet n'a donc pas une suite entièrement verte.

Une première exécution du shell a échoué dans `song_lua_conditional_spawn_skips_probed_receiver` (deux proxies visibles au lieu d'un), puis huit tests ont échoué sur le verrou empoisonné par cette assertion. La comparaison sans correctif passe (409 tests), puis l'exécution finale avec correctif passe aussi (413 tests). Ce résultat est signalé comme une instabilité observée, sans prétendre en avoir corrigé la cause.

Fixtures : le gitlink de la PR vise `f896283ab5e3a861ba8517569f156b1b34fa2ab6`. Ce commit n'est pas disponible localement et le serveur refuse sa récupération. Les contrôles utilisent la copie locale disponible `84c8ed135bafd4ef22a30d91db913972585456e2`, identique pour la comparaison. Le gitlink du dépôt n'a pas été modifié. Les nouveaux tests de cours construisent leurs propres simfiles et ne dépendent pas de ces fixtures externes.

`cargo clippy --locked -p deadsync-shell --lib --tests -- -A clippy::while_immutable_condition` termine avec succès. L'exception concerne une règle existante dans `deadsync-song-lua/src/cmd.rs`; des avertissements préexistants restent présents, aucun ne vise les fichiers de ce correctif. `rustfmt` vérifie les fichiers de cours modifiés et `git diff --check` passe. Seuls les blocs modifiés du grand fichier de régression gameplay ont été reformatés.

Les journaux locaux sont sous `target/` : `pr845-validation.log`, `pr845-baseline-validation.log`, `pr845-shell-final.log`, `pr845-gameplay-final.log`, `pr845-course-tests.log`, `pr845-clippy.log`, `pr845-profile.log`, `pr845-release-build.log` et `pr845-playtest-validation.log`. `pr845-comparison.json` associe les contrôles de parité aux résultats exécutés et conserve la liste exacte des échecs du thème.

## Reproduction

Depuis ce worktree :

```sh
cargo test --locked -p deadsync-shell --lib course:: -- --test-threads=1
cargo test --locked -p deadsync-gameplay --lib --test partner_modes -- --test-threads=1
cargo test --locked -p deadsync-shell --lib -- --test-threads=1
cargo test --locked --no-fail-fast -p deadsync-score -p deadsync-simfile -p deadsync-theme-simply-love --lib -- --test-threads=1
```

Les tests ignorés existants comprennent notamment des benchmarks et des captures longues. Ils ne sont pas comptés comme réussis. La validation ne constitue pas une session à deux personnes sur pads, ni une preuve de parité sur tous les modcharts.

## Suivi de relecture : transition Endless

La relecture a trouvé un défaut supplémentaire dans l'appelant : malgré l'annulation atomique du cycle, `prepare_player_options_for_course_stage` ignorait l'échec de renouvellement. L'accès modulo de `next_stage()` relançait les anciennes étapes ; après avancement du curseur au-delà de la liste, la condition de renouvellement par égalité ne se présentait plus et les totaux restaient ceux de l'ancien cycle.

Le renouvellement est désormais contrôlé **avant** de valider la transition d'écran. En cas d'échec de chargement, de sélection absente ou de curseur hors limites, la destination devient `SelectCourse`. Un message indique l'échec. Le cycle de sortie normal enregistre la chanson achevée, arrête le gameplay et libère le cours interrompu. Les anciens charts ne sont pas relancés et aucun nouvel état de gameplay n'est initialisé. Les remplacements d'écran demandés par Song Lua gardent leur traitement existant.

Les tests de `app/course_transition_tests.rs` construisent la vraie `App` sans GPU ni audio, une roue de sélection avec deux chansons et un cours Endless réels, ainsi qu'un état de gameplay terminé. Ils empruntent `prepare_global_fade_entry`, le chemin de préparation employé par `on_fade_complete`, et vérifient le changement d'écran, les commandes d'entrée, l'enregistrement du résultat, le nettoyage du cours et l'absence de relance. Pour Dance et Pump, ils couvrent le simfile supprimé après sélection, l'absence de plan et le curseur invalide. Le contrôle nominal démarre réellement les chansons successives et franchit deux frontières de cycles ; les curseurs et totaux sont vérifiés après initialisation.

Le test d'échec a aussi été exécuté avec l'ancienne transition rétablie temporairement : il échoue avec `Gameplay` obtenu au lieu de `SelectCourse`. Les sources corrigées ont ensuite été restaurées et les deux tests passent, y compris les trois variantes d'échec. La suite du shell passe **415 tests**, avec 11 tests préexistants ignorés. Les commandes de rendu et la boucle de fenêtre restent réservées à l'essai manuel ; ces tests valident le chemin d'état complet de la transition de l'application.

Journaux de ce suivi : `target/pr845-endless-before-fix.log`, `target/pr845-endless-transition-final.log`, `target/pr845-endless-shell-final.log`, `target/pr845-endless-clippy.log` et `target/pr845-endless-release.log`. La relecture des chemins normal, erreur, curseur hors limites et remplacement Song Lua n'a pas identifié d'autre défaut dans ce changement.

## Essai manuel préparé

La version d’essai est reconstruite avec le profil `release` et LTO pour macOS ARM64. Le script `build.rs` inscrit automatiquement le commit source abrégé `563a85c34c` dans `DEADSYNC_BUILD_HASH` et la date de ce commit dans `DEADSYNC_BUILD_STAMP`. Le libellé `pr845-endless-transition-2026-10-28` identifie le paquet dans le manifeste local. Le manifeste local `target/pr845-playtest/validation.json` contient le commit source exact, les empreintes des fichiers et le SHA-256 du binaire ; `deadsync.sha256` permet de contrôler la copie du dossier d’essai.

Le dossier local `target/pr845-playtest` utilise le mode portable du jeu : ses réglages, profils et scores sont isolés. `Tester-PR845.command` lance le binaire optimisé placé à côté. Les assets viennent du worktree par lien symbolique.

Le pack `PR845 Synthetic` contient `Partner A` et `Partner B`, avec huit charts par morceau : Single, Double, Couple et Routine pour Dance et Pump. Le son est un métronome synthétique à 120 BPM. Les premiers pas commencent après quatre secondes. Deux cours sont inclus : `PR845 Two Songs` et `PR845 Endless`. Le chargement des 16 charts et les types des deux cours ont été vérifiés par le parser du jeu.

1. Lancer `Tester-PR845.command`, joindre les deux joueurs et choisir le style partenaire depuis « Change Style To » dans la roue musicale.
2. Choisir des vitesses et Mini différents pour P1/P2 avant Routine. En jeu, les deux champs doivent utiliser la vitesse et le Mini de P1 ; après le morceau, les préférences de P2 doivent retrouver leurs valeurs propres.
3. Faire volontairement des résultats différents : le score ITG normal/prédictif doit être commun, les jugements et EX restent individuels. À l'évaluation, vérifier le score commun et les pourcentages P1/P2.
4. Jouer `PR845 Two Songs` avec des résultats différents entre les chansons : le résultat final doit cumuler les deux chansons. Vérifier qu'un seul résultat partagé Routine est enregistré pour P1.
5. En Endless, passer au cycle suivant. En Pump Routine, vérifier particulièrement le panneau droit de P2 et les relâchements.
6. Si le clavier ne répond pas, vérifier l'autorisation macOS « Surveillance de l'entrée » du Terminal, comme indiqué dans le README du projet.

L'annulation d'une erreur de chargement en milieu de cycle est couverte automatiquement : il n'est pas nécessaire de supprimer un simfile pendant votre session pour la reproduire.
