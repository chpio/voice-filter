# Konzept: Live-Sprachfilter für Linux

Stand: 31.10.2026 · Kopie des Dokuments [Konzept: Live-Sprachfilter für Linux](https://claude.ai/artifact/2tXoWyCXGXVbrA8qy7i9kN); das Original ist maßgeblich.

## Ziel und Anforderungen

Ein Live-Sprachfilter für Linux, der als virtuelles Mikrofon in PipeWire erscheint. Jede App (Discord, Browser, OBS) nimmt ihn wie ein normales Mikrofon.

- **Live:** Gesamtlatenz möglichst unter 40 ms, damit Gespräche natürlich bleiben.
- **Funktionen:** Echo entfernen, Rauschen entfernen, fremde Stimmen optional unterdrücken, Stimme verbessern.
- **Hardware:** läuft vollständig auf der CPU. GPU nur als optionale Beschleunigung für schwere Zusatzstufen.
- **Plattform:** Arch Linux, GNOME unter Wayland, PipeWire.
- **Bedienung:** Stufen und Modi zur Laufzeit umschaltbar, ohne Knacken.

## Signalkette

Das Signal läuft in fester Reihenfolge durch alle Stufen. Echo kommt zuerst, weil die übrigen Stufen das Echo sonst als Sprache behandeln.

1. **Mikrofon** – Hardware-Eingang, 48 kHz mono
2. **Echo entfernen** – WebRTC AEC3 über das PipeWire-Modul
3. **Rauschen entfernen** – DeepFilterNet3, 48 kHz, ab 20 ms Latenz (Hauptfilter)
4. **Fremde Stimmen unterdrücken** (optional) – Hush, Modi Aus · Full · Gate, rund 11 ms mehr
5. **Stimme verbessern** – Hochpass, EQ, De-Esser, Kompressor, AGC, Limiter
6. **Restaurierung** (optional, später) – ML-Modell wie Stream.FM, GPU nötig
7. **Virtuelles Mikrofon „Voice Filter“** – Discord, Browser oder OBS wählen es aus

Die Stimmverbesserung sitzt hinter dem Denoising, damit der Kompressor kein Rauschen hochzieht.

## Modellwahl

DeepFilterNet3 ist der Hauptfilter. Es ist die stärkste offene Live-Option mit vollem Frequenzumfang. Hush ergänzt es nur für fremde Stimmen. Einen unabhängigen Vergleich aller Modelle für 2026 gibt es nicht; die Hush-Platzierung im Hugging-Face-Leaderboard stammt vom Hersteller.

| Modell                                                      | Abtastrate | Stärke                                                      | Schwäche                                | Rolle                          |
| ----------------------------------------------------------- | ---------- | ----------------------------------------------------------- | --------------------------------------- | ------------------------------ |
| [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) | 48 kHz     | Beste Live-Qualität, Kern in Rust, LADSPA-Plugin vorhanden  | Mindestens 20 ms Latenz, etwas mehr CPU | Hauptfilter                    |
| [Hush](https://huggingface.co/Aliados/hush)                 | 16 kHz     | Unterdrückt auch fremde Stimmen, 8 MB, unter 1 ms pro Frame | Klingt nach Telefon, junges Projekt     | Optional                       |
| [GTCRN](https://github.com/sherryyu33/gtcrn)                | 16 kHz     | Extrem klein, 23,7 K Parameter                              | Schmalbandig                            | Fallback für schwache Hardware |
| RNNoise                                                     | 48 kHz     | Minimaler CPU-Bedarf                                        | Deutlich schlechtere Qualität           | Nicht vorgesehen               |

Für Hush gibt es eine reine Rust-Implementierung ([hush-vani](https://crates.io/crates/hush-vani)) mit eingebetteten Gewichten, ohne ONNX Runtime. Sie stammt von einem Dritten, nicht vom Hersteller.

## Fremde Stimmen unterdrücken

Die Funktion ist zur Laufzeit aktivierbar und bietet zwei Modi. Hush kostet keine Lizenzgebühren (Apache 2.0); die Kosten sind Latenz, Klang und Wartung.

| Modus | Wie es arbeitet                                                                    | Vorteil                                                  | Nachteil                                                         |
| ----- | ---------------------------------------------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------- |
| Aus   | Hush läuft nicht                                                                   | Keine CPU-Last                                           | Keine Unterdrückung                                              |
| Full  | Hush-Ausgabe ersetzt das Signal                                                    | Stärkste Unterdrückung, auch bei gleichzeitigem Sprechen | Alles über 8 kHz fehlt, klingt nach Telefon                      |
| Gate  | Hush läuft parallel und liefert pro 10 ms nur eine Verstärkung für den 48-kHz-Pfad | Voller Klang                                             | Fremde Stimmen vor allem in Sprechpausen entfernt; experimentell |

**Kosten im Detail:**

- **CPU:** unter 1 ms pro 10-ms-Frame, kaum spürbar neben DeepFilterNet3.
- **Latenz:** rund 11 ms zusätzlich (Hush 10 ms plus Umrechnung 48 auf 16 kHz und zurück). Alle Modi bekommen dieselbe Latenz, damit Umschalten per Überblendung knackfrei bleibt.
- **Entwicklung:** feste 3:1-Umrechnung der Abtastrate, Überblendung beim Umschalten, Glättung der Gate-Verstärkung (schnell öffnen, langsam schließen).
- **Wartung:** Projekt eines Unternehmens mit anderem Kerngeschäft, Version 2 angekündigt. Modellformat kann sich ändern.

Wer die Funktion nie braucht, lässt die Stufe ganz weg und spart die 11 ms.

## Stimmverbesserung

Klassisches DSP bringt den größten hörbaren Gewinn und läuft live praktisch ohne CPU-Last. ML-Restaurierung ist eine spätere, optionale Stufe.

**Ebene 1: klassisches DSP (Teil der ersten Version)**

1. Hochpass um 80 Hz gegen Rumpeln, Trittschall und Plosive
2. EQ: leichte Absenkung um 250 Hz gegen Mulm, Anhebung um 3,5 kHz für Präsenz
3. De-Esser gegen scharfe S-Laute
4. Kompressor für gleichmäßige Lautstärke
5. Automatische Pegelanpassung (AGC)
6. Limiter gegen Übersteuern

Die Werte sind Startpunkte, später als Presets einstellbar.

**Ebene 2: ML-Restaurierung („Studio-Stimme“)**

Ziel: Hall entfernen, fehlende Höhen rekonstruieren, Kompressionsartefakte reparieren. Live und offen gibt es kaum Reifes. Bester Kandidat ist [Stream.FM](https://arxiv.org/html/2512.19442v1) (Forschungsarbeit, Ende 2025):

- Entfernt Rauschen und Hall, ergänzt Höhen, repariert Codec-Artefakte.
- Latenz 24 bis 48 ms.
- Echtzeit auf einer RTX 4080; auf einem Ryzen 7 9800X3D nur knapp (Faktor 0,88). Auf einem Laptop-Prozessor eher nicht.
- Code laut Paper offen; Gewichte und Lizenz noch unklar.

## GPU-Strategie

Die Basis läuft nur auf der CPU. Eine GPU lohnt sich erst für schwere Modelle wie die ML-Restaurierung.

- **Warum nicht für alles:** Live-Audio kommt in 10-ms-Frames. Der Transfer zur GPU kostet bei so kleinen Modellen mehr als die Rechnung selbst.
- **Wo sinnvoll:** Ebene-2-Restaurierung (Stream.FM o. ä.), die auf der CPU nicht in Echtzeit läuft.
- **AMD:** über ONNX Runtime mit ROCm bzw. MIGraphX, oder Rust-nativ über `burn` mit `wgpu`-Backend. Ob Stream.FM darauf läuft, muss geprüft werden.
- **Absicherung:** Ist keine passende GPU da, wird die Stufe deaktiviert. Der Rest der Kette läuft unverändert.

## Architektur

Ein einzelner Rust-Prozess als PipeWire-Node. Alle Stufen laufen darin, damit die Latenz kontrollierbar bleibt.

- **Sprache:** Rust. DeepFilterNet3 und hush-vani sind bereits Rust, PipeWire über `pipewire-rs`.
- **PipeWire:** liest vom Mikrofon (oder von der Echo-Cancel-Quelle) und stellt „Voice Filter“ als virtuelles Mikrofon bereit.
- **Frames:** intern 48 kHz, 10 ms pro Frame (480 Samples). Passt exakt zu DeepFilterNet3 (Hop 480) und Hush (Hop 160 bei 16 kHz).
- **Stufen:** gemeinsames Trait mit Name, Latenz und Verarbeitung eines Frames. Stufen sind austauschbar und weglassbar.
- **Umrechnung 48 ↔ 16 kHz:** festes Filter mit Faktor 3. Bekannte, konstante Latenz, keine Allokation im Audio-Pfad.
- **Steuerung:** Einstellungen lock-frei geteilt, Änderungen über einen lokalen Socket, z. B. `voice-filter ctl speaker gate`. Später eine GNOME-App darauf.
- **Testmodus:** dieselbe Kette verarbeitet WAV-Dateien offline. So lassen sich Modelle und Einstellungen vergleichen und die Echtzeitfähigkeit messen.

## Risiken und offene Fragen

Das größte Risiko ist die Latenz: Alle Stufen zusammen müssen live bleiben.

| Risiko                                                             | Folge                             | Gegenmaßnahme                                                    |
| ------------------------------------------------------------------ | --------------------------------- | ---------------------------------------------------------------- |
| Latenz summiert sich (DFN3 ab 20 ms, Hush +11 ms, PipeWire-Puffer) | Gespräche wirken verzögert        | Latenz pro Stufe messen und anzeigen; Stufen weglassbar          |
| Gate-Modus schneidet leise eigene Sprache ab                       | Silben fehlen                     | Schnell öffnen, langsam schließen; Untergrenze einstellbar       |
| hush-vani ist ein Dritt-Projekt                                    | Wartung unsicher                  | Hinter austauschbarer Schnittstelle kapseln; ONNX-Weg als Plan B |
| DeepFilterNet3 auf crates.io veraltet                              | Echtzeit-Teil nur aus Git nutzbar | Feste Git-Revision einbinden                                     |
| Stream.FM: Gewichte, Lizenz, ROCm unklar                           | Ebene 2 eventuell nicht umsetzbar | Erst prüfen, dann entscheiden                                    |

**Offene Fragen:**

- [ ] Welche Gesamtlatenz ist für dich noch akzeptabel?
- [ ] Echo-Unterdrückung über das PipeWire-Modul oder direkt im eigenen Prozess?
- [ ] Steuerung: nur Kommandozeile, GNOME-App, oder beides?
- [ ] Presets für Stimmverbesserung (z. B. „Call“, „Stream“, „Podcast“)?
- [ ] Soll die Kette auch eingehendes Audio filtern (Lautsprecher-Seite)?

## Roadmap

Vier Phasen, jede für sich nutzbar. Jede Phase endet mit einer Latenz- und Hörprobe.

1. **Basis:** PipeWire-Node, DeepFilterNet3, Offline-Modus für WAV-Tests, Latenzmessung.
2. **Stimme:** DSP-Kette (Hochpass, EQ, De-Esser, Kompressor, AGC, Limiter), Presets, Laufzeitsteuerung.
3. **Echo und fremde Stimmen:** Echo-Unterdrückung einbinden, Hush mit Modi Full und Gate, knackfreies Umschalten.
4. **Restaurierung:** Stream.FM o. ä. prüfen, GPU-Backend für AMD, optional als Stufe.

Danach: grafische Oberfläche für GNOME und ein Paket für Arch (AUR).
