use ::voice_filter::echo::{
    PERIOD_FRAMES, Playback, PlaybackTracker, RUN_FRAMES, SILENCE_FRAMES, SPEECH_HOLD_FRAMES,
};

/// A level of a frame with sound, in dBFS.
const SOUND_DB: f32 = -30.;
const THRESHOLD_DB: f32 = 5.;

/// Feeds `frames` frames at `level_db`, with `snr_db` as the detector's estimate where it asks for
/// one; returns what it decided after each.
fn feed(tracker: &mut PlaybackTracker, frames: usize, level_db: f32, snr_db: f32) -> Vec<Playback> {
    (0..frames)
        .map(|_| {
            let snr = tracker.analyzes(level_db).then_some(snr_db);
            tracker.frame(level_db, snr, THRESHOLD_DB)
        })
        .collect()
}

#[test]
fn starts_as_speech_and_turns_to_music_without_it() {
    let mut tracker = PlaybackTracker::new();
    assert_eq!(feed(&mut tracker, 1, -120., 0.), [Playback::Silent]);
    // it might be a call: speech until it heard none for the hold time
    let decided = feed(&mut tracker, SPEECH_HOLD_FRAMES + 10, SOUND_DB, -10.);
    let speech = decided
        .iter()
        .take_while(|p| **p == Playback::Speech)
        .count();
    assert_eq!(speech, SPEECH_HOLD_FRAMES - 1);
    assert!(decided[speech..].iter().all(|p| *p == Playback::Music));
}

#[test]
fn stays_with_speech_while_the_detector_hears_it() {
    let mut tracker = PlaybackTracker::new();
    let decided = feed(&mut tracker, 3 * SPEECH_HOLD_FRAMES, SOUND_DB, 15.);
    assert!(decided.iter().all(|p| *p == Playback::Speech));
}

#[test]
fn turns_to_speech_with_the_first_run_of_it() {
    let mut tracker = PlaybackTracker::new();
    feed(
        &mut tracker,
        SPEECH_HOLD_FRAMES + 2 * PERIOD_FRAMES,
        SOUND_DB,
        -10.,
    );
    assert_eq!(feed(&mut tracker, 1, SOUND_DB, -10.), [Playback::Music]);
    let decided = feed(&mut tracker, 2 * PERIOD_FRAMES, SOUND_DB, 25.);
    let music = decided
        .iter()
        .take_while(|p| **p == Playback::Music)
        .count();
    assert!(music <= PERIOD_FRAMES, "{music} frames still music");
    assert!(decided[music..].iter().all(|p| *p == Playback::Speech));
}

#[test]
fn analyzes_a_run_of_every_period_and_not_in_silence() {
    let mut tracker = PlaybackTracker::new();
    let mut analyzed = 0;
    for _ in 0..4 * PERIOD_FRAMES {
        if tracker.analyzes(SOUND_DB) {
            analyzed += 1;
        }
        tracker.frame(SOUND_DB, None, THRESHOLD_DB);
    }
    assert_eq!(analyzed, 4 * RUN_FRAMES);
    assert!(!tracker.analyzes(-120.));
}

#[test]
fn is_silent_after_a_second_of_silence_and_starts_as_speech_again() {
    let mut tracker = PlaybackTracker::new();
    feed(&mut tracker, SPEECH_HOLD_FRAMES + 10, SOUND_DB, -10.);
    // a short pause keeps it
    let decided = feed(&mut tracker, SILENCE_FRAMES - 1, -120., 0.);
    assert!(decided.iter().all(|p| *p == Playback::Music));
    assert_eq!(feed(&mut tracker, 1, -120., 0.), [Playback::Silent]);
    assert_eq!(feed(&mut tracker, 1, SOUND_DB, -10.), [Playback::Speech]);
    // the stream of the speakers stops: silent at once
    assert_eq!(tracker.stopped(), Playback::Silent);
}
