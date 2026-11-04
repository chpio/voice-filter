# The texts of the app and the tray. A single line break is a space, an empty line a new
# paragraph (see src/i18n.rs). Ids of stages and parameters follow their names in the chain.

## Stages, in the order of the chain
stage-echo = Echo Cancellation
stage-denoise = Noise Suppression
stage-highpass = High-Pass Filter
stage-eq = Equalizer
stage-deesser = De-Esser
stage-agc = Automatic Gain Control
stage-compressor = Compressor
stage-expander = Expander
stage-limiter = Limiter

## The groups of stages in the window
group-echo = Echo Cancellation
group-echo-description =
    Removes the echo of the speakers chosen here from the microphone, for calls without headphones.
group-denoise = Noise Suppression
group-denoise-description =
    Removes background noise. The limit keeps part of it: lower sounds more natural, 100 dB removes
    it all.
group-tone = Tone
group-tone-description =
    Removes rumble, makes the voice clearer (less boom in the low mids, more presence) and tames
    harsh S sounds.
group-level = Level
group-level-description =
    Brings the voice to a constant level, evens out the loudness within words, keeps the noise down
    in speech pauses and the peaks below −1 dBFS.

## Parameter titles (the daemon sends the same in English)
param-echo-protect = Voice Protection
param-denoise-level = Input Level
param-denoise-max-gain = Maximum Input Gain
param-denoise-limit = Limit
param-highpass-freq = Frequency
param-eq-low-mid = Low Mids (250 Hz)
param-eq-presence = Presence
param-eq-presence-freq = Presence Frequency
param-deesser-freq = Frequency
param-deesser-threshold = Threshold
param-agc-max-gain = Maximum Gain
param-agc-threshold = Speech Threshold
param-agc-target = Target Level
param-compressor-threshold = Threshold
param-compressor-ratio = Ratio
param-expander-range = Range
param-expander-threshold = Speech Threshold
param-expander-hold = Hold
param-expander-level = Opens Above
param-limiter-ceiling = Ceiling
param-limiter-release = Release
param-listen-volume = Volume

## What the stages do, inside their rows
help-stage-echo =
    Removes from the microphone what the speakers chosen below play, so the other side of a call
    doesn't hear itself when you don't wear headphones. It adapts to the delay and the room while
    the speakers play.

    Adds 5 ms of latency.
help-stage-denoise =
    A neural network (DeepFilterNet3) separates the voice from the background noise, such as fans,
    keyboards or traffic, and removes the noise. It hears a quiet microphone raised to a level it
    works at.

    Adds 30 ms of latency, the most of all stages, even when switched off (why: see Filter at the
    top).
help-stage-highpass =
    Removes everything below its frequency: rumble, footfall, bumps against the desk and the pops of
    P and B sounds. The voice itself lies above it. It comes first, so the noise suppression doesn't
    take the rumble for a voice.
help-stage-eq =
    Shapes the tone: turns down the low mids around 250 Hz, which make a voice sound muddy or boomy,
    and raises the presence around 3.5 kHz, which makes speech clearer.
help-stage-deesser =
    Turns down harsh S and SH sounds: while the band above its frequency makes up more of the sound
    than the threshold, it turns that band down, by at most 12 dB. The rest of the voice stays as it
    is.

    Adds 1 ms of latency.
help-stage-agc =
    Brings the voice to a constant level, whether you speak softly or loudly, close to the
    microphone or farther away. It adjusts over seconds (up by at most 6 dB per second, down faster)
    and keeps the gain in speech pauses in steady noise such as hum and while you type, so it
    doesn't raise them.
help-stage-compressor =
    Evens out the loudness within words and sentences, which is too fast for the gain control: it
    turns loud syllables down above the threshold and raises everything by about as much, so the
    voice sounds steadier and fuller at the same level.
help-stage-expander =
    Turns speech pauses down, so the noise that's left (and that the gain control raised) isn't
    heard between words. It opens as soon as it hears speech, from the level or from how sure the
    noise suppression is, and closes again after the hold time.
help-stage-limiter =
    Keeps the peaks below the ceiling, so laughing, coughing or a loud word don't clip. It looks 1
    ms ahead, to turn down before a peak instead of cutting it off.

    Adds 1 ms of latency.

