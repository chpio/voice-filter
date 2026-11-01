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
    assert_eq!(desktop::quote(r#"/a$b/"v""#), r#""/a\\$b/\\"v\\"""#);
    assert_eq!(desktop::quote(r"/a\b"), r#""/a\\\\b""#);
    assert_eq!(desktop::quote("/100%/v"), "/100%%/v");
}

#[test]
fn glib_reads_the_path_back_from_exec() {
    use ::gtk::glib;

    let dir = ::tempfile::tempdir().unwrap();
    let exe = r#"/opt/a "b" $c `d` \e 'f'/voice-filter"#;
    let [entry, _] = desktop::install(dir.path(), Path::new(exe)).unwrap();
    let file = glib::KeyFile::new();
    file.load_from_file(entry, glib::KeyFileFlags::NONE)
        .unwrap();
    let exec = file.string("Desktop Entry", "Exec").unwrap();
    let argv = glib::shell_parse_argv(exec.as_str()).unwrap();
    assert_eq!(argv, [exe, "ui"].map(::std::ffi::OsString::from));
}
