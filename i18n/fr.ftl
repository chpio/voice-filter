# The texts of i18n/en.ftl in French. A single line break is a space, an empty line a new
# paragraph (see src/i18n.rs). French typography: a narrow no-break space before : ; ? ! %.

## Stages, in the order of the chain
stage-echo = Annulation d’écho
stage-denoise = Suppression du bruit
stage-highpass = Filtre passe-haut
stage-eq = Égaliseur
stage-deesser = Dé-esseur
stage-agc = Contrôle automatique du gain
stage-compressor = Compresseur
stage-expander = Expandeur
stage-limiter = Limiteur

## The groups of stages in the window
group-echo = Annulation d’écho
group-echo-description =
    Retire du micro l’écho des haut-parleurs choisis ici, pour les appels sans casque.
group-denoise = Suppression du bruit
group-denoise-description =
    Retire le bruit de fond. La limite en garde une partie : plus bas sonne plus naturel, 100 dB
    retire tout.
group-tone = Timbre
group-tone-description =
    Retire les grondements, rend la voix plus claire (moins de résonance dans le bas médium, plus de
    présence) et adoucit les S sifflants.
group-level = Niveau
group-level-description =
    Amène la voix à un niveau constant, égalise le volume à l’intérieur des mots, garde le bruit bas
    dans les pauses et les crêtes sous −1 dBFS.

## Parameter titles (the daemon sends the same in English)
param-echo-protect = Protection de la voix
param-denoise-level = Niveau d’entrée
param-denoise-max-gain = Gain d’entrée maximal
param-denoise-limit = Limite
param-highpass-freq = Fréquence
param-eq-low-mid = Bas médium (250 Hz)
param-eq-presence = Présence
param-eq-presence-freq = Fréquence de présence
param-deesser-freq = Fréquence
param-deesser-threshold = Seuil
param-agc-max-gain = Gain maximal
param-agc-threshold = Seuil de parole
param-agc-target = Niveau cible
param-compressor-threshold = Seuil
param-compressor-ratio = Taux
param-expander-range = Plage
param-expander-threshold = Seuil de parole
param-expander-hold = Maintien
param-expander-level = S’ouvre au-dessus de
param-limiter-ceiling = Plafond
param-limiter-release = Relâchement
param-listen-volume = Volume

## What the stages do, inside their rows
help-stage-echo =
    Retire du micro ce que jouent les haut-parleurs choisis plus bas, pour que l’autre côté d’un
    appel ne s’entende pas lui-même quand tu ne portes pas de casque. Elle s’adapte au délai et à la
    pièce pendant que les haut-parleurs jouent.

    Ajoute 5 ms de latence.
help-stage-denoise =
    Un réseau de neurones (DeepFilterNet3) sépare la voix du bruit de fond, comme les ventilateurs,
    les claviers ou la circulation, et retire le bruit. Il entend un micro faible relevé à un niveau
    avec lequel il travaille bien.

    Ajoute 30 ms de latence, le plus de toutes les étapes, même désactivée (pourquoi{N}: voir Filtre
    tout en haut).
help-stage-highpass =
    Retire tout ce qui est sous sa fréquence : grondements, bruits de pas, chocs contre le bureau et
    les plosives des P et des B. La voix elle-même se situe au-dessus. Il vient en premier, pour que
    la suppression du bruit ne prenne pas le grondement pour une voix.
help-stage-eq =
    Façonne le timbre : baisse le bas médium autour de 250 Hz, qui rend une voix sourde ou
    caverneuse, et relève la présence autour de 3,5 kHz, qui rend la parole plus claire.
help-stage-deesser =
    Adoucit les S et les CH sifflants : tant que la bande au-dessus de sa fréquence représente une
    plus grande part du son que le seuil, il baisse cette bande, de 12 dB au plus. Le reste de la
    voix reste tel quel.

    Ajoute 1 ms de latence.
help-stage-agc =
    Amène la voix à un niveau constant, que tu parles doucement ou fort, près du micro ou plus loin.
    Il s’ajuste sur quelques secondes (monte de 6 dB par seconde au plus, descend plus vite) et
    garde le gain dans les pauses, avec un bruit constant comme un bourdonnement et pendant que tu
    tapes, pour ne pas les relever.