## What the parameters do, behind the info button
help-param-echo-protect =
    While you talk into what the speakers play, the echo cancellation turns your voice down with the
    echo. While you talk, this much of your voice is kept as it was, the noise suppression removes
    what's left of the echo, music above all; while you're silent the echo cancellation acts fully.
    Higher keeps more of the voice but lets more of what the speakers play through under it; 0 %
    leaves it to the echo cancellation alone.
help-param-denoise-limit =
    How far it may turn the noise down. 100 dB removes it entirely; around 20 to 30 dB keeps a
    little of the background, which sounds more natural and loses less of the voice.
help-param-denoise-level =
    The level the noise suppression hears your voice at. It takes a voice that's too quiet for noise
    and mutes it, so it raises a quiet microphone to this level first and takes the gain back after
    it: what comes out stays as loud. Higher keeps more of a quiet voice, but lets more noise
    through in the pauses.
help-param-denoise-max-gain =
    How far it may raise a quiet microphone for the noise suppression. 0 dB switches the raising
    off.
help-param-highpass-freq =
    Everything below this frequency is removed. Higher removes more rumble, but makes deep voices
    thinner; 80 Hz is safe for all voices.
help-param-eq-low-mid =
    The gain around 250 Hz. Below 0 dB the voice sounds less muddy and boomy, too far down it sounds
    thin.
help-param-eq-presence =
    The gain around the presence frequency. Above 0 dB speech sounds clearer and closer, too far up
    harsh.
help-param-eq-presence-freq = The center of the presence band. Lower sounds warmer, higher brighter.
help-param-deesser-freq =
    Where the S sounds start: it turns down the band above this frequency. Lower catches more of
    them, but turns down more of the voice too.
help-param-deesser-threshold =
    The share of the band in the whole sound above which it acts, 0 dB being all of it. Lower acts
    more often and more strongly, higher only on the harshest S sounds.
help-param-agc-target =
    The level it brings the voice to, on average (RMS). −18 dBFS is usual for calls and leaves room
    for the peaks. The compressor follows it.
help-param-agc-max-gain =
    The most it raises a quiet voice. Higher reaches quieter speech or a microphone farther away,
    but raises its noise as well.
help-param-agc-threshold =
    How sure the noise suppression must be that it hears speech (its signal-to-noise ratio) for the
    gain control to measure it. Higher leaves out more noise, but quieter parts of words too, so it
    raises the voice less; −20 dB leaves it to the level alone, as with the noise suppression off.
help-param-compressor-threshold =
    Above this level it turns down. Lower evens out more, normal syllables too, higher only the
    loudest ones.
help-param-compressor-ratio =
    How much it turns down above the threshold: at 3:1, 3 dB more in give 1 dB more out. 1:1 does
    nothing, higher evens out more but sounds squeezed.
help-param-expander-range =
    How far it turns the pauses down. More makes them quieter, but the change between speech and
    pause more audible; 0 dB turns it off.
help-param-expander-threshold =
    How sure the noise suppression must be that it hears speech (its signal-to-noise ratio) to open.
    Lower opens for more, noise too, higher only for clear speech. With the noise suppression off
    only the level counts.
help-param-expander-hold =
    How long it stays open after the last speech. Longer keeps the ends of words and short pauses,
    shorter quiets the pauses sooner.
help-param-expander-level =
    It also opens when the sound gets louder than this, for quiet speech the noise suppression isn't
    sure about. Lower opens for more, noise too; pauses usually lie at −55 to −65 dBFS.
help-param-limiter-ceiling =
    The highest the peaks may go. −1 dBFS leaves room for what codecs add when they compress the
    sound for the call.
help-param-limiter-release =
    How fast it lets go after a peak. Shorter sounds louder, but distorts sooner.

## What the other rows show
help-filter =
    Switches the whole filter. Off, apps get the microphone as it is, while each stage keeps its own
    switch and settings for when it's on again. The icon in the tray switches it too.

    The latency stays the same when it's off: the microphone is delayed by as much as the filter
    would take (about 37 ms, 30 ms of it the noise suppression; the Latency row below shows all of
    it). Dropping the delay would cut a piece out of the sound when switching off and insert a gap
    when switching on, heard as a click or a stutter in a call; this way switching just crossfades.
help-listen-volume =
    How loud you hear yourself. Only for listening: apps get the voice at the level the filter
    brings it to.
