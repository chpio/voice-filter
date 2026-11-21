# Using Voice Filter

The details behind the short version in the [README](../README.md#usage): which apps get the
filter, what's kept between runs, the GNOME app, the tray and the offline mode. The control
interface is in [dbus.md](dbus.md).

**Contents:** [Which apps get the filter](#which-apps-get-the-filter) ·
[While it runs](#while-it-runs) · [Saved settings](#saved-settings) ·
[Switching stages and the filter](#switching-stages-and-the-filter) ·
[Echo cancellation over speakers](#echo-cancellation-over-speakers) ·
[The GNOME app](#the-gnome-app) · [The tray](#the-tray) · [Offline mode](#offline-mode)

## Which apps get the filter

```bash
# filters the default input for all apps, until Ctrl+C
voice-filter run
# filters the microphone <name> for all apps that record from it
voice-filter run --input <name>
# only for the apps that record from "Voice Filter"
voice-filter run --mode manual
```

**Automatic** (the default): every app that records from the microphone gets it filtered, nothing
to set up in the apps. Choose the microphone with `--input` or in the app, kept for the next runs:

- **Default Input** (the default): the filter reads from the microphone chosen in the system's
  sound settings and follows when you choose another one there or plug one in.
- **A particular microphone**: the apps that record from it get it filtered, also through the
  default input when it's chosen there; apps on another microphone get theirs unfiltered, and
  while it's unplugged all of them do.

WirePlumber links the filter in between as a [smart filter][smart-filters]; while
`voice-filter run` isn't running, the apps get the microphone directly.

**Manual** (`--mode manual`, and always without WirePlumber 0.5 or later): only the apps that
record from **Voice Filter** get the filtered microphone: select it as the microphone in the app
(or as the default input of the system). `--mode auto` goes back; switching sets up the streams
again, the sound drops for a moment.

In the manual mode Voice Filter stays after `voice-filter run` stopped, until PipeWire restarts
(e.g. at the next login): apps that record from it stay with it and get silence until
`voice-filter run` is back, rather than switching to the unfiltered microphone. To remove it, stop
`voice-filter run`, look up the ID of Voice Filter under _Sources_ in `wpctl status` and run
`pw-cli destroy <id>` (not `pw-cli destroy voice-filter`: that name also matches the client of
`voice-filter run`); switching to the automatic mode removes it too.

[smart-filters]: https://pipewire.pages.freedesktop.org/wireplumber/policies/smart_filters.html

## While it runs

The microphone is only in use while an app records from Voice Filter, and only then the chain
runs: while no app records, `voice-filter run` takes no CPU. If Voice Filter is chosen as the
default input in the system's sound settings, the filter reads from the source with the highest
priority and the app says so; choose your microphone there instead.

`--output` names the speakers whose echo gets removed, kept for the next runs; without one it's
the default output. `voice-filter inputs` and `voice-filter outputs` list the names, the chosen
one marked with `*`.

Only one `voice-filter run` can be active at a time. A second one stops with "Voice Filter is
already running".

`voice-filter run` exits with an error when it loses its connection to PipeWire, so a service
manager can start it again. `--verbose` prints the latency and the dropouts every 10 s.

## Saved settings

The microphone, the speakers, the mode, the switches and the settings of the stages are kept for
the next runs, in `~/.local/state/voice-filter/` (`$XDG_STATE_HOME`); `chain` there lists what
differs from the defaults, one setting per line (`denoise off`, `denoise.limit 30`), delete it to
go back to them.

## Switching stages and the filter

Every stage can be switched off while it runs. A stage that is off is skipped (it uses no CPU) and
its input plays instead, delayed by the stage's latency, so the total latency never changes.
Switching crossfades over one frame, so there are no clicks. The echo cancellation takes a few
seconds after being switched on until it removes the echo fully again, as it missed what the
speakers played meanwhile.

The whole filter switches off the same way, all stages at once, while each stage keeps its own
switch for when the filter is on again; kept for the next runs too. Switched off, it still delays
the microphone by its latency (37 ms, 30 ms of it the noise suppression): dropping the delay would
cut a piece out of the sound when switching off and insert a gap when switching on, heard as a
click or a stutter in a call, while at the same latency switching just crossfades.

## Echo cancellation over speakers

With headphones there's no echo, switch the echo cancellation off. Over speakers it removes the
echo; what's left of it it turns down band by band, and the voice with it while you talk into the
playback, less while it hears you talk (tuned for that). **Voice Protection** (off, 0 %) keeps a
part of your voice as it was while you talk and leaves the rest of the echo to the noise
suppression, which removes music by itself; while you're silent the echo cancellation acts fully.
Higher keeps more of the voice but lets more of the playback through under it.

## The GNOME app

`voice-filter ui` controls a running `voice-filter run`. It's a separate process, so closing it
never interrupts the audio. `voice-filter install-desktop` adds it to the apps of the desktop, with
its icon (it writes `~/.local/share/applications/net.b128.VoiceFilter.desktop`,
`~/.local/share/icons/hicolor/scalable/apps/net.b128.VoiceFilter.svg` and the two icons of the tray
in `~/.local/share/icons/hicolor/symbolic/apps/`; delete them to remove it). In the app you can:

- switch the whole **filter** off and on: off, apps get the microphone as it is, still at the
  same latency (so switching doesn't click or stutter), and every stage keeps its own switch and
  settings
- choose the **microphone**: the default input (it shows which one that is) or a particular one,
  switched while the filter keeps running
- choose the **speakers** whose echo the echo cancellation removes (in its row); with headphones
  there's no echo, it can be off. Its status shows whether it hears the speakers, how loud, and the
  delay of their echo
- turn on **Listen** to hear yourself through the filter on the default output (use headphones),
  at a volume of its own; **Keep Listening** turns it on again after a restart, e.g. to keep
  listening quietly (the microphone stays in use then)
- switch **each stage** on and off and adjust its **settings**, e.g. the limit of the noise
  suppression (lower values keep more of the original voice, 100 dB removes all the noise it can)
  or the frequency of the high-pass; a stage's settings go back to their defaults with its
  **Reset** button, everything with **Reset the Filter…** in the main menu
- watch the **levels** of the microphone and of Voice Filter, and the background noise of the
  microphone: above −45 dBFS the app points out that it's too noisy
- watch the **latency**, the processing time per frame (mean, ~95 %, ~99 % and the longest, of
  all stages, the echo cancellation and the noise suppression on their own and the other stages
  together) and dropouts

The info button at the end of a row explains what the setting or the status value means; an
expanding row (a stage, the processing time) explains itself as the first line inside. The status
values (latency, processing time, dropouts, the echo cancellation's status and the background noise)
have a copy button and can be selected, e.g. for a bug report.

The app and the icon in the tray speak the language of the desktop (`LANG`, `LANGUAGE`), English
where there's no translation; translations are in `i18n/`, one Fluent file per language.

## The tray

`voice-filter run` also shows an icon in the tray: a click opens the app, its menu switches the
filter, the echo cancellation (no echo with headphones) and Listen, and the icon shows whether it's
on (the microphone between noise and a clean wave, as on the app icon, struck through while it's
off). KDE and most other desktops show it as they are; GNOME Shell needs the extension “AppIndicator
and KStatusNotifierItem Support” (on Arch `gnome-shell-extension-appindicator`).

## Offline mode

```bash
voice-filter process in.wav out.wav
# with what the speakers played, for the echo cancellation
voice-filter process in.wav out.wav --reference played.wav
```

Runs a WAV file (48 kHz, mixed down to mono) through the same chain and writes the result, lined
up with the input. It prints the latency and the processing time of every stage. Use it to compare
settings, e.g. `--atten-lim 12` for a limit of 12 dB; it takes the defaults, not the settings kept
for `run` (`run --atten-lim` sets the limit and keeps it, like the app). For the echo
cancellation, record the microphone and the monitor of the speakers at the same time, e.g.:

```bash
pw-record --target <microphone> --rate 48000 --channels 1 in.wav &
pw-record --target <speakers> -P '{ stream.capture.sink = true }' \
    --rate 48000 --channels 1 played.wav &
```
