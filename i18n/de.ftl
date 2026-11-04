# The texts of i18n/en.ftl in German. A single line break is a space, an empty line a new
# paragraph (see src/i18n.rs).

## Stages, in the order of the chain
stage-echo = Echounterdrückung
stage-denoise = Rauschunterdrückung
stage-highpass = Hochpassfilter
stage-eq = Equalizer
stage-deesser = De-Esser
stage-agc = Automatische Pegelregelung
stage-compressor = Kompressor
stage-expander = Expander
stage-limiter = Limiter

## The groups of stages in the window
group-echo = Echounterdrückung
group-echo-description =
    Entfernt das Echo der hier gewählten Lautsprecher aus dem Mikrofon, für Anrufe ohne Kopfhörer.
group-denoise = Rauschunterdrückung
group-denoise-description =
    Entfernt Hintergrundgeräusche. Die Grenze lässt einen Teil davon übrig: niedriger klingt
    natürlicher, 100 dB entfernt alles.
group-tone = Klang
group-tone-description =
    Entfernt Rumpeln, macht die Stimme klarer (weniger Dröhnen in den unteren Mitten, mehr Präsenz)
    und zähmt scharfe S-Laute.
group-level = Pegel
group-level-description =
    Bringt die Stimme auf einen gleichmäßigen Pegel, gleicht die Lautstärke innerhalb von Wörtern
    aus, hält das Rauschen in Sprechpausen niedrig und die Spitzen unter −1 dBFS.

## Parameter titles (the daemon sends the same in English)
param-echo-protect = Stimmschutz
param-denoise-level = Eingangspegel
param-denoise-max-gain = Maximale Eingangsverstärkung
param-denoise-limit = Grenze
param-highpass-freq = Frequenz
param-eq-low-mid = Untere Mitten (250 Hz)
param-eq-presence = Präsenz
param-eq-presence-freq = Präsenzfrequenz
param-deesser-freq = Frequenz
param-deesser-threshold = Schwelle
param-agc-max-gain = Maximale Verstärkung
param-agc-threshold = Sprachschwelle
param-agc-target = Zielpegel
param-compressor-threshold = Schwelle
param-compressor-ratio = Verhältnis
param-expander-range = Absenkung
param-expander-threshold = Sprachschwelle
param-expander-hold = Haltezeit
param-expander-level = Öffnet ab
param-limiter-ceiling = Obergrenze
param-limiter-release = Rücklaufzeit
param-listen-volume = Lautstärke

## What the stages do, inside their rows
help-stage-echo =
    Entfernt aus dem Mikrofon, was die unten gewählten Lautsprecher abspielen, damit sich die
    Gegenseite eines Anrufs nicht selbst hört, wenn du keine Kopfhörer trägst. Sie passt sich an die
    Verzögerung und den Raum an, während die Lautsprecher spielen.

    Fügt 5 ms Latenz hinzu.
help-stage-denoise =
    Ein neuronales Netz (DeepFilterNet3) trennt die Stimme von Hintergrundgeräuschen wie Lüftern,
    Tastaturen oder Verkehr und entfernt die Geräusche. Ein leises Mikrofon hört es auf einen Pegel
    angehoben, mit dem es gut arbeitet.

    Fügt 30 ms Latenz hinzu, am meisten von allen Stufen, auch ausgeschaltet (warum: siehe Filter
    ganz oben).
help-stage-highpass =
    Entfernt alles unterhalb seiner Frequenz: Rumpeln, Trittschall, Stöße gegen den Tisch und das
    Ploppen von P- und B-Lauten. Die Stimme selbst liegt darüber. Er kommt zuerst, damit die
    Rauschunterdrückung das Rumpeln nicht für eine Stimme hält.
help-stage-eq =
    Formt den Klang: senkt die unteren Mitten um 250 Hz, die eine Stimme dumpf oder dröhnend klingen
    lassen, und hebt die Präsenz um 3,5 kHz, die Sprache klarer macht.
help-stage-deesser =
    Dämpft scharfe S- und Sch-Laute: Solange das Band oberhalb seiner Frequenz mehr vom Klang
    ausmacht als die Schwelle, senkt er dieses Band ab, um höchstens 12 dB. Der Rest der Stimme
    bleibt, wie er ist.

    Fügt 1 ms Latenz hinzu.
help-stage-agc =
    Bringt die Stimme auf einen gleichmäßigen Pegel, ob du leise oder laut sprichst, nah am Mikrofon
    oder weiter weg. Sie regelt über Sekunden nach (hoch um höchstens 6 dB pro Sekunde, runter
    schneller) und hält die Verstärkung in Sprechpausen, bei gleichmäßigem Rauschen wie Brummen und
    beim Tippen, damit sie diese nicht anhebt.