help-input-level =
    How loud the microphone is, before the filter; the bar goes from −60 to 0 dBFS.

    The background noise is its quietest moment in the last 2 seconds, what it picks up while you're
    silent. Below −60 dBFS is quiet; above −45 dBFS the noise suppression has a hard time and leaves
    a hiss. A microphone boost that's turned up (in alsamixer often “Mic Boost” or “Internal Mic
    Boost”) raises the noise; a headset or a USB microphone usually picks up much less.
help-output-level =
    How loud the voice leaves the filter, what apps get; the bar goes from −60 to 0 dBFS. Speech
    should sit around the target level of the gain control (−18 dBFS), the pauses far below.
help-latency =
    How long the voice takes from the microphone to the app: the microphone's buffer, the buffer
    before the virtual microphone, what the stages need (the noise suppression 30 ms) and the buffer
    of the app.
help-processing =
    How long 10 ms of audio took through the stages in the last 10 seconds: the mean, the time 95 %
    and 99 % of it stayed within (rounded up by at most 2 %) and the longest. All of it, the echo
    cancellation and the noise suppression on their own (they take the most), and the other stages
    together. The longest time has to stay well below 10 ms, otherwise there are dropouts; a stage
    switched off takes almost none.

    PipeWire hands the audio over in cycles, the samples per cycle are their size (48 samples are 1
    ms). The filter asks for 480, apps that need a lower latency can make it smaller.
help-dropouts =
    Since the filter started.

    Silence: audio that wasn't ready in time, the virtual microphone played silence instead, a gap.
    A little of it right after an app starts recording is normal, while the buffer settles.

    Dropped: audio thrown away to bring the latency back down, e.g. after the filter paused, a jump.

    Errors: 10 ms of audio a stage failed on, replaced by silence.

## The window
window-title = Voice Filter
main-menu = Main Menu
more-information = More Information
menu-reset = _Reset the Filter…
reset-heading = Reset the Filter?
reset-body =
    Switches all stages on and sets their settings to the defaults. The microphone and the speakers
    stay as they are.
reset-cancel = _Cancel
reset-confirm = _Reset
missing-title = Voice Filter Isn't Running
missing-description = Start it with <tt>voice-filter run</tt>
missing-no-bus = Could not connect to the session bus: { $error }
missing-no-proxy = Could not create the proxy: { $error }
copy = Copy
copied = Copied
group-input = Input
group-input-description = The filter reads from this microphone, apps read from “Voice Filter”.
filter = Filter
filter-subtitle = Off, apps get the microphone as it is
microphone = Microphone
speakers = Speakers
default-input = Default Input
default-output = Default Output
unplugged = { $device } (unplugged)
listen = Listen
listen-subtitle = Plays it on the default output, use headphones
listen-volume = Volume
keep-listening = Keep Listening
keep-listening-subtitle =
    Listens again after a restart. The microphone stays in use and the filter runs all the time.
echo-status = Status
echo-not-in-chain = Not in the chain
echo-not-running = Not running: switched off, or no app records from “Voice Filter”
echo-silent = The speakers are silent, there's no echo to remove
echo-delay-short = less than 4 ms
echo-delay = about { $delay } ms
echo-hears = Hears the speakers at { $level } dBFS, their echo comes back after { $delay }
default-settings = Default Settings
reset = Reset
group-levels = Levels
level-input = Microphone
level-output = Voice Filter
input-not-running = Not running: no app records from “Voice Filter”
input-noise = Background noise at { $floor } dBFS
input-noise-loud =
    Background noise at { $floor } dBFS, very loud: the noise suppression leaves a hiss. Lower the
    microphone boost, if it's up, or use a headset
group-status = Status
latency = Latency
latency-value =
    { $total } ms until the app: microphone { $microphone }, buffer { $bridge }, processing { $chain }
processing = Processing Time
processing-mean = Mean
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = Longest
processing-all = All stages
processing-other = Other stages
processing-summary = { $mean } ms per 10 ms frame, longest { $max } ms
processing-nothing = Nothing processed in the last 10 seconds
processing-cycle = { $samples } samples per cycle
processing-copied = Per 10 ms frame in ms: { $columns }
dropouts = Dropouts Since the Start
dropouts-value =
    { $silence } ms of silence, { $dropped } ms dropped, { $errors ->
        [one] 1 error
       *[other] { $errors } errors
    }

## The icon in the tray
tray-title = Voice Filter
tray-on = On
tray-off = Off: the microphone passes unfiltered
tray-filter = Filter
tray-open = Open Voice Filter
