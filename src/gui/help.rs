//! What the stages, their parameters and the status rows do: behind an info button at the end of
//! their row, or inside an expanding row. The texts are in `i18n/`.

use ::adw::prelude::*;

use crate::i18n::{lookup, tr};

/// What a stage does, by its name in the chain (`help-stage-…` in `i18n/`).
pub fn stage(name: &str) -> Option<String> {
    lookup(&format!("help-stage-{name}"), &[])
}

/// What a parameter changes, by stage and parameter name (`help-param-…`).
pub fn param(stage: &str, name: &str) -> Option<String> {
    lookup(&format!("help-param-{stage}-{name}"), &[])
}

/// The ids of the texts of the other rows.
pub const FILTER: &str = "help-filter";
pub const LISTEN_VOLUME: &str = "help-listen-volume";
pub const INPUT_LEVEL: &str = "help-input-level";
pub const OUTPUT_LEVEL: &str = "help-output-level";
pub const LATENCY: &str = "help-latency";
pub const PROCESSING: &str = "help-processing";
pub const DROPOUTS: &str = "help-dropouts";

/// The explanation as the first row inside an expanding row, whose header has no room for an
/// info button at its end (libadwaita puts the switch and the arrow there).
pub fn text_row(text: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .xalign(0.)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .css_classes(["dim-label"])
        .build();
    gtk::ListBoxRow::builder()
        .activatable(false)
        .selectable(false)
        .child(&label)
        .build()
}

/// The info button, always the last widget of its row.
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
        .tooltip_text(tr("more-information"))
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build();
    button.add_css_class("flat");
    button
}