help-stage-compressor =
    Gleicht die Lautstärke innerhalb von Wörtern und Sätzen aus, wofür die Pegelregelung zu langsam
    ist: Er senkt laute Silben oberhalb der Schwelle ab und hebt alles um etwa so viel an, sodass
    die Stimme beim gleichen Pegel gleichmäßiger und voller klingt.
help-stage-expander =
    Senkt Sprechpausen ab, damit das verbliebene Rauschen (das die Pegelregelung angehoben hat)
    zwischen den Wörtern nicht zu hören ist. Er öffnet, sobald er Sprache hört, am Pegel oder daran,
    wie sicher sich die Rauschunterdrückung ist, und schließt nach der Haltezeit wieder.
help-stage-limiter =
    Hält die Spitzen unter der Obergrenze, damit Lachen, Husten oder ein lautes Wort nicht
    übersteuern. Er schaut 1 ms voraus, um vor einer Spitze abzusenken, statt sie abzuschneiden.

    Fügt 1 ms Latenz hinzu.

## What the parameters do, behind the info button
help-param-echo-protect =
    Wenn du in das hineinsprichst, was die Lautsprecher abspielen, senkt die Echounterdrückung deine
    Stimme mit dem Echo ab. Während du sprichst, bleibt so viel deiner Stimme erhalten, wie sie war,
    und die Rauschunterdrückung entfernt den Rest des Echos, vor allem Musik; während du schweigst,
    wirkt die Echounterdrückung voll. Höher erhält mehr von der Stimme, lässt aber mehr von dem
    durch, was die Lautsprecher darunter abspielen; 0 % überlässt es allein der Echounterdrückung.
help-param-denoise-limit =
    Wie weit sie das Rauschen absenken darf. 100 dB entfernt es ganz; etwa 20 bis 30 dB lassen etwas
    vom Hintergrund übrig, was natürlicher klingt und weniger von der Stimme kostet.
help-param-denoise-level =
    Der Pegel, mit dem die Rauschunterdrückung deine Stimme hört. Eine zu leise Stimme hält sie für
    Rauschen und schaltet sie stumm, deshalb hebt sie ein leises Mikrofon vorher auf diesen Pegel an
    und nimmt die Verstärkung danach wieder zurück: Was herauskommt, bleibt gleich laut. Höher
    erhält mehr von einer leisen Stimme, lässt aber in den Pausen mehr Rauschen durch.
help-param-denoise-max-gain =
    Wie weit sie ein leises Mikrofon für die Rauschunterdrückung anheben darf. 0 dB schaltet das
    Anheben aus.
help-param-highpass-freq =
    Alles unterhalb dieser Frequenz wird entfernt. Höher entfernt mehr Rumpeln, macht tiefe Stimmen
    aber dünner; 80 Hz ist für alle Stimmen sicher.
help-param-eq-low-mid =
    Die Verstärkung um 250 Hz. Unter 0 dB klingt die Stimme weniger dumpf und dröhnend, zu weit
    abgesenkt klingt sie dünn.
help-param-eq-presence =
    Die Verstärkung um die Präsenzfrequenz. Über 0 dB klingt Sprache klarer und näher, zu weit
    angehoben scharf.
help-param-eq-presence-freq = Die Mitte des Präsenzbands. Tiefer klingt wärmer, höher heller.
help-param-deesser-freq =
    Wo die S-Laute anfangen: Er senkt das Band oberhalb dieser Frequenz ab. Tiefer erfasst mehr
    davon, senkt aber auch mehr von der Stimme ab.
help-param-deesser-threshold =
    Der Anteil des Bands am gesamten Klang, ab dem er eingreift; 0 dB ist der ganze Klang. Niedriger
    greift öfter und stärker ein, höher nur bei den schärfsten S-Lauten.
help-param-agc-target =
    Der Pegel, auf den sie die Stimme im Mittel (RMS) bringt. −18 dBFS ist für Anrufe üblich und
    lässt Raum für die Spitzen. Der Kompressor folgt ihm.
help-param-agc-max-gain =
    Wie weit sie eine leise Stimme höchstens anhebt. Höher erreicht leisere Sprache oder ein weiter
    entferntes Mikrofon, hebt aber auch dessen Rauschen an.
help-param-agc-threshold =
    Wie sicher sich die Rauschunterdrückung sein muss, dass sie Sprache hört (ihr
    Signal-Rausch-Abstand), damit die Pegelregelung sie misst. Höher lässt mehr Rauschen außen vor,
    aber auch leisere Teile von Wörtern, sodass sie die Stimme weniger anhebt; −20 dB überlässt es
    allein dem Pegel, wie bei ausgeschalteter Rauschunterdrückung.
