//! The icon in the tray, through the methods the tray's host reads, without a bus.

use ::std::path::PathBuf;
use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::ksni::{MenuItem, Tray as _};
use ::voice_filter::i18n::tr;
use ::voice_filter::settings;
use ::voice_filter::tray::{ICON_OFF, ICON_ON, ICONS, Tray, write_icons};

/// A tray over a chain without an echo cancellation.
fn tray(settings_dir: Option<PathBuf>, app: Option<&str>) -> Tray {
    Tray {
        enabled: Arc::new(AtomicBool::new(true)),
        switches: vec![("denoise", Arc::new(AtomicBool::new(true)))],
        params: Vec::new(),
        settings_dir,
        icon_theme_path: String::new(),
        app: app.map(PathBuf::from),
    }
}

/// Clicks the checkmark that switches the filter.
fn click_filter(tray: &mut Tray) {
    let menu = tray.menu();
    let MenuItem::Checkmark(item) = &menu[0] else {
        panic!("the first entry switches the filter: {menu:?}");
    };
    (item.activate)(tray);
}

#[test]
fn shows_whether_the_filter_is_on() {
    let tray = tray(None, None);
    assert_eq!(tray.icon_name(), ICON_ON);
    assert_eq!(tray.tool_tip().description, tr("tray-on"));
    tray.enabled.store(false, Ordering::Relaxed);
    assert_eq!(tray.icon_name(), ICON_OFF);
    assert_eq!(tray.tool_tip().description, tr("tray-off"));
    let menu = tray.menu();
    let MenuItem::Checkmark(item) = &menu[0] else {
        panic!("{menu:?}");
    };
    assert!(!item.checked);
}

#[test]
fn the_menu_switches_the_filter_and_keeps_it() {
    let dir = ::tempfile::tempdir().unwrap();
    let mut tray = tray(Some(dir.path().to_owned()), None);
    click_filter(&mut tray);
    assert!(!tray.enabled.load(Ordering::Relaxed));
    assert!(!settings::load_enabled(dir.path()).unwrap());
    click_filter(&mut tray);
    assert!(tray.enabled.load(Ordering::Relaxed));
    assert!(settings::load_enabled(dir.path()).unwrap());
}

/// The tray of the chain `run` builds.
fn tray_of_the_chain(settings_dir: Option<PathBuf>) -> Tray {
    let (chain, _) = ::voice_filter::chain::build(None).unwrap();
    Tray {
        enabled: chain.enabled(),
        switches: chain.switches(),
        params: chain.params(),
        ..tray(settings_dir, None)
    }
}

#[test]
fn the_menu_switches_the_echo_cancellation_and_keeps_it() {
    let dir = ::tempfile::tempdir().unwrap();
    let mut tray = tray_of_the_chain(Some(dir.path().to_owned()));
    let echo = tray.echo().unwrap().clone();
    let click = |tray: &mut Tray| {
        let menu = tray.menu();
        let MenuItem::Checkmark(item) = &menu[1] else {
            panic!("the second entry switches the echo cancellation: {menu:?}");
        };
        assert_eq!(item.label, tr("stage-echo"));
        assert_eq!(item.checked, echo.load(Ordering::Relaxed));
        (item.activate)(tray);
    };
    click(&mut tray);
    assert!(!echo.load(Ordering::Relaxed));
    let chain = ::std::fs::read_to_string(dir.path().join("chain")).unwrap();
    assert_eq!(chain, "echo off\n");
    click(&mut tray);
    assert!(echo.load(Ordering::Relaxed));
    // the filter keeps on while the echo cancellation is switched
    assert!(tray.enabled.load(Ordering::Relaxed));
}

#[test]
fn the_echo_cancellation_waits_for_the_filter() {
    let tray = tray_of_the_chain(None);
    tray.enabled.store(false, Ordering::Relaxed);
    let menu = tray.menu();
    let MenuItem::Checkmark(item) = &menu[1] else {
        panic!("{menu:?}");
    };
    // its own switch, which only counts while the filter is on
    assert!(item.checked);
    assert!(!item.enabled);
}

#[test]
fn opens_the_app_only_if_there_is_one() {
    assert_eq!(tray(None, None).menu().len(), 1);
    let menu = tray(None, Some("/usr/bin/voice-filter")).menu();
    assert_eq!(menu.len(), 3);
    assert!(matches!(menu[1], MenuItem::Separator));
    let MenuItem::Standard(item) = &menu[2] else {
        panic!("{menu:?}");
    };
    assert_eq!(item.label, tr("tray-open"));
}

#[test]
fn writes_the_icons_by_their_names() {
    let dir = ::tempfile::tempdir().unwrap();
    let icons = dir.path().join("voice-filter/icons");
    write_icons(&icons).unwrap();
    for (name, svg) in ICONS {
        let written = ::std::fs::read_to_string(icons.join(format!("{name}.svg"))).unwrap();
        assert_eq!(written, svg);
    }
}