help-stage-compressor =
    Égalise le volume à l’intérieur des mots et des phrases, ce qui est trop rapide pour le contrôle
    du gain : il baisse les syllabes fortes au-dessus du seuil et relève le tout d’autant, pour que
    la voix sonne plus régulière et plus pleine au même niveau.
help-stage-expander =
    Baisse les pauses de la parole, pour que le bruit restant (que le contrôle du gain a relevé) ne
    s’entende pas entre les mots. Il s’ouvre dès qu’il entend de la parole, d’après le niveau ou
    d’après la certitude de la suppression du bruit, et se referme après le temps de maintien.
help-stage-limiter =
    Garde les crêtes sous le plafond, pour qu’un rire, une toux ou un mot fort ne saturent pas. Il
    anticipe de 1 ms, pour baisser avant une crête au lieu de la couper.

    Ajoute 1 ms de latence.

## What the parameters do, behind the info button
help-param-echo-protect =
    Quand tu parles par-dessus ce que jouent les haut-parleurs, l’annulation d’écho baisse ta voix
    avec l’écho. Pendant que tu parles, cette part de ta voix est gardée telle quelle et la
    suppression du bruit retire ce qui reste de l’écho, surtout la musique ; quand tu te tais,
    l’annulation d’écho agit pleinement. Plus haut garde plus de voix, mais laisse passer davantage
    de ce que jouent les haut-parleurs ; 0 % laisse tout à l’annulation d’écho.
help-param-denoise-limit =
    Jusqu’où elle peut baisser le bruit. 100 dB le retire entièrement ; autour de 20 à 30 dB, il
    reste un peu de fond, ce qui sonne plus naturel et coûte moins de voix.
help-param-denoise-level =
    Le niveau auquel la suppression du bruit entend ta voix. Elle prend une voix trop faible pour du
    bruit et la coupe, alors elle relève d’abord un micro faible à ce niveau puis retire ce gain
    ensuite : ce qui sort reste aussi fort. Plus haut garde plus d’une voix faible, mais laisse
    passer plus de bruit dans les pauses.
help-param-denoise-max-gain =
    Jusqu’où elle peut relever un micro faible pour la suppression du bruit. 0 dB désactive le
    relèvement.
help-param-highpass-freq =
    Tout ce qui est sous cette fréquence est retiré. Plus haut retire plus de grondements, mais
    amincit les voix graves ; 80 Hz convient à toutes les voix.
help-param-eq-low-mid =
    Le gain autour de 250 Hz. Sous 0 dB la voix sonne moins sourde et caverneuse, trop bas elle
    sonne mince.
help-param-eq-presence =
    Le gain autour de la fréquence de présence. Au-dessus de 0 dB la parole sonne plus claire et
    plus proche, trop haut agressive.
help-param-eq-presence-freq =
    Le centre de la bande de présence. Plus bas sonne plus chaud, plus haut plus brillant.
help-param-deesser-freq =
    Là où commencent les S : il baisse la bande au-dessus de cette fréquence. Plus bas en attrape
    davantage, mais baisse aussi plus de voix.
help-param-deesser-threshold =
    La part de la bande dans le son total au-dessus de laquelle il agit, 0 dB étant tout le son.
    Plus bas agit plus souvent et plus fort, plus haut seulement sur les S les plus sifflants.
help-param-agc-target =
    Le niveau auquel il amène la voix, en moyenne (RMS). −18 dBFS est courant pour les appels et
    laisse de la place aux crêtes. Le compresseur le suit.
help-param-agc-max-gain =
    Le plus qu’il relève une voix faible. Plus haut atteint une parole plus faible ou un micro plus
    éloigné, mais relève aussi son bruit.
help-param-agc-threshold =
    À quel point la suppression du bruit doit être sûre d’entendre de la parole (son rapport signal
    sur bruit) pour que le contrôle du gain la mesure. Plus haut laisse de côté plus de bruit, mais
    aussi des parties plus faibles des mots, donc il relève moins la voix ; −20 dB s’en remet au
    seul niveau, comme avec la suppression du bruit désactivée.
