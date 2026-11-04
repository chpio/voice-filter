//! The icon in the tray, through the methods the tray's host reads, without a bus.

use ::std::path::PathBuf;
use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::ksni::{MenuItem, Tray as _};
use ::voice_filter::settings;
use ::voice_filter::tray::{ICON_OFF, ICON_ON, ICONS, Tray, write_icons};

fn tray(settings_dir: Option<PathBuf>, app: Option<&str>) -> Tray {
    Tray {
        enabled: Arc::new(AtomicBool::new(true)),
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
    assert_eq!(tray.tool_tip().description, "On");
    tray.enabled.store(false, Ordering::Relaxed);
    assert_eq!(tray.icon_name(), ICON_OFF);
    assert!(tray.tool_tip().description.starts_with("Off"));
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

#[test]
fn opens_the_app_only_if_there_is_one() {
    assert_eq!(tray(None, None).menu().len(), 1);
    let menu = tray(None, Some("/usr/bin/voice-filter")).menu();
    assert_eq!(menu.len(), 3);
    assert!(matches!(menu[1], MenuItem::Separator));
    let MenuItem::Standard(item) = &menu[2] else {
        panic!("{menu:?}");
    };
    assert_eq!(item.label, "Open Voice Filter");
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
