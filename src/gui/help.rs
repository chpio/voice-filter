//! What the stages, their parameters and the status rows do, behind an info button on their row.

use ::adw::prelude::*;

/// What each stage does, by its name in the chain.
const STAGES: [(&str, &str); 9] = [
    (
        "echo",
        "Removes from the microphone what the speakers chosen below play, so the other side of a \
         call doesn't hear itself when you don't wear headphones. It adapts to the delay and the \
         room while the speakers play.\n\nAdds 5 ms of latency.",
    ),
    (
        "denoise",
        "A neural network (DeepFilterNet3) separates the voice from the background noise, such \
         as fans, keyboards or traffic, and removes the noise. It hears a quiet microphone \
         raised to a level it works at.\n\nAdds 30 ms of latency, the most of all stages.",
    ),
    (
        "highpass",
        "Removes everything below its frequency: rumble, footfall, bumps against the desk and \
         the pops of P and B sounds. The voice itself lies above it. It comes first, so the \
         noise suppression doesn't take the rumble for a voice.",
    ),
    (
        "eq",
        "Shapes the tone: turns down the low mids around 250 Hz, which make a voice sound muddy \
         or boomy, and raises the presence around 3.5 kHz, which makes speech clearer.",
    ),
    (
        "deesser",
        "Turns down harsh S and SH sounds: while the band above its frequency makes up more of \
         the sound than the threshold, it turns that band down, by at most 12 dB. The rest of \
         the voice stays as it is.\n\nAdds 1 ms of latency.",
    ),
    (
        "agc",
        "Brings the voice to a constant level, whether you speak softly or loudly, close to the \
         microphone or farther away. It adjusts over seconds (up by at most 6 dB per second, \
         down faster) and keeps the gain in speech pauses in steady noise such as hum and \
         while you type, so it doesn't raise them.",
    ),
    (
        "compressor",
        "Evens out the loudness within words and sentences, which is too fast for the gain \
         control: it turns loud syllables down above the threshold and raises everything by \
         about as much, so the voice sounds steadier and fuller at the same level.",
    ),
    (
        "expander",
        "Turns speech pauses down, so the noise that's left (and that the gain control raised) \
         isn't heard between words. It opens as soon as it hears speech, from the level or from \
         how sure the noise suppression is, and closes again after the hold time.",
    ),
    (
        "limiter",
        "Keeps the peaks below the ceiling, so laughing, coughing or a loud word don't clip. It \
         looks 1 ms ahead, to turn down before a peak instead of cutting it off.\n\nAdds 1 ms \
         of latency.",
    ),
];

/// What each parameter changes, by stage and parameter name.
const PARAMS: [(&str, &str, &str); 21] = [
    (
        "echo",
        "protect",
        "While you talk into what the speakers play, the echo cancellation turns your voice \
         down with the echo. While you talk, this much of your voice is kept as it was, the \
         noise suppression removes what's left of the echo, music above all; while you're \
         silent the echo cancellation acts fully. Higher keeps more of the voice but lets more \
         of what the speakers play through under it; 0 % leaves it to the echo cancellation \
         alone.",
    ),
    (
        "denoise",
        "limit",
        "How far it may turn the noise down. 100 dB removes it entirely; around 20 to 30 dB \
         keeps a little of the background, which sounds more natural and loses less of the \
         voice.",
    ),
    (
        "denoise",
        "level",
        "The level the noise suppression hears your voice at. It takes a voice that's too quiet \
         for noise and mutes it, so it raises a quiet microphone to this level first and takes \
         the gain back after it: what comes out stays as loud. Higher keeps more of a quiet \
         voice, but lets more noise through in the pauses.",
    ),
    (
        "denoise",
        "max-gain",
        "How far it may raise a quiet microphone for the noise suppression. 0 dB switches the \
         raising off.",
    ),
    (
        "highpass",
        "freq",
        "Everything below this frequency is removed. Higher removes more rumble, but makes deep \
         voices thinner; 80 Hz is safe for all voices.",
    ),
    (
        "eq",
        "low-mid",
        "The gain around 250 Hz. Below 0 dB the voice sounds less muddy and boomy, too far down \
         it sounds thin.",
    ),
    (
        "eq",
        "presence",
        "The gain around the presence frequency. Above 0 dB speech sounds clearer and closer, \
         too far up harsh.",
    ),
    (
        "eq",
        "presence-freq",
        "The center of the presence band. Lower sounds warmer, higher brighter.",
    ),
    (
        "deesser",
        "freq",
        "Where the S sounds start: it turns down the band above this frequency. Lower catches \
         more of them, but turns down more of the voice too.",
    ),
    (
        "deesser",
        "threshold",
        "The share of the band in the whole sound above which it acts, 0 dB being all of it. \
         Lower acts more often and more strongly, higher only on the harshest S sounds.",
    ),
    (
        "agc",
        "target",
        "The level it brings the voice to, on average (RMS). −18 dBFS is usual for calls and \
         leaves room for the peaks. The compressor follows it.",
    ),
    (
        "agc",
        "max-gain",
        "The most it raises a quiet voice. Higher reaches quieter speech or a microphone farther \
         away, but raises its noise as well.",
    ),
    (
        "agc",
        "threshold",
        "How sure the noise suppression must be that it hears speech (its signal-to-noise \
         ratio) for the gain control to measure it. Higher leaves out more noise, but quieter \
         parts of words too, so it raises the voice less; −20 dB leaves it to the level alone, \
         as with the noise suppression off.",
    ),
    (
        "compressor",
        "threshold",
        "Above this level it turns down. Lower evens out more, normal syllables too, higher \
         only the loudest ones.",
    ),
    (
        "compressor",
        "ratio",
        "How much it turns down above the threshold: at 3:1, 3 dB more in give 1 dB more out. \
         1:1 does nothing, higher evens out more but sounds squeezed.",
    ),
    (
        "expander",
        "range",
        "How far it turns the pauses down. More makes them quieter, but the change between \
         speech and pause more audible; 0 dB turns it off.",
    ),
    (
        "expander",
        "threshold",
        "How sure the noise suppression must be that it hears speech (its signal-to-noise \
         ratio) to open. Lower opens for more, noise too, higher only for clear speech. With \
         the noise suppression off only the level counts.",
    ),
    (
        "expander",
        "hold",
        "How long it stays open after the last speech. Longer keeps the ends of words and \
         short pauses, shorter quiets the pauses sooner.",
    ),
    (
        "expander",
        "level",
        "It also opens when the sound gets louder than this, for quiet speech the noise \
         suppression isn't sure about. Lower opens for more, noise too; pauses usually lie at \
         −55 to −65 dBFS.",
    ),
    (
        "limiter",
        "ceiling",
        "The highest the peaks may go. −1 dBFS leaves room for what codecs add when they \
         compress the sound for the call.",
    ),
    (
        "limiter",
        "release",
        "How fast it lets go after a peak. Shorter sounds louder, but distorts sooner.",
    ),
];

