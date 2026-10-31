#![cfg(feature = "gui")]

use ::std::path::Path;

use ::voice_filter::gui::desktop;

#[test]
fn installs_the_entry_and_the_icon() {
    let dir = ::tempfile::tempdir().unwrap();
    let [entry, icon] =
        desktop::install(dir.path(), Path::new("/opt/voice filter/voice-filter")).unwrap();
    assert_eq!(
        entry,
        dir.path().join("applications/net.b128.VoiceFilter.desktop")
    );
    assert_eq!(
        icon,
        dir.path()
            .join("icons/hicolor/scalable/apps/net.b128.VoiceFilter.svg")
    );
    let entry = ::std::fs::read_to_string(entry).unwrap();
    assert!(entry.starts_with("[Desktop Entry]\n"), "{entry}");
    assert!(
        entry.contains("\nExec=\"/opt/voice filter/voice-filter\" ui\n"),
        "{entry}"
    );
    assert!(entry.contains("\nIcon=net.b128.VoiceFilter\n"), "{entry}");
    assert!(::std::fs::read_to_string(icon).unwrap().starts_with("<svg"));
}

#[test]
fn quotes_paths_for_exec() {
    assert_eq!(
        desktop::quote("/usr/bin/voice-filter"),
        "/usr/bin/voice-filter"
    );
    assert_eq!(desktop::quote("/a b/v"), "\"/a b/v\"");
    assert_eq!(desktop::quote("/a$b/\"v\""), "\"/a\\$b/\\\"v\\\"\"");
}
