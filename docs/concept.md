# Concept: Live Voice Filter for Linux

Started as the German document [Konzept: Live-Sprachfilter für Linux](https://claude.ai/artifact/2tXoWyCXGXVbrA8qy7i9kN) (as of 2026-10-31); this file is now the authoritative version, the original isn't updated anymore. How to use it is in the [README](../README.md), the measurements behind the defaults of the noise suppression and the order of the chain are in [measurements](measurements.md).

**Status** (2026-11-01): phases 1 and 2 are done, most of phase 3. The live filter runs as the virtual microphone "Voice Filter" with high-pass, echo cancellation, DeepFilterNet3, EQ, de-esser, AGC, compressor, expander and limiter, controlled from a GNOME app over D-Bus, with an icon in the tray that switches the whole filter. Missing from phase 3: listening tests of the voice protection while talking into the playback (see [Double talk](#double-talk-the-echo-cancellation-only-decides), in the chain, measured offline) and more tests. See the [roadmap](#roadmap).

**Contents:** [Goal and requirements](#goal-and-requirements) · [Signal chain](#signal-chain) · [Model choice](#model-choice) · [Echo cancellation](#echo-cancellation) · [Suppressing other voices](#suppressing-other-voices) · [Voice enhancement](#voice-enhancement) · [GPU strategy](#gpu-strategy) · [Architecture](#architecture) · [Risks and open questions](#risks-and-open-questions) · [Roadmap](#roadmap)

## Goal and requirements

A live voice filter for Linux that shows up in PipeWire as a virtual microphone. Any app (Discord, browser, OBS) uses it like a regular microphone.

- **Live:** total latency below 40 ms if possible, so conversations stay natural.
- **Features:** remove echo, remove noise, enhance the voice; optionally suppress other voices (postponed, see [Suppressing other voices](#suppressing-other-voices)).
- **Hardware:** runs entirely on the CPU, with little load (the whole chain about 7 % of a core live while you talk, see `docs/measurements.md`, "Live CPU load"); no GPU.
- **Platform:** Arch Linux, GNOME on Wayland, PipeWire.
- **Operation:** stages and modes can be switched at runtime, without clicks.

## Signal chain

The signal passes through all stages in a fixed order. Echo comes first after the high-pass, because otherwise the other stages treat the echo as speech.

1. **Microphone** – hardware input, 48 kHz mono
2. **Remove rumble** – the high-pass of the voice enhancement (80 Hz), first: below it there's only noise, and DeepFilterNet3 took the rumble for speech (see [Model choice](#model-choice))
3. **Remove echo** – WebRTC AEC3 in our own process, with the speakers' monitor as the reference (see [Echo cancellation](#echo-cancellation))
4. **Remove noise** – DeepFilterNet3, 48 kHz, 30 ms latency (main filter), hearing a quiet voice raised to −35 dBFS
5. **Suppress other voices** (optional, postponed) – Hush, modes Off · Full · Gate, about 11 ms more
6. **Enhance the voice** – EQ, de-esser, AGC, compressor, expander, limiter
7. **Virtual microphone "Voice Filter"** – selected in Discord, the browser or OBS

Voice enhancement comes after denoising, so the compressor doesn't bring up the noise.

## Model choice

DeepFilterNet3 is the main filter. It's the strongest open live option with the full frequency range. Hush was meant to complement it for other voices, but is postponed after a first test (see [Suppressing other voices](#suppressing-other-voices)). There's no independent comparison of all models for 2026; Hush's placement on the Hugging Face leaderboard comes from its maker.

| Model                                                       | Sample rate | Strength                                                 | Weakness                           | Role                       |
| ----------------------------------------------------------- | ----------- | -------------------------------------------------------- | ---------------------------------- | -------------------------- |
| [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) | 48 kHz      | Best live quality, core in Rust, LADSPA plugin available | 30 ms latency, somewhat more CPU   | Main filter                |
| [Hush](https://huggingface.co/Aliados/hush)                 | 16 kHz      | Also suppresses other voices, 8 MB, under 1 ms per frame | Sounds like a phone, young project | Postponed                  |
| [GTCRN](https://github.com/sherryyu33/gtcrn)                | 16 kHz      | Extremely small, 23.7 K parameters                       | Narrowband                         | Fallback for weak hardware |
| RNNoise                                                     | 48 kHz      | Minimal CPU usage                                        | Clearly worse quality              | Not planned                |

There's a pure Rust implementation of Hush ([hush-vani](https://crates.io/crates/hush-vani)) with embedded weights, without ONNX Runtime. It comes from a third party, not from the maker.

**DeepFilterNet3 vendored as `deep-filter-rt`:** the Rust crate of DeepFilterNet (`deep_filter`, folder `libDF`) is only usable from git: the last release on crates.io (0.2.5, July 2022) has neither DeepFilterNet3 nor the real-time inference, and the upstream repo has been dormant since September 2024. crates.io doesn't accept packages with git dependencies, so voice-filter couldn't be published that way. Instead we vendor it as the workspace crate `crates/deep-filter-rt` and publish it ourselves:

- Copied without its history from upstream commit `d375b2d` (2024-10-17): `libDF` and the DeepFilterNet3 model, into `crates/deep-filter-rt`; its `NOTICE.md` names the origin. The upstream history stays available on GitHub; taking it over would have brought about 470 foreign, unsigned commits without conventional commit prefixes and an older model version (8 MB) into this repo, while the code gets reworked heavily anyway. In the workspace a change of its API and the caller go into one commit; it can still be split off into a repo of its own later.
- The model is stored unpacked in `models/DeepFilterNet3/` (three ONNX files and `config.ini`) instead of upstream's `DeepFilterNet3_onnx.tar.gz`: the weights barely compress (gzip saves 7 %, zstd, xz or brotli at most 1 % more), git and crates.io compress anyway, `config.ini` stays readable in diffs and the crates for unpacking the archive can go. Not in Git LFS: the files hardly ever change, and LFS would break builds from clones without git-lfs (`include_bytes!` would embed the pointer files) and every mirror would need LFS.
- Reduced to the inference: the core (`lib.rs`) and the `tract` inference (`tract.rs`), about 1,700 of upstream's 7,900 lines and 10 of its 28 dependencies. Updated from tract 0.21 to 0.23, the output differs from upstream's by at most 1.6e-7 (−114 dB), i.e. rounding.
- The model (8.6 MB) stays embedded and fits the 10 MB limit of crates.io. The low-latency variant (36 MB) doesn't, and isn't used (see the decisions under [Risks and open questions](#risks-and-open-questions)).

**DeepFilterNet3 on recordings** (2026-11-01, raw recordings of a USB microphone and of a laptop's built-in one, speech, a minute of silence, speech, run through the chain with `examples/trace.rs`):

- **Zero mask:** where the model estimates a local SNR below −10 dB (`min_db_thresh`, upstream's default), `DfTract` applies zeros instead of its gains, the frame comes out as exact silence. In the pauses of the USB microphone that took 10–12 s after the speech. Before, the model let part of the noise through, the AGC's gain held from the speech (+27 to +30 dB) and the compressor's make-up raised it above the microphone's own level (median −51 and −55 dBFS against −65 and −67), and the estimate above 0 dB in 8 % of the frames (up to 30 % in a second) kept opening the expander: heard live as strongly raised noise that went silent after a while.
- **Skipping:** upstream's real-time defaults also skip the decoders where the estimate is high, to save CPU: above 30 dB (`max_db_erb_thresh`) the frame passes unchanged, above 20 dB (`max_db_df_thresh`) without the deep filter. On the VoiceBank-DEMAND test set (824 sentences, the noise suppression alone, a fresh state per sentence) that cost most of the model's quality: PESQ (wideband) 2.07 instead of 2.70, STOI 0.861 (below the noisy input's 0.921) instead of 0.925, SI-SDR 9.0 instead of 18.7 dB. Without the zero mask too: 3.16, 0.945 and 18.9 dB, upstream's published PESQ (3.17). `Denoise` runs the decoders on every frame (about 0.05 ms more per frame); whether the zero mask stays is open (see [Risks and open questions](#risks-and-open-questions)). Details in [measurements](measurements.md#skipping-frames-by-the-estimated-snr).
- **It depends on the level:** the complex features are divided by the square root of their mean magnitude (`band_unit_norm`), so they grow with the level, and the model judges by it. The built-in microphone at a low volume (speech at −58 dBFS) was muted all the time, speech included; 20 dB more and only a quarter of its speech frames were. The USB microphone 20 dB quieter (speech at −62 dBFS): 99 % muted. Louder input is muted less in the pauses too (the USB microphone 10 dB louder: 5 % of the pause instead of 71 %), so the noise stays. Speech should reach it at about −45 to −35 dBFS.
- **Rumble and the input level:** on a call over headphones (USB microphone at 30–40 cm, echo cancellation off, a fresh analysis at 10 ms) 54 % of the frames of the pauses came out above −60 dBFS, up to −23 dBFS, while with the echo cancellation on only 3.5 %. The microphone's noise lies mostly below 100 Hz (20–50 Hz the most), AEC3 filters that out (sonora forces WebRTC's high-pass with it), and with it DeepFilterNet3 estimated more than 0 dB in 43 % of the pause frames instead of 3 %, the AGC raised it by about 23 dB and the expander opened. Without the rumble the model hears the voice quieter too: with the high-pass first, or the echo cancellation on, it muted 12–13 % of the speech frames the raw microphone keeps (speech at about −43 dBFS; listened to: two spots were speech, one a noise, something moved, which the raw microphone let through at −20 dBFS); 10 dB more before it kept them. So the high-pass comes first in the chain, and `Denoise` raises the input of the model so it hears the speech at −35 dBFS (measured by its level like the AGC, at most 20 dB, never lowering) and takes the gain off the output again, so the stages after it get the level as before: 5.6 % of the pause frames above −60 dBFS, 2.3 % of the speech frames muted, with the echo cancellation on 4.0 % and 2.6 %; the built-in microphone along on the same call 58 % → 1.5 %, the other recordings about as before. On the VoiceBank-DEMAND test set (studio speech, the input gain doesn't act at its −22 dBFS) the high-pass first costs a little: PESQ 2.65 instead of 2.70 (better at a low SNR, 2.15 instead of 1.81, worse at a high one, 3.13 instead of 3.53), without the zero mask 2.91 instead of 3.14; at 40 Hz it was 2.75, but the call's pauses stayed as noisy (its rumble reaches up to 80 Hz), a frequency in between is still to try. Heard live afterwards: much less noise in the pauses, on the built-in microphone too. Details in [measurements](measurements.md#high-pass-and-input-level).
- **Tones:** the built-in microphone sits close to the laptop's fans, which add tones at 1.3–1.7 kHz, 20–30 dB above the noise around them, in pairs (two fans). After about 10 s of a pause the estimate rose from −12 to about 0 dB, the model stopped muting and let them through, at −43 dBFS after the AGC and compressor; a steady tone looks like a held vowel. That was before the high-pass moved first and `denoise` raised its input: with the chain now the estimate stays at about −14 dB through the minute of silence, and taking the tones out (notches at every one) changes neither it nor the noise left in the pauses, which is broadband (see [measurements](measurements.md#steady-tones)).
- **The expander against it** (simulated on the frames): speech threshold 6 dB and opening above −40 dBFS kept the pauses of the built-in microphone at about −63 dBFS instead of −44 (closed 62 % of the time instead of 9 %) and those of the USB microphone silent, while 2–5 % of the quieter speech frames were turned down (1.6–3.1 % with the defaults), the loud ones none. With −45 dBFS the fan noise still opened it. Not the default yet: to be checked on more recordings and on clean speech.

## Echo cancellation

Echo cancellation needs the signal the speakers are playing as a reference: an adaptive filter learns how the room changes the sound on its way from the speakers to the microphone, predicts the echo from the reference and subtracts it; a second step suppresses the residual echo. With headphones there's no echo, the stage can stay off.

**In our own process** (stage `echo`). Besides the microphone, voice-filter records the monitor of an output device, chosen in the app like the microphone (the default output by default), as the reference, and runs AEC3 as the first stage of the chain. It works on 10 ms frames at 48 kHz, our frames. Against PipeWire's `libpipewire-module-echo-cancel` (the same WebRTC AEC3): the speakers are chosen in the app and switched at runtime, nothing to configure and no playback to reroute (the module either creates a sink the playback has to go through, or with `monitor.mode` listens to the default output only, and changing it means editing its config and restarting PipeWire), no extra nodes with their own buffers, and the stage switches, has parameters and status like the others. The cost is more code (a second capture stream with a bridge into the audio path, the choice of the output) and the clocks of microphone and speakers, which drift against each other (AEC3 keeps estimating the delay). The reference stream runs on the output device's clock, in no group with the other streams (see the live test).

AEC3 comes from [sonora](https://github.com/dignifiedquire/sonora) (BSD-3-Clause), a pure Rust port of WebRTC's audio processing (M145 plus later fixes, passes the C++ test suite), so no C++ library. Only its echo canceller runs, its noise suppression and AGC2 stay off. The alternatives: [webrtc-audio-processing](https://github.com/tonarino/webrtc-audio-processing) wraps the C++ library (system package or built with meson); [aec3-rs](https://github.com/RubyBit/aec3-rs) is a less verified port with an API still changing; [DTLN-aec](https://github.com/breizhn/DTLN-aec) is a neural model at 16 kHz.

**Spike** (2026-10-31, sonora 0.2.0, AEC3 only, a recording of the USB microphone and of the speakers' monitor, playback for 24 s with the speaker talking into it from 7 s, then the speaker alone):

- Echo alone: 28–34 dB removed. The speaker alone, without playback: unchanged.
- Talking into the playback: the level of the output no longer follows the playback (correlation 0.04, the microphone 0.37). But DeepFilterNet3 alone did as well in this recording (0.00); in an earlier test it let speech from a phone through, so a test with only speech played back is still to do.
- 60 µs per frame on average, at most 0.6 ms; flushing denormals changed nothing here. About 40 allocations per frame, gone since sonora 0.3.0 (its PR #40).
**Live test** (2026-10-31, a video with speech over the speakers, nobody talking, the echo cancellation switched every 15 s, only it and the tone stages on, recorded from "Voice Filter"):

- With it −72 to −76 dBFS, without it −38 to −49 dBFS: about 30 dB of the echo removed. Switched on again it took about 4 s to remove it fully: a stage that's off pauses and misses what the speakers play meanwhile.
- In the first version the reference stream was in the `node.group` and `node.link-group` of the other streams. PipeWire puts the nodes of both on one driver, so the speakers ran under the USB microphone's clock and its quantum of 256 while an app recorded: xruns at every join and leave (511 at the output, 3134 at Firefox's playback, which wants 900–3600), heard as dropouts and a choppy video. Without them, the reference stream follows the speakers' clock and hands over through its ring.
- Talking into the playback, the voice gets choppy now and then: AEC3's suppressor turns down what's left of the echo frequency by frequency, and the voice with it, the more the louder the echo is against the voice (quiet here, about −36 dBFS at the microphone). AEC3's configuration could make it gentler, which sonora's builder takes since 0.3.0 (`echo_canceller3_config`, not tried yet, see `TODO.md`); the plan is now to let DeepFilterNet3 shape the voice instead, see [Double talk](#double-talk-the-echo-cancellation-only-decides).
- AEC3's own metrics only cover its linear filter: on the spike's recording, which lost 30 dB of echo, it reported 5–7 dB ERLE and an ERL stuck at its initial 20 dB, so the app doesn't show them. Its delay comes in steps of 4 ms, 4 ms on that recording, 0 live with the USB microphone (another microphone showed more).

### Double talk: the echo cancellation only decides

**The problem:** AEC3 works in two steps. Its linear filter predicts the echo from the reference and subtracts it, which leaves the voice as it is. Its suppressor then turns down, band by band, where echo is left, and takes the voice in those bands with it while you talk into the playback. Yet DeepFilterNet3 tells music from a voice by itself; only speech from the speakers (a call, a video) it can't tell from yours.

**Measured** (2026-11-01, offline, `examples/sidechain.rs`, the USB microphone, with only the echo cancellation and DeepFilterNet3): a double talk whose voice is known, built from a recording of the echo alone (the microphone and the reference) with a recording of the speaker alone added. Every variant runs on it and on the voice alone; per band (24, 100 Hz to 20 kHz) and 20 ms where the voice is, how much quieter the output is than for the voice alone (*loss*, the mean, and the share of more than 6 dB, choppy), and the level that's left of the echo alone. Three echoes: a video with speech over a laptop's speakers (at the microphone 14 dB below the voice; also raised by 12 dB, about as loud as the voice), music without vocals and a song with vocals over a Bluetooth speaker (both about 11 dB below the voice).

| Echo                     | Variant                         | Echo alone | Loss    | > 6 dB |
| ------------------------ | ------------------------------- | ---------- | ------- | ------ |
| Video, speech            | now (AEC3, then DeepFilterNet3) | −89 dBFS   | 8.0 dB  | 38 %   |
|                          | without the echo cancellation   | −85 dBFS   | 2.7 dB  | 13 %   |
|                          | sidechain, adaptive (see below) | −89 dBFS   | 3.6 dB  | 13 %   |
| Video, speech, +12 dB    | now                             | −118 dBFS  | 7.3 dB  | 33 %   |
|                          | without the echo cancellation   | −71 dBFS   | 5.1 dB  | 25 %   |
|                          | sidechain, adaptive             | −118 dBFS  | 3.6 dB  | 14 %   |
| Music without vocals     | now                             | −120 dBFS  | 10.7 dB | 51 %   |
|                          | without the echo cancellation   | −120 dBFS  | 5.0 dB  | 23 %   |
|                          | sidechain                       | −120 dBFS  | 4.6 dB  | 30 %   |
| Song with vocals         | now                             | −120 dBFS  | 14.4 dB | 64 %   |
|                          | without the echo cancellation   | −120 dBFS  | 6.3 dB  | 30 %   |
|                          | sidechain                       | −120 dBFS  | 7.1 dB  | 39 %   |

- **Music:** DeepFilterNet3 alone removes it completely, with vocals too (this song); the echo cancellation adds nothing but the loss, twice as much as without it.
- **Speech from the speakers:** DeepFilterNet3 alone leaves −71 to −85 dBFS of it (after the AGC up to 30 dB more), the echo cancellation is needed.
- Part of the loss is DeepFilterNet3's own: with the echo it also loses 3–6 dB against the voice alone.

**The sidechain:** DeepFilterNet3 analyzes the echo cancellation's output, as now, but its gains go to a mix of 25 % of that and 75 % of the raw microphone (delayed by the echo cancellation's 5 ms, so they line up; `DfTract::process_with`): the suppressor only steers which bands pass, it no longer turns them down itself. The deep filter stays off then, its coefficients are a filter for the analyzed spectrum, not the mix (with it the loss was 3–5 dB higher); the gains cover the lower frequencies too. Between a mix of 0 and 0.25 the difference is small, at 0.5 the loss rises. Alone it leaves more of an echo alone (speech −68 to −81 dBFS instead of −89 to −118), as its gains don't close fully where the analyzed signal is almost silent. *Adaptive*: the sidechain only while DeepFilterNet3's SNR estimate on the analyzed signal is above 10 dB (held 200 ms, the switch crossfaded over a frame), otherwise the chain as now; the estimate comes 30 ms before the output, so it switches in time. At 5 dB the voice keeps a little more (2.9 dB loss), but remains of the speech after the echo cancellation open it and leave −83 instead of −89 dBFS of a quiet echo. AEC3's linear output (only subtracted, without the suppressor) would be the better mix partner than the raw microphone, but sonora doesn't hand it out and in WebRTC it only covers the lowest band (up to 8 kHz).

**Speech in what the speakers play** (2026-11-01, `examples/reference_vad.rs`): which of the two the echo cancellation should do follows from the reference, not from the microphone. DeepFilterNet3 on the reference tells them apart, and only its encoder is needed for the SNR estimate (`DfTract::analyze`, without the decoders, the deep filter and the synthesis: the same values in half the time, 110–150 µs per frame). Means over 3 s, of 28 windows of the video and of 1 min each of the music:

| Encoder runs               | Per frame | Video: windows > 5 dB, highest | Song with vocals: highest | Without vocals: highest |
| -------------------------- | --------- | ------------------------------ | ------------------------- | ----------------------- |
| every frame                | 111 µs    | 7, +17.9 dB                    | +1.1 dB                   | −10.1 dB                |
| runs of 10 of 40 frames    | 28 µs     | 7, +18.3 dB                    | −0.4 dB                   | −9.6 dB                 |
| runs of 25 of 100 frames   | 28 µs     | 12, +22.1 dB                   | −0.5 dB                   | −7.3 dB                 |
| runs of 10 of 80 frames    | 14 µs     | 8, +14.1 dB                    | +5.1 dB                   | −8.5 dB                 |

- In runs of consecutive frames it can run a quarter of the time (about 0.3 % of a core, and only while the speakers play); an eighth and less, the song with vocals got above 5 dB in single windows.
- Every second or fourth frame instead doesn't work: the model sees the signal two or four times as fast, and at four times it hardly recognizes speech anymore (6 instead of 42 seconds above 0 dB).
- WebRTC's RNN VAD (in sonora's AGC2, 23 µs per frame) is no alternative: it takes music for speech (88 % of the frames of the music without vocals).

**In the chain, first version** (2026-11-01): the echo stage handed the noise suppression the raw microphone (delayed by its latency) and what the speakers play: silent, speech or music, the latter two from DeepFilterNet3's encoder on the reference (in runs of 25 of 100 frames, 0.08 ms per frame on average in the echo stage; speech when the playback starts and as soon as a single run is above 5 dB, the 3 s mean missed short passages of the video's speech; music after 10 s without). With speech the adaptive sidechain, with music the sidechain all the time, at Voice Protection (`protect`) 75 %.

**Heard live:** with music, more of it came through than with the echo cancellation alone. Measured in three steps:

- *While you talk:* the noise suppression is open then, and between syllables and in bands without the voice the music of the raw microphone passes. The loss above only counted the cells with the voice; the level of the cells without it, against the voice's (*gaps*), on the Bluetooth recording with the music 10 dB louder (instrumental) and the song with vocals at its louder take:

  | Voice Protection          | Instrumental: loss, gaps | With vocals: loss, gaps |
  | ------------------------- | ------------------------ | ----------------------- |
  | 0 % (as before)           | 22.1 dB, −34.1 dB        | 23.1 dB, −41.9 dB       |
  | 0 %, without deep filter  | 14.6 dB, −32.5 dB        | 18.5 dB, −38.3 dB       |
  | 25 %                      | 13.1 dB, −31.2 dB        | 16.0 dB, −33.8 dB       |
  | 50 %                      | 11.8 dB, −28.2 dB        | 14.7 dB, −28.9 dB       |
  | 75 % (the first default)  | 11.7 dB, −26.6 dB        | 14.4 dB, −27.2 dB       |

  Most of the voice it keeps comes from leaving the deep filter out; above 25 % it hardly keeps more of it, while the music in the gaps rises by 5–7 dB. Hence 25 %.
- *Better partners for the mix, tried and dropped:* AEC3's linear output (the echo subtracted, before the suppressor; a local patch of sonora, which computes it but doesn't hand it out) removed only 2.4 dB of the music without vocals, 5.4 dB of the song with vocals and 5.5 dB of the video's speech, against 9–22 dB of AEC3 in all: its filter covers only the first ~50 ms of the echo, and the speakers distort. A mix band by band by DeepFilterNet3's gains (the raw microphone where they're high, the echo cancellation's output where they're low) lay on the same trade-off as Voice Protection: voice and music share the bands where the voice is.
- *While you're silent:* through the whole chain (`trace --wav`, with AGC and compressor) the music without vocals alone came out at −62.5 dBFS without the sidechain, −51.2 dBFS at 25 % and −42.7 dBFS at 75 %: the gains don't close fully on the echo cancellation's output, and what they let through came from the raw microphone (the offline measurement above, 5 s of the echo alone without the AGC, didn't show it).

**In the chain now:** the sidechain only while the speakers play (the reference above −70 dBFS within the last second) and you talk (DeepFilterNet3's SNR above 10 dB, held 200 ms), whatever they play; while you're silent the echo cancellation acts fully (the music alone at −62.5 dBFS again, as without the sidechain), with silent speakers it's as before. With that music and speech from the speakers got the same, so the detector of speech in the reference went, with its parameter; `DfTract::analyze` stays in deep-filter-rt. The parameter of the echo stage: `protect` (25 %; 0 % is the chain as before).

Still to do: more recordings (rap, a podcast with a music bed, a real call over the speakers) and listening live: whether the level of the music under the voice is heard to change with the phrases (about 11 dB between talking and pauses), and whether quiet word beginnings switch in time.

- AGC2 against our AGC, compressor and limiter, on another recording: 6 dB quieter with its defaults, the same spread between quiet and loud speech, pauses 5 dB further below the speech, peaks up to 0 dBFS. Not better enough to give up our adjustable chain; the expander would bring the quieter pauses.

## Suppressing other voices

**Postponed** (2026-10-31): in a first test Hush didn't separate the voices, DeepFilterNet3 alone did better (see [Test](#test) below). The plan as it was:

The feature can be enabled at runtime and offers two modes. Hush costs no license fees (Apache 2.0); the costs are latency, sound and maintenance.

| Mode | How it works                                                                 | Advantage                                         | Disadvantage                                               |
| ---- | ---------------------------------------------------------------------------- | ------------------------------------------------- | ---------------------------------------------------------- |
| Off  | Hush doesn't run                                                             | No CPU load                                       | No suppression                                             |
| Full | Hush's output replaces the signal                                            | Strongest suppression, even when speaking at once | Everything above 8 kHz is missing, sounds like a phone     |
| Gate | Hush runs in parallel and only provides a gain per 10 ms for the 48 kHz path | Full sound                                        | Other voices mainly removed in speech pauses; experimental |

**Costs in detail:**

- **CPU:** under 1 ms per 10 ms frame, barely noticeable next to DeepFilterNet3.
- **Latency:** about 11 ms extra (Hush 10 ms plus resampling from 48 to 16 kHz and back). All modes get the same latency, so switching with a crossfade stays click-free.
- **Development:** fixed 3:1 resampling, crossfade when switching, smoothing of the gate gain (open fast, close slowly).
- **Maintenance:** a project of a company with a different core business, version 2 announced. The model format may change.

If you never need the feature, leave the stage out entirely and save the 11 ms.

### Test

A recording from the USB microphone (speech at about −36 dBFS RMS, rather quiet): first the speaker alone, then a video with speech from a phone about a meter away, both at once. Run offline through DeepFilterNet3, hush-vani 0.1.2 (Full, and Gate with a gain per 10 ms from Hush's output against its input) and combinations, compared by ear and by the level per second against the input:

- DeepFilterNet3 alone keeps the speaker (−2 to −8 dB) and lets the video through while it plays alone; while the speaker talks, it suppresses the video by itself, and sounds good.
- Hush after DeepFilterNet3 swallowed the speaker alone (−27 to −65 dB). Hush depends on the level: 10 dB more before it keeps the speaker (still 5–10 dB below DeepFilterNet3 alone), but then attenuates speaker and video alike. With AGC and limiter before it (Hush last) it attenuates everything by 12–15 dB, again without separating: the AGC evens out the level difference Hush seems to rely on.
- Before trying again: a recording with the microphone close (speech peaks around −10 dBFS), which also helps every other stage. Hush alone at the original level separated as little.

Separating by the speaker's own voice (target speaker extraction or personalized speech enhancement, conditioned on an enrollment of a few seconds of the voice) would be the actual solution. Open options as of 2026-10:

- [VoxLock](https://github.com/arbazsiddiqui/voxlock): a 27 M parameter causal band-split RNN (from the REAL-TSE baseline), ONNX, 16 kHz, 24 ms plus chunk latency; but 60–110 % of a core by its own numbers (DeepFilterNet3 needs 2.5 %), and no license stated for the weights.
- Personalized DeepFilterNet2 ([arXiv 2404.08022](https://arxiv.org/abs/2404.08022)), Personalized PercepNet, pDCCRN: papers without released weights. DeepFilterNet3 with a speaker embedding would fit the chain best, but means training it ourselves, which is ruled out (see the decisions under [Risks and open questions](#risks-and-open-questions)).
- [WeSep](https://github.com/wenet-e2e/wesep): a toolkit for target speaker extraction, no pretrained models released yet.

## Voice enhancement

Classic DSP brings the biggest audible gain and runs live with practically no CPU load. ML restoration isn't planned (see level 2).

**Level 1: classic DSP (part of the first version)**

| Order | Stage      | Purpose                                 | Implemented as                                                                                                                                                    |
| ----: | ---------- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|     1 | High-pass  | Rumble, footfall noise, plosives        | 80 Hz, 4th order Butterworth (two biquads); first in the chain, before the echo cancellation and DeepFilterNet3 (see [Signal chain](#signal-chain))              |
|     2 | EQ         | Less muddiness, more presence           | −3 dB at 250 Hz, +3 dB at 3.5 kHz (peaking, Q 1)                                                                                                                  |
|     3 | De-esser   | Harsh S sounds                          | band above 5 kHz turned down while its share of the energy is above −7 dB (ratio 4, at most 12 dB), split with a linear-phase FIR, 1 ms latency                   |
|     4 | AGC        | A constant level, whatever the distance | −18 dBFS RMS, gain −12…+30 dB, rises 6 dB/s, falls 30 dB/s, measures speech only (above −55 dBFS and the noise floor, 80 ms on end, local SNR above 0 dB)         |
|     5 | Compressor | Even loudness within words              | 3:1 above −24 dBFS, soft knee 6 dB, attack 5 ms, release 120 ms                                                                                                   |
|     6 | Expander   | Quiet speech pauses                     | −20 dB unless the level is above −50 dBFS or DeepFilterNet3's local SNR above 0 dB (it's 30 ms ahead, so it opens before a word), hold 200 ms, closes over 150 ms |
|     7 | Limiter    | No clipping                             | −1 dBFS, 1 ms lookahead, release 50 ms                                                                                                                            |

The AGC comes before the compressor (the first draft had it the other way round): the compressor then sees a known level, so its threshold and make-up gain can be fixed relative to the AGC's target. The de-esser goes after the EQ, whose presence boost brings out the S sounds, and before the AGC: it detects the S sounds relative to the total energy, so it doesn't depend on the level. The expander comes after the compressor: the AGC holds its gain in the pauses and the compressor's make-up gain raises what the noise suppression leaves, so the noise became audible there. It opens for the level like a classic expander and also where DeepFilterNet3 hears speech, which catches quieter speech. The AGC measures only what DeepFilterNet3 hears as speech too (its local SNR above 0 dB), so louder noise that passes its noise floor doesn't change the gain; on two recordings the estimate also dropped below 0 dB within words (143 of the 160 frames it left out of a live recording, some at −13 dBFS), which lowered the gain by 1 dB on average and up to 9 dB, while it left out only 17 frames in 89 s that weren't next to speech. It is in for testing live, with its threshold adjustable (−20 dB turns it off). DeepFilterNet3's estimate alone wasn't enough: live it cut quiet speech and word ends now and then, on three recordings up to 20 % of the quieter speech; with the level too none on two of them and 1.7 % on a very quiet one, while short pauses between sentences went 10–20 dB down. WebRTC's AGC2, compared on a recording, kept the pauses 5 dB further below the speech than our AGC and compressor, with the same evenness of the speech, but less to adjust. The values are the defaults, each one adjustable at runtime from the GNOME app. No presets: without measurements of what each use needs, they'd only be guesses (see Decided).

**Level 2: ML restoration ("studio voice"), not planned**

Goal: remove reverb, reconstruct missing highs, repair compression artifacts. There's hardly anything mature that is both live and open. The best candidate was [Stream.FM](https://arxiv.org/html/2512.19442v1) (research paper, late 2025):

- Removes noise and reverb, adds highs, repairs codec artifacts.
- Latency 24 to 48 ms.
- Real time on an RTX 4080; on a Ryzen 7 9800X3D only barely (factor 0.88). Probably not on a laptop processor.
- Code open according to the paper; weights and license still unclear.

Dropped for its cost: it needs a GPU or a whole CPU core, while the chain runs on about 3 % of one. The chain stays slim.

## GPU strategy

None: the chain runs on the CPU only. Live audio comes in 10 ms frames, and for models as small as DeepFilterNet3 the transfer to the GPU costs more than the computation itself. Only a heavy model such as the ML restoration would have needed one, and it isn't planned.

## Architecture

A single Rust process as a PipeWire node. All stages run inside it, so the latency stays under control.

- **Language:** Rust. DeepFilterNet3 (as `deep-filter-rt`) and hush-vani are already Rust, PipeWire via `pipewire-rs`.
- **PipeWire:** reads from the microphone (or from the echo-cancel source) and provides "Voice Filter" as a virtual microphone. It's a node of PipeWire's own that voice-filter creates on its first start (`object.linger`) and only writes into, so it stays until PipeWire restarts: apps stay linked to it while voice-filter restarts or after it crashed, and record silence meanwhile instead of switching to the unfiltered microphone. (At first the device was a stream of the process and went with it; declaring it in `~/.config/pipewire/pipewire.conf.d/` would have kept it across PipeWire restarts too, but needs an installed config file.) It adds no latency: it runs in the microphone's cycle like the streams.
- **Frames:** internally 48 kHz, 10 ms per frame (480 samples). Fits DeepFilterNet3 (hop 480) and Hush (hop 160 at 16 kHz) exactly.
- **PipeWire details** (from a spike with PipeWire 1.6.10, WirePlumber 0.5.18 and `pipewire` 0.10):
  - A stream as the virtual microphone needs `media.class = Audio/Source`, not `Audio/Source/Virtual`: WirePlumber never sets a format for the latter, so it gets no ports and no app can record from it (the same happens to `pw-loopback`'s virtual sources). For a null sink in an adapter (`support.null-audio-sink`, created through the core, as now) it's the other way round: `Audio/Source/Virtual` gets an input and an output port, `Audio/Source` none.
  - WirePlumber links a playback stream only to sinks: with `target.object` on the virtual microphone it played to the default output. voice-filter links its output stream itself (link factory, `node.autoconnect = false`), and the stream is `node.passive`, so the link doesn't keep the virtual microphone running while no app records (`link.passive` on the link had no effect).
  - The quantum belongs to the whole graph: the default here is 1024 samples (21 ms), `node.latency = 480/48000` lowers it, but PipeWire rounds it down to a power of two (`clock.power-of-two-quantum`, true by default), so it runs at 256. With 256 the bridge needs at least 448 samples (9.3 ms) to turn the blocks into 480-sample frames, measured are 11.3 ms; with a quantum of 480 (`clock.power-of-two-quantum = false`, a system-wide setting in `pipewire.conf.d`, or forced) it is 5–15 ms depending on the order of the callbacks, while the microphone gets 4.7 ms slower, so the total doesn't drop (measured, see [Quantum](measurements.md#quantum)). The process callback has to produce `buffer.requested()` samples (feature `v0_3_49`), not the buffer size, otherwise the buffer adds about 250 ms. A FIFO collects the samples into 480-sample frames; if the quantum doesn't fit 480, this adds up to one frame (10 ms) of latency.
  - The capture stream, the output stream and the virtual microphone go into one `node.group`, like `module-loopback` does, so they run in the microphone's cycle; the virtual microphone's own timer only drives it while voice-filter isn't running.
- **Stages:** a common trait with name, latency and processing of one frame. Stages are interchangeable and optional. Each can be switched off at runtime: its input, delayed by its latency, plays instead, crossfaded over one frame, so the total latency never changes; a switched off stage doesn't process and saves its CPU. The whole filter switches off the same way, all stages at once, each keeping its own switch for when it's on again.
- **Memory in the audio path:** the stages allocate nothing per frame, except DeepFilterNet3: tract allocates the output of every node and the packed operands of its matrix products, about 820 times per frame, and has no way to run on preallocated tensors. Patching tract for that would touch most of its ops and its tensor storage, an inference of our own would take days and risk the model's output; instead the binary's allocator (`rt_alloc`) serves what the chain allocates in the live mode from a pool of 4 MiB, reserved and touched at the start (TLSF, constant time, no system calls; a spin lock only other threads freeing into it contend for), everything else from the system allocator. The chain takes about 300 KB of it; neither faster nor slower than glibc (see `docs/measurements.md`).
- **Resampling 48 ↔ 16 kHz:** fixed filter with factor 3. Known, constant latency, no allocation in the audio path.
- **Control:** a GNOME app (GTK4 with libadwaita via `gtk4-rs`/`libadwaita-rs`, `voice-filter ui`) as a separate process; the audio process keeps running when the window is closed. They talk over the D-Bus session bus (`zbus`, name `net.b128.VoiceFilter.Daemon`), which also allows a command line (`busctl` or our own `voice-filter ctl`). Inside the audio process the settings are shared lock-free with the audio path.
- **Test mode:** the same chain processes WAV files offline. This allows comparing models and settings and measuring real-time capability.

## Risks and open questions

The biggest risk is latency: all stages together must stay live.

| Risk                                                                          | Consequence                         | Countermeasure                                                  |
| ----------------------------------------------------------------------------- | ----------------------------------- | --------------------------------------------------------------- |
| Latency adds up (DFN3 30 ms, FIFO up to 10 ms, Hush +11 ms, PipeWire buffers) | Conversations feel delayed          | Measure and show latency per stage; stages can be left out      |
| Gate mode cuts off quiet speech of your own                                   | Syllables go missing                | Open fast, close slowly; adjustable floor                       |
| hush-vani is a third-party project                                            | Maintenance uncertain               | Encapsulate behind an interchangeable interface; ONNX as plan B |
| DeepFilterNet3 outdated on crates.io, upstream dormant since 2024             | Real-time part only usable from git | Vendored as `deep-filter-rt`, published ourselves               |

**Decided:**

- Latency budget: no fixed number up front. Phase 1 measures the latency per stage, the budget follows from it and decides whether Hush and the low-latency variant of DeepFilterNet3 are needed. Measured so far: DeepFilterNet3 delays by 30 ms (10 ms STFT overlap plus 2 lookahead frames of 10 ms, `DfTract::delay`), not 20 ms as first assumed; with the FIFO (up to 10 ms) and the PipeWire buffers the target of 40 ms is out of reach with DeepFilterNet3 alone. Measured live (USB microphone, quantum 256): microphone 8.0 ms (as PipeWire reports it) + bridge 11.3 ms + chain 31.0 ms (DeepFilterNet3 and limiter) = 50.3 ms until the app, which buffers on top of that (the de-esser added 1 ms to the chain since); at most 2 ms of processing per 10 ms frame, no dropouts. The low-latency variant without lookahead would save 20 ms; a quantum of 480 saves nothing, what the bridge gains the microphone loses (see PipeWire details).
- Echo cancellation: in our own process, not PipeWire's module (see [Echo cancellation](#echo-cancellation)).
- Control: GNOME app over D-Bus.
- No ML restoration: it would cost a GPU or a whole CPU core, the chain stays slim (see [Voice enhancement](#voice-enhancement)).
- No low-latency variant of DeepFilterNet3: measured on the VoiceBank-DEMAND test set (824 sentences, the noise suppression alone, both models on every frame, without `DfTract` skipping frames by the estimated SNR), it delays by 10 instead of 30 ms, but needs about five times the CPU (1.8 instead of 0.37 ms per 10 ms frame: it makes up for the missing lookahead with a larger network) for the same quality (PESQ 3.07 against 3.16, STOI 0.950 against 0.945, SI-SDR 18.7 against 18.9 dB), and at 39 MB it can't be embedded.
- No quantized weights (fp16, int8): the model fits the crate and takes about 5 % of a core live; it would only have mattered to embed the low-latency variant.
- No models trained ourselves, only released ones: personalized denoising that keeps only the user's voice needs an open model that runs in real time on little CPU, not DeepFilterNet3 trained with a speaker embedding (see [Suppressing other voices](#suppressing-other-voices)).
- No presets for voice enhancement (such as "Call", "Stream", "Podcast"): there are no measurements of what each use needs, the defaults and the sliders in the app are enough.
- No package for Arch (AUR).
- No filter for steady tones (fans) before DeepFilterNet3: with the high-pass first and the input gain the tones of the built-in microphone's fans don't open the model anymore, taking them out left the noise in the pauses and the estimate as they were (see [measurements](measurements.md#steady-tones)).

**Open questions:**

- [ ] Default attenuation limit of DeepFilterNet3, to be chosen by listening in the GNOME app (it can change at runtime). Unlimited (the model's default) sounds dull: in a first test recording the speech above 1 kHz lost about 20 dB, with a limit of 12 dB about 11 dB, while the pauses lost 13 and 8 dB. That recording clipped (0.16 % of the samples), which may account for part of it.
- [ ] Defaults of the expander: speech threshold 6 dB and opening above −40 dBFS (see [DeepFilterNet3 on recordings](#model-choice)), once more recordings and clean speech confirm it.
- [ ] Drop the zero mask of DeepFilterNet3 (see [Model choice](#model-choice)): on the VoiceBank-DEMAND test set PESQ 3.16 instead of 2.70 and quiet speech isn't muted anymore, but the pauses aren't exact silence anymore, the expander has to keep the noise down. Through the chain on the recordings: on the USB microphone hardly any difference, the noise of the built-in microphone came through at −44 to −50 dBFS in three quarters of the pause frames without it (the expander's level criterion, −50 dBFS, kept it open). Measured again with the chain now: the same; with the expander opening only above −40 dBFS the pauses of the fans went about as quiet as with the zeros (8.9 against 4.9 % of the frames above −60 dBFS) but as much speech came out below −45 dBFS (18 %), on the call 18.7 against 1.4 % in the pauses and 4.8 against 8.0 % of the speech ([measurements](measurements.md#zero-mask)). The zeros also skip the model's decoders, which saves about 40 % of the chain's CPU (156 instead of 272 µs per frame on the call, three quarters of its frames zeroed); to be decided by listening.
- [ ] Settings per microphone: they're global, while a good and a noisy microphone want different ones (the expander's thresholds, the level the noise suppression hears the voice at); they'd follow the input chosen.
- [ ] Should the chain also filter incoming audio (speaker side)?
- [ ] Test coverage of the live mode and the app's window, which the CI's coverage leaves out as whole files (they need PipeWire or a display): either pull what can be tested out into files of their own, as with the bridge and the settings (on stable, only whole files can be left out), or measure on nightly and leave out single functions with `#[cfg_attr(coverage_nightly, coverage(off))]`. Not decided whether it's worth it.

## Roadmap

Three phases, each usable on its own. Each phase ends with a latency and listening test.

1. **Base** – done
   - [x] vendor `deep-filter-rt`
   - [x] PipeWire node with DeepFilterNet3
   - [x] offline mode for WAV tests
   - [x] latency measurement per stage
2. **Voice** – done
   - [x] high-pass, EQ, de-esser, AGC, compressor, expander, limiter
   - [x] D-Bus interface and GNOME app
   - [x] attenuation limit of DeepFilterNet3 as a runtime setting (its default is still open)
3. **Echo**
   - [x] spike: AEC3 from sonora on a recording (see [Echo cancellation](#echo-cancellation))
   - [x] AEC3 as the first stage, the speakers' monitor as the reference, the output device chosen in the app (5 ms latency; allocation-free since sonora 0.3.0)
   - [x] live test: about 30 dB of the echo removed, the reference stream on the speakers' clock
   - [x] offline: what the echo cancellation costs the voice in double talk, the sidechain against it, and DeepFilterNet3's encoder telling speech in the reference from music (see [Double talk](#double-talk-the-echo-cancellation-only-decides))
   - [x] less choppy speech when talking into the playback: the sidechain in the chain while you talk (Voice Protection, 25 %; live still to listen to)
   - [ ] more recordings for it (rap, a podcast with music, a real call), listening tests
   - [ ] a test with only speech from the speakers, and over a longer call
   - other voices (Hush with Full and Gate modes, or a personalized model) are postponed
