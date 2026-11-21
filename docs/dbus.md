# D-Bus interface

The control interface of `voice-filter run`, which the GNOME app uses too; what the settings do is
in [usage.md](usage.md) and in the stage table of the [README](../README.md#how-it-works).

The daemon serves `net.b128.VoiceFilter.Daemon` on the session bus (object `/net/b128/VoiceFilter`,
interface `net.b128.VoiceFilter1`):

| Member            | Type                 | Description                                                                                                         |
| ----------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `Mode`            | `s`, read/write      | `auto` (all apps on the microphone) or `manual` (only apps on Voice Filter), kept for the next runs                 |
| `AutoAvailable`   | `b`, read            | whether the session manager has smart filters (WirePlumber 0.5 or later), which `auto` needs                        |
| `Input`           | `s`, read/write      | the microphone's `node.name`, empty for the default input                                                           |
| `ActiveInput`     | `s`, read            | the `node.name` of the source the filter reads from, empty while no app records                                     |
| `DefaultInput`    | `s`, read            | the `node.name` of the system's default input                                                                       |
| `Output`          | `s`, read/write      | the speakers' `node.name` (for the echo), empty for the default output                                              |
| `Listen`          | `b`, read/write      | plays Voice Filter on the default output                                                                            |
| `KeepListen`      | `b`, read/write      | keeps `Listen` for the next runs (off by default)                                                                   |
| `ListenVolume`    | `d`, read/write      | the volume of `Listen` in dB, −40 to 0, kept for the next runs                                                      |
| `Enabled`         | `b`, read/write      | the whole filter on or off (off, the microphone passes as it is), kept for the next runs                            |
| `SetStageEnabled` | method `(sb)`        | switches a stage on or off by name, kept for the next runs                                                          |
| `Parameters`      | `a(ssssddddd)`, read | the settings: stage, name, title, unit, min, max, step, default, value                                              |
| `SetParameter`    | method `(ssd)`       | sets a setting of a stage (stage, name, value), within its range, kept for the next runs                            |
| `Stages`          | `a(sb)`, read        | the stages in order and whether they're on                                                                          |
| `Inputs`          | `a(ssi)`, read       | the microphones: name, description, priority                                                                        |
| `Outputs`         | `a(ssi)`, read       | the output devices, the same way                                                                                    |
| `Echo`            | `a{sd}`, read        | the echo cancellation: frame counters, level of the speakers, AEC3's ERLE/ERL (of its linear filter only) and delay |
| `Latency`         | `a{sd}`, read        | `total`, `microphone`, `bridge`, `chain` and `app`, in ms                                                           |
| `Quantum`         | `t`, read            | samples per PipeWire cycle                                                                                          |
| `MaxFrameTime`    | `d`, read            | longest processing time of a frame over the last 10 s, in ms                                                        |
| `FrameTimes`      | `a(sdddd)`, read     | processing time per frame over the last 10 s in ms: part, mean, 95 % and 99 % percentile, longest                   |
| `Levels`          | `a{sd}`, read        | `input` and `output` (dBFS, falling 20 dB/s) and `input-floor`, the noise floor of the microphone                   |
| `Counters`        | `a{st}`, read        | `frames`, `silence`, `dropped` and `errors` since the start                                                         |

For example with `busctl`:

```bash
vf="net.b128.VoiceFilter.Daemon /net/b128/VoiceFilter net.b128.VoiceFilter1"
busctl --user introspect $vf
busctl --user get-property $vf Parameters
busctl --user call $vf SetParameter ssd denoise limit 12
busctl --user call $vf SetStageEnabled sb agc false
busctl --user set-property $vf Enabled b false
```

The interface may still change before 1.0.