help-param-compressor-threshold =
    Oberhalb dieses Pegels senkt er ab. Niedriger gleicht mehr aus, auch normale Silben, höher nur
    die lautesten.
help-param-compressor-ratio =
    Wie stark er oberhalb der Schwelle absenkt: Bei 3:1 ergeben 3 dB mehr am Eingang 1 dB mehr am
    Ausgang. 1:1 tut nichts, höher gleicht mehr aus, klingt aber gepresst.
help-param-expander-range =
    Wie weit er die Pausen absenkt. Mehr macht sie leiser, den Wechsel zwischen Sprache und Pause
    aber hörbarer; 0 dB schaltet ihn aus.
help-param-expander-threshold =
    Wie sicher sich die Rauschunterdrückung sein muss, dass sie Sprache hört (ihr
    Signal-Rausch-Abstand), damit er öffnet. Niedriger öffnet öfter, auch bei Rauschen, höher nur
    bei klarer Sprache. Bei ausgeschalteter Rauschunterdrückung zählt nur der Pegel.
help-param-expander-hold =
    Wie lange er nach der letzten Sprache offen bleibt. Länger erhält Wortenden und kurze Pausen,
    kürzer macht die Pausen früher leise.
help-param-expander-level =
    Er öffnet auch, wenn der Klang lauter als dieser Pegel wird, für leise Sprache, bei der sich die
    Rauschunterdrückung nicht sicher ist. Niedriger öffnet öfter, auch bei Rauschen; Pausen liegen
    meist bei −55 bis −65 dBFS.
help-param-limiter-ceiling =
    Wie hoch die Spitzen höchstens gehen dürfen. −1 dBFS lässt Raum für das, was Codecs hinzufügen,
    wenn sie den Klang für den Anruf komprimieren.
help-param-limiter-release =
    Wie schnell er nach einer Spitze wieder loslässt. Kürzer klingt lauter, verzerrt aber früher.

## What the other rows show
help-filter =
    Schaltet den ganzen Filter. Ausgeschaltet bekommen Apps das Mikrofon, wie es ist, während jede
    Stufe ihren eigenen Schalter und ihre Einstellungen für das Wiedereinschalten behält. Das Symbol
    im Tray schaltet ihn ebenfalls.

    Die Latenz bleibt auch ausgeschaltet gleich: Das Mikrofon wird so weit verzögert, wie der Filter
    brauchen würde (etwa 37 ms, davon 30 ms für die Rauschunterdrückung; die Zeile Latenz weiter
    unten zeigt alles). Fiele die Verzögerung weg, ginge beim Ausschalten ein Stück Ton verloren und
    beim Einschalten entstünde eine Lücke, im Anruf als Knacken oder Stocken zu hören; so wird beim
    Umschalten nur überblendet.
help-listen-volume =
    Wie laut du dich selbst hörst. Nur fürs Mithören: Apps bekommen die Stimme mit dem Pegel, auf
    den der Filter sie bringt.
help-input-level =
    Wie laut das Mikrofon vor dem Filter ist; der Balken reicht von −60 bis 0 dBFS.

    Das Hintergrundrauschen ist sein leisester Moment in den letzten 2 Sekunden, also das, was es
    aufnimmt, während du schweigst. Unter −60 dBFS ist leise; über −45 dBFS tut sich die
    Rauschunterdrückung schwer und lässt ein Zischen übrig. Eine hochgedrehte Mikrofonverstärkung
    (in alsamixer oft „Mic Boost“ oder „Internal Mic Boost“) hebt das Rauschen an; ein Headset oder
    ein USB-Mikrofon nimmt meist viel weniger auf.
help-output-level =
    Wie laut die Stimme den Filter verlässt, also das, was Apps bekommen; der Balken reicht von −60
    bis 0 dBFS. Sprache sollte um den Zielpegel der Pegelregelung liegen (−18 dBFS), die Pausen weit
    darunter.
help-latency =
    Wie lange die Stimme vom Mikrofon bis zur App braucht: der Puffer des Mikrofons, der Puffer vor
    dem virtuellen Mikrofon, was die Stufen brauchen (die Rauschunterdrückung 30 ms) und der Puffer
    der App.
