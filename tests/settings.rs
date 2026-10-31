use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::voice_filter::param::Param;
use ::voice_filter::settings;

type Switches = Vec<(&'static str, Arc<AtomicBool>)>;
type Params = Vec<(&'static str, Param)>;

#[test]
fn keeps_the_input() {
    let dir = ::tempfile::tempdir().unwrap();
    // a directory that doesn't exist yet
    let dir = dir.path().join("voice-filter");
    assert_eq!(settings::load_input(&dir).unwrap(), None);

    settings::save_input(&dir, "alsa_input.usb-mic").unwrap();
    assert_eq!(
        settings::load_input(&dir).unwrap().as_deref(),
        Some("alsa_input.usb-mic")
    );
}

#[test]
fn treats_an_empty_file_as_no_input() {
    let dir = ::tempfile::tempdir().unwrap();
    ::std::fs::write(dir.path().join("input"), "\n").unwrap();
    assert_eq!(settings::load_input(dir.path()).unwrap(), None);
}

#[test]
fn keeps_the_output_apart_from_the_input() {
    let dir = ::tempfile::tempdir().unwrap();
    settings::save_input(dir.path(), "usb-mic").unwrap();
    settings::save_output(dir.path(), "speakers").unwrap();
    assert_eq!(
        settings::load_input(dir.path()).unwrap().as_deref(),
        Some("usb-mic")
    );
    assert_eq!(
        settings::load_output(dir.path()).unwrap().as_deref(),
        Some("speakers")
    );
}

/// The switches and parameters of a small chain, as `Chain::switches` and `Chain::params` give them.
fn chain() -> (Switches, Params) {
    (
        vec![
            ("denoise", Arc::new(AtomicBool::new(true))),
            ("agc", Arc::new(AtomicBool::new(true))),
        ],
        vec![
            (
                "denoise",
                Param::new("limit", "Limit", "dB", (0., 100., 1.), 100.),
            ),
            (
                "agc",
                Param::new("target", "Target", "dBFS", (-30., -12., 0.5), -18.),
            ),
            (
                "agc",
                Param::new("max-gain", "Maximum Gain", "dB", (0., 40., 1.), 30.),
            ),
        ],
    )
}

#[test]
fn keeps_the_stages_and_parameters() {
    let dir = ::tempfile::tempdir().unwrap();
    let (switches, params) = chain();
    switches[1].1.store(false, Ordering::Relaxed);
    params[0].1.set(30.).unwrap();
    params[1].1.set(-20.5).unwrap();
    settings::save_chain(dir.path(), &switches, &params).unwrap();
    // only what differs from the defaults
    assert_eq!(
        ::std::fs::read_to_string(dir.path().join("chain")).unwrap(),
        "agc off\ndenoise.limit 30\nagc.target -20.5\n"
    );

    let (switches, params) = chain();
    let warnings = settings::restore_chain(dir.path(), &switches, &params).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(switches[0].1.load(Ordering::Relaxed));
    assert!(!switches[1].1.load(Ordering::Relaxed));
    assert_eq!(params[0].1.get(), 30.);
    assert_eq!(params[1].1.get(), -20.5);
    assert_eq!(params[2].1.get(), 30.);
}

#[test]
fn restores_nothing_without_saved_settings() {
    let dir = ::tempfile::tempdir().unwrap();
    let (switches, params) = chain();
    let warnings = settings::restore_chain(dir.path(), &switches, &params).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(params[0].1.get(), 100.);
}

#[test]
fn skips_what_doesnt_fit_the_chain() {
    let dir = ::tempfile::tempdir().unwrap();
    ::std::fs::write(
        dir.path().join("chain"),
        "reverb off\nagc maybe\ndenoise.limit 120\ndenoise.color 3\nagc.target loud\nagc\n\
         denoise off\nagc.max-gain 20\n",
    )
    .unwrap();
    let (switches, params) = chain();
    let warnings = settings::restore_chain(dir.path(), &switches, &params).unwrap();
    assert_eq!(warnings.len(), 6, "{warnings:?}");
    assert!(warnings[0].contains("reverb off"), "{}", warnings[0]);
    // the rest still applies
    assert!(!switches[0].1.load(Ordering::Relaxed));
    assert!(switches[1].1.load(Ordering::Relaxed));
    assert_eq!(params[0].1.get(), 100.);
    assert_eq!(params[1].1.get(), -18.);
    assert_eq!(params[2].1.get(), 20.);
}

#[test]
fn keeps_listen_and_its_volume() {
    let dir = ::tempfile::tempdir().unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), None);
    // forgetting what isn't kept is fine
    settings::save_listen(dir.path(), None).unwrap();
    settings::save_listen(dir.path(), Some(true)).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), Some(true));
    settings::save_listen(dir.path(), Some(false)).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), Some(false));
    settings::save_listen(dir.path(), None).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), None);

    assert_eq!(settings::load_listen_volume(dir.path()).unwrap(), None);
    settings::save_listen_volume(dir.path(), -12.5).unwrap();
    assert_eq!(
        settings::load_listen_volume(dir.path()).unwrap(),
        Some(-12.5)
    );
    ::std::fs::write(dir.path().join("listen-volume"), "loud\n").unwrap();
    assert!(settings::load_listen_volume(dir.path()).is_err());
}
