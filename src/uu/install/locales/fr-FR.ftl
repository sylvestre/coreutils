install-about = Copier SOURCE vers DEST ou plusieurs SOURCE(s) vers le
  RÉPERTOIRE existant, tout en définissant les modes de permission et propriétaire/groupe
install-usage = install [OPTION]... [FICHIER]...

# Messages d'aide
install-help-ignored = ignoré
install-help-compare = comparer chaque paire de fichiers source et destination, et dans certains cas, ne pas modifier la destination du tout
install-help-directory = traiter tous les arguments comme des noms de répertoires. créer tous les composants des répertoires spécifiés
install-help-create-leading = créer tous les composants principaux de DEST sauf le dernier, puis copier SOURCE vers DEST
install-help-group = définir la propriété du groupe, au lieu du groupe actuel du processus
install-help-mode = définir le mode de permission (comme dans chmod), au lieu de rwxr-xr-x
install-help-owner = définir la propriété (super-utilisateur uniquement)
install-help-preserve-timestamps = appliquer les temps d'accès/modification des fichiers SOURCE aux fichiers de destination correspondants
install-help-strip = supprimer les tables de symboles (aucune action Windows)
install-help-strip-program = programme utilisé pour supprimer les binaires (aucune action Windows)
install-help-target-directory = déplacer tous les arguments SOURCE dans RÉPERTOIRE
install-help-no-target-directory = traiter DEST comme un fichier normal
install-help-verbose = expliquer ce qui est fait
install-help-preserve-context = préserver le contexte de sécurité
install-help-context = définir le contexte de sécurité des fichiers et répertoires

# Messages d'erreur
install-error-unexpected-argument = erreur : argument inattendu '{ $arg }' trouvé
install-error-tip = conseil
install-error-similar-exists = un argument similaire existe :
install-error-similar-suggestion =

  conseil : un argument similaire existe : '{ $valid_arg }'

install-error-pass-as-value = conseil : pour passer '{ $arg }' comme valeur, utilisez '-- { $arg }'
install-error-usage = Utilisation :
install-error-more-info = Pour plus d'informations
install-error-try = essayez
install-error-generic = erreur
install-error-generic-with-details = erreur : { $details }
install-error-help-suggestion = Pour plus d'informations, essayez '--help'.
install-error-unknown-argument = { $error_word } : argument inattendu '{ $arg }' trouvé
install-error-unknown-argument-tip = { $tip_word } : pour passer '{ $arg }' comme valeur, utilisez '{ $tip_command }'
install-error-unknown-argument-usage = Utilisation : install [OPTION]... [FICHIER]...
install-error-unknown-argument-generic = erreur : argument inattendu
install-error-dir-needs-arg = { $util_name } avec -d nécessite au moins un argument.
install-error-create-dir-failed = échec de la création de { $path }
install-error-chmod-failed = échec du chmod { $path }
install-error-chmod-failed-detailed = { $path } : échec du chmod avec l'erreur { $error }
install-error-chown-failed = échec du chown { $path } : { $error }
install-error-invalid-target = cible invalide { $path } : Aucun fichier ou répertoire de ce type
install-error-target-not-dir = la cible { $path } n'est pas un répertoire
install-error-backup-failed = impossible de sauvegarder { $from } vers { $to }
install-error-install-failed = impossible d'installer { $from } vers { $to }
install-error-strip-failed = échec du programme strip : { $error }
install-error-strip-abnormal = le processus strip s'est terminé anormalement - code de sortie : { $code }
install-error-metadata-failed = erreur de métadonnées
install-error-invalid-user = utilisateur invalide : { $user }
install-error-invalid-group = groupe invalide : { $group }
install-error-omitting-directory = omission du répertoire { $path }
install-error-not-a-directory = échec de l'accès à { $path } : N'est pas un répertoire
install-error-override-directory-failed = impossible d'écraser le répertoire { $dir } avec un non-répertoire { $file }
install-error-same-file = '{ $file1 }' et '{ $file2 }' sont le même fichier
install-error-extra-operand = opérande supplémentaire { $operand }
  { $usage }
install-error-invalid-mode = Chaîne de mode invalide : { $error }
install-error-mutually-exclusive-target = Les options --target-directory et --no-target-directory sont mutuellement exclusives
install-error-mutually-exclusive-compare-preserve = Les options --compare et --preserve-timestamps sont mutuellement exclusives
install-error-mutually-exclusive-compare-strip = Les options --compare et --strip sont mutuellement exclusives
install-error-missing-file-operand = opérande de fichier manquant
install-error-missing-destination-operand = opérande de fichier de destination manquant après '{ $path }'
install-error-failed-to-remove = Échec de la suppression du fichier existant { $path }. Erreur : { $error }

# Messages d'avertissement
install-warning-compare-ignored = l'option --compare (-C) est ignorée quand un mode est indiqué avec des bits non liés à des droits

# Sortie détaillée
install-verbose-creating-directory = création du répertoire { $path }
install-verbose-creating-directory-step = install : création du répertoire { $path }
install-verbose-removed = supprimé { $path }
install-verbose-copy = { $from } -> { $to }
install-verbose-backup = (sauvegarde : { $backup })