help-param-compressor-threshold =
    Au-dessus de ce niveau, il baisse. Plus bas égalise davantage, les syllabes normales aussi, plus
    haut seulement les plus fortes.
help-param-compressor-ratio =
    De combien il baisse au-dessus du seuil : à 3 :1, 3 dB de plus en entrée donnent 1 dB de plus en
    sortie. 1 :1 ne fait rien, plus haut égalise davantage mais sonne écrasé.
help-param-expander-range =
    De combien il baisse les pauses. Plus les rend plus silencieuses, mais rend le passage entre
    parole et pause plus audible ; 0 dB le désactive.
help-param-expander-threshold =
    À quel point la suppression du bruit doit être sûre d’entendre de la parole (son rapport signal
    sur bruit) pour qu’il s’ouvre. Plus bas s’ouvre plus souvent, aussi pour du bruit, plus haut
    seulement pour une parole nette. Avec la suppression du bruit désactivée, seul le niveau compte.
help-param-expander-hold =
    Combien de temps il reste ouvert après la dernière parole. Plus long garde les fins de mots et
    les courtes pauses, plus court rend les pauses silencieuses plus tôt.
help-param-expander-level =
    Il s’ouvre aussi quand le son dépasse ce niveau, pour une parole faible dont la suppression du
    bruit n’est pas sûre. Plus bas s’ouvre plus souvent, aussi pour du bruit ; les pauses se situent
    en général entre −55 et −65 dBFS.
help-param-limiter-ceiling =
    Le plus haut que peuvent monter les crêtes. −1 dBFS laisse de la place à ce qu’ajoutent les
    codecs quand ils compressent le son pour l’appel.
help-param-limiter-release =
    La vitesse à laquelle il relâche après une crête. Plus court sonne plus fort, mais distord plus
    tôt.

## What the other rows show
help-filter =
    Active ou désactive tout le filtre. Désactivé, les applications reçoivent le micro tel quel,
    tandis que chaque étape garde son propre interrupteur et ses réglages pour quand il est
    réactivé. L’icône de la zone de notification le bascule aussi.

    La latence reste la même quand il est désactivé : le micro est retardé d’autant que le filtre en
    aurait besoin (environ 37 ms, dont 30 ms pour la suppression du bruit ; la ligne Latence plus
    bas montre le tout). Supprimer ce retard couperait un morceau du son à la désactivation et
    insérerait un blanc à l’activation, entendu comme un clic ou une coupure dans un appel ; ainsi,
    la bascule n’est qu’un fondu enchaîné.
help-listen-volume =
    Le volume auquel tu t’entends. Seulement pour l’écoute : les applications reçoivent la voix au
    niveau où le filtre l’amène.
help-input-level =
    Le volume du micro, avant le filtre ; la barre va de −60 à 0 dBFS.

    Le bruit de fond est son moment le plus silencieux des 2 dernières secondes, ce qu’il capte
    pendant que tu te tais. Sous −60 dBFS, c’est calme ; au-dessus de −45 dBFS, la suppression du
    bruit a du mal et laisse un souffle. Une amplification du micro poussée (dans alsamixer souvent
    « Mic Boost » ou « Internal Mic Boost ») relève le bruit ; un casque-micro ou un micro USB capte
    en général beaucoup moins.
help-output-level =
    Le volume auquel la voix sort du filtre, ce que reçoivent les applications ; la barre va de −60
    à 0 dBFS. La parole devrait se situer autour du niveau cible du contrôle du gain (−18 dBFS), les
    pauses bien en dessous.
help-latency =
    Le temps que met la voix du micro jusqu’à l’application : le tampon du micro, le tampon avant le
    micro virtuel, ce dont les étapes ont besoin (la suppression du bruit 30 ms) et le tampon de
    l’application.
help-processing =
    Le temps qu’ont mis 10 ms d’audio à traverser les étapes pendant les 10 dernières secondes : la
    moyenne, le temps dans lequel sont restés 95 % et 99 % d’entre eux (arrondi au plus de 2 % vers
    le haut) et le plus long. Le tout, l’annulation d’écho et la suppression du bruit séparément (ce
    sont elles qui prennent le plus) et les autres étapes ensemble. Le temps le plus long doit
    rester bien en dessous de 10 ms, sinon il y a des coupures ; une étape désactivée ne prend
    presque rien.

    PipeWire transmet l’audio par cycles, les échantillons par cycle en sont la taille (48
    échantillons font 1 ms). Le filtre en demande 480, les applications qui ont besoin d’une latence
    plus faible peuvent la réduire.