help-processing =
    Wie lange 10 ms Audio in den letzten 10 Sekunden durch die Stufen gebraucht haben: der
    Mittelwert, die Zeit, in der 95 % und 99 % davon blieben (um höchstens 2 % aufgerundet), und die
    längste. Alles zusammen, die Echounterdrückung und die Rauschunterdrückung einzeln (sie brauchen
    am meisten) und die übrigen Stufen zusammen. Die längste Zeit muss deutlich unter 10 ms bleiben,
    sonst gibt es Aussetzer; eine ausgeschaltete Stufe braucht fast nichts.

    PipeWire übergibt das Audio in Zyklen, die Samples pro Zyklus sind ihre Größe (48 Samples sind 1
    ms). Der Filter fordert 480 an, Apps, die eine geringere Latenz brauchen, können sie
    verkleinern.
help-dropouts =
    Seit dem Start des Filters.

    Stille: Audio, das nicht rechtzeitig fertig war; das virtuelle Mikrofon hat stattdessen Stille
    gespielt, eine Lücke. Ein wenig davon direkt nachdem eine App die Aufnahme beginnt ist normal,
    während sich der Puffer einpendelt.

    Verworfen: Audio, das weggeworfen wurde, um die Latenz wieder zu senken, etwa nachdem der Filter
    pausiert hat, ein Sprung.

    Fehler: 10 ms Audio, an denen eine Stufe gescheitert ist, durch Stille ersetzt.

## The window
window-title = Voice Filter
main-menu = Hauptmenü
more-information = Mehr Informationen
menu-reset = Filter _zurücksetzen …
reset-heading = Filter zurücksetzen?
reset-body =
    Schaltet alle Stufen ein und setzt ihre Einstellungen auf die Standardwerte. Mikrofon und
    Lautsprecher bleiben, wie sie sind.
reset-cancel = _Abbrechen
reset-confirm = _Zurücksetzen
missing-title = Voice Filter läuft nicht
missing-description = Starte ihn mit <tt>voice-filter run</tt>
missing-no-bus = Keine Verbindung zum Sitzungsbus: { $error }
missing-no-proxy = Der Proxy konnte nicht erstellt werden: { $error }
copy = Kopieren
copied = Kopiert
group-input = Eingang
group-input-description = Der Filter liest von diesem Mikrofon, Apps lesen von „Voice Filter“.
filter = Filter
filter-subtitle = Ausgeschaltet bekommen Apps das Mikrofon, wie es ist
microphone = Mikrofon
speakers = Lautsprecher
default-input = Standardeingang
default-output = Standardausgang
unplugged = { $device } (nicht angeschlossen)
listen = Mithören
listen-subtitle = Spielt es auf dem Standardausgang ab, mit Kopfhörern
listen-volume = Lautstärke
keep-listening = Mithören beibehalten
keep-listening-subtitle =
    Hört nach einem Neustart wieder mit. Das Mikrofon bleibt in Benutzung und der Filter läuft die
    ganze Zeit.
echo-status = Status
echo-not-in-chain = Nicht in der Kette
echo-not-running = Läuft nicht: ausgeschaltet, oder keine App nimmt von „Voice Filter“ auf
echo-silent = Die Lautsprecher sind still, es gibt kein Echo zu entfernen
echo-delay-short = weniger als 4 ms
echo-delay = etwa { $delay } ms
echo-hears = Hört die Lautsprecher mit { $level } dBFS, ihr Echo kommt nach { $delay } zurück
default-settings = Standardeinstellungen
reset = Zurücksetzen
group-levels = Pegel
level-input = Mikrofon
level-output = Voice Filter
input-not-running = Läuft nicht: keine App nimmt von „Voice Filter“ auf
input-noise = Hintergrundrauschen bei { $floor } dBFS
input-noise-loud =
    Hintergrundrauschen bei { $floor } dBFS, sehr laut: Die Rauschunterdrückung lässt ein Zischen
    übrig. Senke die Mikrofonverstärkung, falls sie hochgedreht ist, oder nimm ein Headset
group-status = Status
latency = Latenz
latency-value =
    { $total } ms bis zur App: Mikrofon { $microphone }, Puffer { $bridge }, Verarbeitung { $chain }
processing = Verarbeitungszeit
processing-mean = Mittel
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = Längste
processing-all = Alle Stufen
processing-other = Übrige Stufen
processing-summary = { $mean } ms pro 10-ms-Frame, längste { $max } ms
processing-nothing = In den letzten 10 Sekunden nichts verarbeitet
processing-cycle = { $samples } Samples pro Zyklus
processing-copied = Pro 10-ms-Frame in ms: { $columns }
dropouts = Aussetzer seit dem Start
dropouts-value =
    { $silence } ms Stille, { $dropped } ms verworfen, { $errors ->
        [one] 1 Fehler
       *[other] { $errors } Fehler
    }

## The icon in the tray
tray-title = Voice Filter
tray-on = Ein
tray-off = Aus: Das Mikrofon geht ungefiltert durch
tray-filter = Filter
tray-open = Voice Filter öffnen