pub const LISTEN_VOLUME: &str = "How loud you hear yourself. Only for listening: apps get \
    the voice at the level the filter brings it to.";

pub const INPUT_LEVEL: &str = "How loud the microphone is, before the filter; the bar goes \
    from −60 to 0 dBFS.\n\nThe background noise is its quietest moment in the last 2 seconds, \
    what it picks up while you're silent. Below −60 dBFS is quiet; above −45 dBFS the noise \
    suppression has a hard time and leaves a hiss. A microphone boost that's turned up (in \
    alsamixer often “Mic Boost” or “Internal Mic Boost”) raises the noise; a headset or a USB \
    microphone usually picks up much less.";

pub const OUTPUT_LEVEL: &str = "How loud the voice leaves the filter, what apps get; the bar \
    goes from −60 to 0 dBFS. Speech should sit around the target level of the gain control \
    (−18 dBFS), the pauses far below.";

pub const LATENCY: &str = "How long the voice takes from the microphone to the app: the \
    microphone's buffer, the buffer before the virtual microphone, what the stages need (the \
    noise suppression 30 ms) and the buffer of the app.";

pub const PROCESSING: &str = "The longest time 10 ms of audio took through all stages in the \
    last 10 seconds. It has to stay well below 10 ms, otherwise there are dropouts.\n\nPipeWire \
    hands the audio over in cycles, the samples per cycle are their size (48 samples are 1 ms). \
    The filter asks for 480, apps that need a lower latency can make it smaller.";

pub const DROPOUTS: &str = "Since the filter started.\n\nSilence: audio that wasn't ready in \
    time, the virtual microphone played silence instead, a gap. A little of it right after an \
    app starts recording is normal, while the buffer settles.\n\nDropped: audio thrown away to \
    bring the latency back down, e.g. after the filter paused, a jump.\n\nErrors: 10 ms of \
    audio a stage failed on, replaced by silence.";

pub fn stage(name: &str) -> Option<&'static str> {
    STAGES
        .iter()
        .find(|(stage, _)| *stage == name)
        .map(|(_, help)| *help)
}

pub fn param(stage: &str, name: &str) -> Option<&'static str> {
    PARAMS
        .iter()
        .find(|(s, n, _)| *s == stage && *n == name)
        .map(|(_, _, help)| *help)
}

/// A flat info button that shows `text` in a popover.
pub fn button(text: &str) -> gtk::MenuButton {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .max_width_chars(40)
        .xalign(0.)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let popover = gtk::Popover::builder().child(&label).build();
    let button = gtk::MenuButton::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("More Information")
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build();
    button.add_css_class("flat");
    button
}