help-dropouts =
    Depuis le démarrage du filtre.

    Silence : de l’audio qui n’était pas prêt à temps ; le micro virtuel a joué du silence à la
    place, un blanc. Un peu juste après qu’une application commence à enregistrer est normal, le
    temps que le tampon se stabilise.

    Abandonné : de l’audio jeté pour faire redescendre la latence, par exemple après une pause du
    filtre, un saut.

    Erreurs : 10 ms d’audio sur lesquelles une étape a échoué, remplacées par du silence.

## The window
window-title = Voice Filter
main-menu = Menu principal
more-information = Plus d’informations
menu-reset = _Réinitialiser le filtre…
reset-heading = Réinitialiser le filtre ?
reset-body =
    Active toutes les étapes et remet leurs réglages par défaut. Le micro et les haut-parleurs
    restent tels quels.
reset-cancel = _Annuler
reset-confirm = _Réinitialiser
missing-title = Voice Filter ne fonctionne pas
missing-description = Lance-le avec <tt>voice-filter run</tt>
missing-no-bus = Impossible de se connecter au bus de session : { $error }
missing-no-proxy = Impossible de créer le proxy : { $error }
copy = Copier
copied = Copié
group-input = Entrée
group-input-description = Le filtre lit ce micro, les applications lisent « Voice Filter ».
filter = Filtre
filter-subtitle = Désactivé, les applications reçoivent le micro tel quel
microphone = Micro
speakers = Haut-parleurs
default-input = Entrée par défaut
default-output = Sortie par défaut
unplugged = { $device } (débranché)
listen = Écouter
listen-subtitle = Le joue sur la sortie par défaut, utilise un casque
listen-volume = Volume
keep-listening = Garder l’écoute
keep-listening-subtitle =
    Reprend l’écoute après un redémarrage. Le micro reste utilisé et le filtre fonctionne en
    permanence.
echo-status = État
echo-not-in-chain = Absente de la chaîne
echo-not-running = Inactive : désactivée, ou aucune application n’enregistre « Voice Filter »
echo-silent = Les haut-parleurs sont silencieux, il n’y a pas d’écho à retirer
echo-delay-short = moins de 4 ms
echo-delay = environ { $delay } ms
echo-hears = Entend les haut-parleurs à { $level } dBFS, leur écho revient après { $delay }
default-settings = Réglages par défaut
reset = Réinitialiser
group-levels = Niveaux
level-input = Micro
level-output = Voice Filter
input-not-running = Inactif : aucune application n’enregistre « Voice Filter »
input-noise = Bruit de fond à { $floor } dBFS
input-noise-loud =
    Bruit de fond à { $floor } dBFS, très fort : la suppression du bruit laisse un souffle. Baisse
    l’amplification du micro si elle est poussée, ou utilise un casque-micro
group-status = État
latency = Latence
latency-value =
    { $total } ms jusqu’à l’application : micro { $microphone }, tampon { $bridge }, traitement {
    $chain }
processing = Temps de traitement
processing-mean = Moyenne
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = Le plus long
processing-all = Toutes les étapes
processing-other = Autres étapes
processing-summary = { $mean } ms par trame de 10 ms, au plus { $max } ms
processing-nothing = Rien de traité pendant les 10 dernières secondes
processing-cycle = { $samples } échantillons par cycle
processing-copied = Par trame de 10 ms, en ms : { $columns }
dropouts = Coupures depuis le démarrage
dropouts-value =
    { $silence } ms de silence, { $dropped } ms abandonnées, { $errors ->
        [one] { $errors } erreur
       *[other] { $errors } erreurs
    }

## The icon in the tray
tray-title = Voice Filter
tray-on = Activé
tray-off = Désactivé : le micro passe sans filtre
tray-filter = Filtre
tray-open = Ouvrir Voice Filter
