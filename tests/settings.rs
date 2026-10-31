use ::voice_filter::settings;

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
