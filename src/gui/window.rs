use ::std::cell::{Cell, RefCell};
use ::std::rc::Rc;
use ::std::time::Duration;

use ::adw::prelude::*;
use ::gtk::{gio, glib};
use ::zbus::proxy::CacheProperties;

use super::help;
use super::proxy::{Parameter, Status, VoiceFilterProxy};
use crate::i18n::{FluentValue, lookup, tr, tr_args};
use crate::samples_ms;

/// The groups of the stages: the id of their title and description (`group-…` in `i18n/`) and
/// their stages, whose titles are `stage-…`; what the stages do is inside their rows ([`help`]).
const GROUPS: [(&str, &[&str]); 4] = [
    ("echo", &["echo"]),
    ("denoise", &["denoise"]),
    ("tone", &["highpass", "eq", "deesser"]),
    ("level", &["agc", "compressor", "expander", "limiter"]),
];

/// The title of a stage, its name if there's none.
fn stage_title(name: &str) -> String {
    lookup(&format!("stage-{name}"), &[]).unwrap_or_else(|| name.to_owned())
}

/// How often the window reads the state; the status properties don't emit change signals.
const POLL: Duration = Duration::from_secs(1);
/// How often it reads the levels for the meters, while the filter runs.
const POLL_LEVELS: Duration = Duration::from_millis(100);
/// The range of the meters, in dBFS.
const METER_MIN_DB: f64 = -60.;
/// The width of a slider's scale, that of a meter too, so they line up.
const SLIDER_WIDTH: i32 = 200;
/// The width of a slider's value in characters, enough for the widest (`-40.0 dBFS`).
const VALUE_CHARS: i32 = 10;
/// Above this noise floor of the microphone (dBFS), for [`NOISY_POLLS`] polls in a row, the window
/// says that it's noisy: the noise suppression leaves a hiss then (heard on a built-in microphone
/// at −27 dBFS).
const NOISY_DB: f64 = -45.;
const NOISY_POLLS: u32 = 5;

/// The row of a stage: it expands to the sliders of its parameters and a row that resets them, its
/// switch turns the stage on and off. Every stage has parameters, so every row expands the same
/// way (a `SwitchRow` among them would have its switch elsewhere).
struct StageRow {
    row: adw::ExpanderRow,
    /// Parameter name and its scale.
    scales: Vec<(String, gtk::Scale)>,
    /// Sets the parameters to their defaults.
    reset: gtk::Button,
}

struct Widgets {
    stack: gtk::Stack,
    missing: adw::StatusPage,
    /// The groups of [`GROUPS`], their rows come with the first status.
    groups: Vec<adw::PreferencesGroup>,
    /// Stage name, its row and the group the row is in.
    stages: RefCell<Vec<(String, StageRow, adw::PreferencesGroup)>>,
    /// The stages with their parameter names that `stages` were built for.
    layout: RefCell<Vec<(String, Vec<String>)>>,
    input: adw::ComboRow,
    /// The `node.name` per entry of `input`, the first is the default input (empty).
    input_names: RefCell<Vec<String>>,
    /// The output device whose echo gets removed, in the row of the echo cancellation.
    output: adw::ComboRow,
    /// Like `input_names`.
    output_names: RefCell<Vec<String>>,
    /// How the echo cancellation does, under `output`.
    echo_status: adw::ActionRow,
    /// The frame counters of the echo cancellation at the last poll, to see whether they move.
    echo_frames: Cell<(f64, f64)>,
    /// The switch of the whole filter and its row, which has the info button after it.
    enabled: gtk::Switch,
    enabled_row: adw::ActionRow,
    /// Its switch switches listening, it expands to its volume and whether it's kept.
    listen: adw::ExpanderRow,
    /// The slider of the volume of Listen, with its value, and the scale in it.
    listen_volume: (gtk::Box, gtk::Scale),
    keep_listen: adw::SwitchRow,
    /// The meters of the microphone (its noise floor in the subtitle) and of the output.
    input_level: (adw::ActionRow, gtk::LevelBar),
    output_level: (adw::ActionRow, gtk::LevelBar),
    /// Whether the filter ran since the poll before, from its frame counter, and the counter.
    running: Cell<bool>,
    frames: Cell<u64>,
    /// Polls in a row with the noise floor above [`NOISY_DB`].
    noisy_polls: Cell<u32>,
    latency: adw::ActionRow,
    /// The processing time: a summary in the subtitle, expanding to a table of its parts.
    processing: adw::ExpanderRow,
    processing_table: ProcessingTable,
    dropouts: adw::ActionRow,
    /// Set while the window shows values it read, so the handlers don't send them back.
    updating: Cell<bool>,
    proxy: RefCell<Option<VoiceFilterProxy<'static>>>,
    /// Stage, name and default of every parameter, for resetting all of them.
    defaults: RefCell<Vec<(String, String, f64)>>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    // says when a value was copied
    let toasts = adw::ToastOverlay::new();
    let (processing, processing_table) = processing_row(&toasts);
    // a row whose subtitle shows a value, selectable and with a button that copies it
    let row = |title: &str| {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle_selectable(true)
            .build();
        row.add_suffix(&copy_button(&toasts, subtitle_of(&row)));
        row
    };
    // a status row with an info button
    let status_row = |title: &str, text: &str| {
        let row = row(title);
        row.add_suffix(&help::button(&tr(text)));
        row
    };
    let widgets = Rc::new(Widgets {
        stack: gtk::Stack::new(),
        missing: adw::StatusPage::builder()
            .icon_name("audio-input-microphone-symbolic")
            .title(tr("missing-title"))
            .description(tr("missing-description"))
            .build(),
        groups: GROUPS
            .iter()
            .map(|(id, _)| {
                adw::PreferencesGroup::builder()
                    .title(tr(&format!("group-{id}")))
                    .description(tr(&format!("group-{id}-description")))
                    .build()
            })
            .collect(),
        stages: RefCell::new(Vec::new()),
        layout: RefCell::new(Vec::new()),
        // the chosen microphone as the subtitle, over the width of the row, instead of squeezed
        // next to the title
        input: adw::ComboRow::builder()
            .title(tr("microphone"))
            .use_subtitle(true)
            .model(&gtk::StringList::new(&[]))
            .build(),
        input_names: RefCell::new(Vec::new()),
        output: adw::ComboRow::builder()
            .title(tr("speakers"))
            .use_subtitle(true)
            .model(&gtk::StringList::new(&[]))
            .build(),
        output_names: RefCell::new(Vec::new()),
        echo_status: row(&tr("echo-status")),
        echo_frames: Cell::new((0., 0.)),
        enabled: gtk::Switch::builder().valign(gtk::Align::Center).build(),
        enabled_row: adw::ActionRow::builder()
            .title(tr("filter"))
            .subtitle(tr("filter-subtitle"))
            .build(),
        listen: adw::ExpanderRow::builder()
            .title(tr("listen"))
            .subtitle(tr("listen-subtitle"))
            .show_enable_switch(true)
            .build(),
        listen_volume: slider(&listen_volume()),
        keep_listen: adw::SwitchRow::builder()
            .title(tr("keep-listening"))
            .subtitle(tr("keep-listening-subtitle"))
            .build(),
        input_level: meter_row(&tr("level-input"), help::INPUT_LEVEL, Some(&toasts)),
        output_level: meter_row(&tr("level-output"), help::OUTPUT_LEVEL, None),
        running: Cell::new(false),
        frames: Cell::new(0),
        noisy_polls: Cell::new(0),
        latency: status_row(&tr("latency"), help::LATENCY),
        processing,
        processing_table,
        dropouts: status_row(&tr("dropouts"), help::DROPOUTS),
        updating: Cell::new(false),
        proxy: RefCell::new(None),
        defaults: RefCell::new(Vec::new()),
    });

    let group = |title: &str, description: Option<&str>, rows: &[&gtk::Widget]| {
        let group = adw::PreferencesGroup::builder().title(title).build();
        group.set_description(description);
        for row in rows {
            group.add(*row);
        }
        group
    };
    let volume_row = adw::ActionRow::builder().title(tr("listen-volume")).build();
    volume_row.add_suffix(&widgets.listen_volume.0);
    volume_row.add_suffix(&help::button(&tr(help::LISTEN_VOLUME)));
    widgets.listen.add_row(&volume_row);
    widgets.listen.add_row(&widgets.keep_listen);
    // not a SwitchRow: its switch would come after the info button
    widgets.enabled_row.add_suffix(&widgets.enabled);
    widgets
        .enabled_row
        .add_suffix(&help::button(&tr(help::FILTER)));
    widgets
        .enabled_row
        .set_activatable_widget(Some(&widgets.enabled));
    let page = adw::PreferencesPage::new();
    page.add(&group(
        &tr("group-input"),
        Some(&tr("group-input-description")),
        &[
            widgets.enabled_row.upcast_ref(),
            widgets.input.upcast_ref(),
            widgets.listen.upcast_ref(),
        ],
    ));
    page.add(&group(
        &tr("group-levels"),
        None,
        &[
            widgets.input_level.0.upcast_ref(),
            widgets.output_level.0.upcast_ref(),
        ],
    ));
    for stage_group in &widgets.groups {
        page.add(stage_group);
    }
    page.add(&group(
        &tr("group-status"),
        None,
        &[
            widgets.latency.upcast_ref(),
            widgets.processing.upcast_ref(),
            widgets.dropouts.upcast_ref(),
        ],
    ));
    widgets.stack.add_named(&page, Some("controls"));
    widgets.stack.add_named(&widgets.missing, Some("missing"));
    widgets.stack.set_visible_child_name("missing");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar.add_top_bar(&header);
    toasts.set_child(Some(&widgets.stack));
    toolbar.set_content(Some(&toasts));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(tr("window-title"))
        .default_width(600)
        .default_height(720)
        .content(&toolbar)
        .build();
    header.pack_end(&main_menu(&widgets, &window));

    connect_controls(&widgets);
    glib::spawn_future_local(poll_levels(widgets.clone()));
    glib::spawn_future_local(poll(widgets));
    window
}

/// A row with a meter of a level, and an info button; with `toasts` its subtitle can be copied.
fn meter_row(
    title: &str,
    help: &str,
    toasts: Option<&adw::ToastOverlay>,
) -> (adw::ActionRow, gtk::LevelBar) {
    // a fixed width, not expanding: the row shares its width between the title and the bar by
    // their natural widths, so the microphone's long subtitle made its meter the shorter one
    let bar = gtk::LevelBar::builder()
        .min_value(0.)
        .max_value(1.)
        .width_request(SLIDER_WIDTH)
        .valign(gtk::Align::Center)
        .build();
    // one color: the default offsets color the low end like a warning
    for offset in [
        gtk::LEVEL_BAR_OFFSET_LOW,
        gtk::LEVEL_BAR_OFFSET_HIGH,
        gtk::LEVEL_BAR_OFFSET_FULL,
    ] {
        bar.remove_offset_value(Some(offset));
    }
    let row = adw::ActionRow::builder().title(title).build();
    // the noise floor in the subtitle of the microphone's, selectable and with a button that
    // copies it; the button before the bar, so the bars of both rows line up
    if let Some(toasts) = toasts {
        row.set_subtitle_selectable(true);
        row.add_suffix(&copy_button(toasts, subtitle_of(&row)));
    }
    row.add_suffix(&bar);
    row.add_suffix(&help::button(&tr(help)));
    (row, bar)
}

/// What [`copy_button`] copies from a row: its subtitle. Weak, the button is in the row, it would
/// keep it alive.
fn subtitle_of(row: &adw::ActionRow) -> impl Fn() -> Option<String> + 'static {
    let row = row.downgrade();
    move || {
        row.upgrade()
            .map(|row| row.subtitle().unwrap_or_default().into())
    }
}

/// A button that copies what `text` gives, and says so in a toast.
fn copy_button(
    toasts: &adw::ToastOverlay,
    text: impl Fn() -> Option<String> + 'static,
) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text(tr("copy"))
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    let toasts = toasts.downgrade();
    button.connect_clicked(move |button| {
        let Some(text) = text() else {
            return;
        };
        button.clipboard().set_text(&text);
        if let Some(toasts) = toasts.upgrade() {
            toasts.add_toast(adw::Toast::builder().title(tr("copied")).timeout(2).build());
        }
    });
    button
}

/// A level in dBFS as the fill of a meter, 0 to 1.
fn meter_fill(db: f64) -> f64 {
    ((db - METER_MIN_DB) / -METER_MIN_DB).clamp(0., 1.)
}

/// Reads the levels every [`POLL_LEVELS`] while the filter runs and shows them in the meters.
async fn poll_levels(widgets: Rc<Widgets>) {
    loop {
        glib::timeout_future(POLL_LEVELS).await;
        let proxy = widgets.proxy.borrow().clone();
        let levels = match proxy {
            Some(proxy) if widgets.running.get() => proxy.levels().await.ok(),
            _ => None,
        };
        let level = |name: &str| {
            levels
                .as_ref()
                .and_then(|levels| levels.get(name).copied())
                .map_or(0., meter_fill)
        };
        widgets.input_level.1.set_value(level("input"));
        widgets.output_level.1.set_value(level("output"));
    }
}

/// Whether the filter runs, and the noise floor of the microphone under its meter.
fn show_levels(widgets: &Widgets, status: &Status) {
    let frames = status.counters.get("frames").copied().unwrap_or_default();
    let running = frames != widgets.frames.replace(frames);
    widgets.running.set(running);
    let floor = status.levels.get("input-floor").copied().unwrap_or(-120.);
    let noisy_polls = if running && floor > NOISY_DB {
        widgets.noisy_polls.get().saturating_add(1)
    } else {
        0
    };
    widgets.noisy_polls.set(noisy_polls);
    let subtitle = if !running {
        tr("input-not-running")
    } else {
        let id = if noisy_polls >= NOISY_POLLS {
            "input-noise-loud"
        } else {
            "input-noise"
        };
        tr_args(id, &[("floor", format!("{floor:.0}").into())])
    };
    show_value(&widgets.input_level.0, &subtitle);
}

/// The stages stay adjustable while the filter is off, dimmed.
fn dim_stages(widgets: &Widgets, filter_on: bool) {
    for group in &widgets.groups {
        group.set_opacity(if filter_on { 1. } else { 0.5 });
    }
}

/// Sends what the user changes to the filter.
fn connect_controls(widgets: &Rc<Widgets>) {
    let weak = Rc::downgrade(widgets);
    widgets.enabled.connect_active_notify(move |switch| {
        let on = switch.is_active();
        // at once, not after the daemon reports it back; also for a state read from it
        if let Some(widgets) = weak.upgrade() {
            dim_stages(&widgets, on);
        }
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        send(
            "switch the filter",
            async move { proxy.set_enabled(on).await },
        );
    });
    let weak = Rc::downgrade(widgets);
    widgets.listen.connect_enable_expansion_notify(move |row| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let on = row.enables_expansion();
        send(
            "switch listening",
            async move { proxy.set_listen(on).await },
        );
    });
    let weak = Rc::downgrade(widgets);
    widgets.listen_volume.1.connect_value_changed(move |scale| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let db = scale.value();
        send("set the volume of listening", async move {
            proxy.set_listen_volume(db).await
        });
    });
    let weak = Rc::downgrade(widgets);
    widgets.keep_listen.connect_active_notify(move |row| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let keep = row.is_active();
        send("keep listening", async move {
            proxy.set_keep_listen(keep).await
        });
    });
    connect_device(widgets, Device::Input);
    connect_device(widgets, Device::Output);
}

/// The microphone or the output device.
#[derive(Clone, Copy)]
enum Device {
    Input,
    Output,
}

impl Device {
    /// Its row and the `node.name` per entry.
    fn picker(self, widgets: &Widgets) -> (&adw::ComboRow, &RefCell<Vec<String>>) {
        match self {
            Self::Input => (&widgets.input, &widgets.input_names),
            Self::Output => (&widgets.output, &widgets.output_names),
        }
    }
}

/// Sends the device the user picks.
fn connect_device(widgets: &Rc<Widgets>, device: Device) {
    let weak = Rc::downgrade(widgets);
    let (row, _) = device.picker(widgets);
    row.connect_selected_notify(move |row| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let Some(widgets) = weak.upgrade() else {
            return;
        };
        let (_, names) = device.picker(&widgets);
        let Some(name) = names.borrow().get(row.selected() as usize).cloned() else {
            return;
        };
        send(format!("switch to {name}"), async move {
            match device {
                Device::Input => proxy.set_input(&name).await,
                Device::Output => proxy.set_output(&name).await,
            }
        });
    });
}

/// Switches `stages` on and sets the parameters, given as stage, name and default, to their
/// defaults; the window shows it with the next poll.
fn reset(
    widgets: &::std::rc::Weak<Widgets>,
    stages: Vec<String>,
    defaults: Vec<(String, String, f64)>,
) {
    let Some(proxy) = sendable(widgets) else {
        return;
    };
    glib::spawn_future_local(async move {
        for stage in stages {
            if let Err(err) = proxy.set_stage_enabled(&stage, true).await {
                eprintln!("Could not switch {stage} on: {err}");
            }
        }
        for (stage, name, default) in defaults {
            if let Err(err) = proxy.set_parameter(&stage, &name, default).await {
                eprintln!("Could not reset {name} of {stage}: {err}");
            }
        }
    });
}

/// The main menu: resetting all stages and their settings, after asking.
fn main_menu(widgets: &Rc<Widgets>, window: &adw::ApplicationWindow) -> gtk::MenuButton {
    let action = gio::SimpleAction::new("reset", None);
    let (weak, parent) = (Rc::downgrade(widgets), window.downgrade());
    action.connect_activate(move |_, _| {
        let dialog = adw::MessageDialog::new(
            parent.upgrade().as_ref(),
            Some(&tr("reset-heading")),
            Some(&tr("reset-body")),
        );
        dialog.add_response("cancel", &tr("reset-cancel"));
        dialog.add_response("reset", &tr("reset-confirm"));
        dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = weak.clone();
        dialog.connect_response(Some("reset"), move |_, _| {
            let Some(widgets) = weak.upgrade() else {
                return;
            };
            let stages = widgets
                .layout
                .borrow()
                .iter()
                .map(|(stage, _)| stage.clone())
                .collect();
            let defaults = widgets.defaults.borrow().clone();
            reset(&weak, stages, defaults);
        });
        dialog.present();
    });
    window.add_action(&action);
    let menu = gio::Menu::new();
    menu.append(Some(&tr("menu-reset")), Some("win.reset"));
    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("main-menu"))
        .primary(true)
        .menu_model(&menu)
        .build()
}

/// Sends switching a stage on or off.
fn send_enabled(widgets: &::std::rc::Weak<Widgets>, stage: &str, enabled: bool) {
    let Some(proxy) = sendable(widgets) else {
        return;
    };
    let stage = stage.to_owned();
    send(format!("switch {stage}"), async move {
        proxy.set_stage_enabled(&stage, enabled).await
    });
}

/// Sends a change to the filter in the background; prints why it failed, after "Could not".
fn send(
    what: impl ::std::fmt::Display + 'static,
    change: impl Future<Output = ::zbus::Result<()>> + 'static,
) {
    glib::spawn_future_local(async move {
        if let Err(err) = change.await {
            eprintln!("Could not {what}: {err}");
        }
    });
}

/// The digits a step needs: 0 for 1 and above, 1 for 0.1 to 0.5 …
fn digits(step: f64) -> u32 {
    (-step.log10()).ceil().max(0.) as u32
}

fn format_value(value: f64, digits: u32, unit: &str) -> String {
    let digits = digits as usize;
    // a ratio as 3.0:1, everything else with a space
    let space = if unit.starts_with(':') { "" } else { " " };
    format!("{value:.digits$}{space}{unit}")
}

/// The volume of Listen as a parameter for [`slider`], as the daemon has it.
fn listen_volume() -> Parameter {
    let param = crate::volume::param();
    Parameter {
        stage: "listen".to_owned(),
        name: param.name.to_owned(),
        title: param.title.to_owned(),
        unit: param.unit.to_owned(),
        min: param.min as f64,
        max: param.max as f64,
        step: param.step as f64,
        default: param.default as f64,
        value: param.get() as f64,
    }
}

/// The slider of a parameter, the default marked: its value and the scale, in a box for the row,
/// and the scale.
///
/// Both have a fixed width, the same in every row, so the sliders line up: expanding, the row
/// shared its width between the title and the slider by their natural widths, a long title made
/// the slider shorter, and the value drawn by the scale itself made it shorter by the width of
/// the value's text.
fn slider(param: &Parameter) -> (gtk::Box, gtk::Scale) {
    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        param.min,
        param.max,
        param.step,
    );
    let digits = digits(param.step);
    scale.set_digits(digits as i32);
    scale.add_mark(param.default, gtk::PositionType::Bottom, None);
    scale.set_width_request(SLIDER_WIDTH);
    scale.set_valign(gtk::Align::Center);
    // right-aligned next to the scale, with digits of the same width, so it doesn't jump
    let value = gtk::Label::builder()
        .width_chars(VALUE_CHARS)
        .xalign(1.)
        .valign(gtk::Align::Center)
        .css_classes(["numeric"])
        .build();
    let unit = param.unit.clone();
    scale.connect_value_changed(glib::clone!(
        #[weak]
        value,
        move |scale| value.set_label(&format_value(scale.value(), digits, &unit))
    ));
    scale.set_value(param.value);
    // set_value doesn't notify when the value is already the scale's minimum
    value.set_label(&format_value(scale.value(), digits, &param.unit));
    let slider = gtk::Box::builder().spacing(6).build();
    slider.append(&value);
    slider.append(&scale);
    (slider, scale)
}

/// The slider of a parameter of a stage, sending what the user sets.
fn scale(widgets: &::std::rc::Weak<Widgets>, param: &Parameter) -> (gtk::Box, gtk::Scale) {
    let (slider, scale) = slider(param);
    let (stage, name) = (param.stage.clone(), param.name.clone());
    let weak = widgets.clone();
    scale.connect_value_changed(move |scale| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let (stage, name, value) = (stage.clone(), name.clone(), scale.value());
        send(format!("set {name} of {stage}"), async move {
            proxy.set_parameter(&stage, &name, value).await
        });
    });
    (slider, scale)
}

/// Builds the rows of the stages for the stages and parameters of `status`, unless they were
/// built for the same ones.
fn build_stages(widgets: &Rc<Widgets>, status: &Status) {
    let layout: Vec<(String, Vec<String>)> = status
        .stages
        .iter()
        .map(|(stage, _)| {
            let params = status
                .parameters
                .iter()
                .filter(|param| param.stage == *stage);
            (
                stage.clone(),
                params.map(|param| param.name.clone()).collect(),
            )
        })
        .collect();
    if *widgets.layout.borrow() == layout {
        return;
    }
    let weak = Rc::downgrade(widgets);
    for (_, StageRow { row, .. }, group) in widgets.stages.take() {
        // the rows of the output device and the status move to the new row of the echo
        // cancellation
        for moving in [
            widgets.output.upcast_ref::<gtk::Widget>(),
            widgets.echo_status.upcast_ref(),
        ] {
            if moving.parent().is_some() && moving.is_ancestor(&row) {
                row.remove(moving);
            }
        }
        group.remove(&row);
    }
    let mut stages = Vec::new();
    for ((_, names), group) in GROUPS.iter().zip(&widgets.groups) {
        for name in *names {
            if !status.stages.iter().any(|(stage, _)| stage == name) {
                continue;
            }
            let title = stage_title(name);
            let params: Vec<&Parameter> = status
                .parameters
                .iter()
                .filter(|param| param.stage == *name)
                .collect();
            let row = adw::ExpanderRow::builder()
                .title(title.as_str())
                .show_enable_switch(true)
                .build();
            if let Some(text) = help::stage(name) {
                row.add_row(&help::text_row(&text));
            }
            let scales = params
                .iter()
                .map(|param| {
                    let (slider, scale) = scale(&weak, param);
                    // the daemon's title is English, the window's in the user's language
                    let param_title = lookup(&format!("param-{name}-{}", param.name), &[])
                        .unwrap_or_else(|| param.title.clone());
                    let param_row = adw::ActionRow::builder().title(param_title).build();
                    param_row.add_suffix(&slider);
                    if let Some(text) = help::param(name, &param.name) {
                        param_row.add_suffix(&help::button(&text));
                    }
                    row.add_row(&param_row);
                    (param.name.clone(), scale)
                })
                .collect();
            // the echo cancellation also holds its output device and how it does
            if *name == "echo" {
                row.add_row(&widgets.output);
                row.add_row(&widgets.echo_status);
            }
            let defaults = params
                .iter()
                .map(|param| (name.to_string(), param.name.clone(), param.default))
                .collect::<Vec<_>>();
            let reset_button = gtk::Button::builder()
                .label(tr("reset"))
                .valign(gtk::Align::Center)
                .build();
            let weak_reset = weak.clone();
            reset_button.connect_clicked(move |_| reset(&weak_reset, Vec::new(), defaults.clone()));
            let reset_row = adw::ActionRow::builder()
                .title(tr("default-settings"))
                .build();
            reset_row.add_suffix(&reset_button);
            row.add_row(&reset_row);
            let (weak, name) = (weak.clone(), *name);
            row.connect_enable_expansion_notify(move |row| {
                let on = row.enables_expansion();
                // libadwaita expands the row when its switch turns on; switching a stage
                // shouldn't open its parameters (switched off, it was collapsed).
                if on {
                    row.set_expanded(false);
                }
                send_enabled(&weak, name, on);
            });
            group.add(&row);
            let row = StageRow {
                row,
                scales,
                reset: reset_button,
            };
            stages.push((name.to_string(), row, group.clone()));
        }
    }
    widgets.stages.replace(stages);
    widgets.layout.replace(layout);
    widgets.defaults.replace(
        status
            .parameters
            .iter()
            .map(|param| (param.stage.clone(), param.name.clone(), param.default))
            .collect(),
    );
}

/// The proxy to send a change with, unless the change comes from showing a read value.
fn sendable(widgets: &::std::rc::Weak<Widgets>) -> Option<VoiceFilterProxy<'static>> {
    let widgets = widgets.upgrade()?;
    if widgets.updating.get() {
        return None;
    }
    widgets.proxy.borrow().clone()
}

/// Reads the state every [`POLL`] and shows it, or that the filter isn't running.
async fn poll(widgets: Rc<Widgets>) {
    let connection = match ::zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(err) => {
            widgets.missing.set_description(Some(&tr_args(
                "missing-no-bus",
                &[("error", err.to_string().into())],
            )));
            return;
        }
    };
    // the status properties don't emit change signals, cached values would never change
    let proxy = match VoiceFilterProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
    {
        Ok(proxy) => proxy,
        Err(err) => {
            widgets.missing.set_description(Some(&tr_args(
                "missing-no-proxy",
                &[("error", err.to_string().into())],
            )));
            return;
        }
    };
    widgets.proxy.replace(Some(proxy.clone()));
    loop {
        match proxy.status().await {
            Ok(status) => {
                widgets.updating.set(true);
                build_stages(&widgets, &status);
                widgets.updating.set(false);
                show(&widgets, &status);
                widgets.stack.set_visible_child_name("controls");
            }
            Err(_) => widgets.stack.set_visible_child_name("missing"),
        }
        glib::timeout_future(POLL).await;
    }
}

/// The devices to choose from, the chosen one selected; one that's unplugged stays in the list,
/// so the row shows what the filter waits for.
fn show_devices(widgets: &Widgets, device: Device, status: &Status) {
    let (devices, chosen, default) = match device {
        Device::Input => (&status.inputs, &status.input, tr("default-input")),
        Device::Output => (&status.outputs, &status.output, tr("default-output")),
    };
    let (row, known) = device.picker(widgets);
    let mut names = vec![String::new()];
    let mut labels = vec![default];
    for (name, description, _) in devices {
        names.push(name.clone());
        labels.push(description.clone());
    }
    if !names.contains(chosen) {
        names.push(chosen.clone());
        labels.push(tr_args("unplugged", &[("device", chosen.clone().into())]));
    }
    if *known.borrow() != names {
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        row.set_model(Some(&gtk::StringList::new(&labels)));
        known.replace(names.clone());
    }
    let selected = names
        .iter()
        .position(|name| name == chosen)
        .unwrap_or_default() as u32;
    if row.selected() != selected {
        row.set_selected(selected);
    }
}

/// What the echo cancellation does: whether it runs and hears the speakers, how loud, and the delay
/// of the echo it estimates. Not AEC3's ERLE: it only covers its linear filter, a few dB, while
/// what it removes in all was 30 dB on a recording that reported 6.
fn show_echo(widgets: &Widgets, status: &Status) {
    let value = |name: &str| status.echo.get(name).copied();
    let (Some(capture), Some(reference)) = (value("capture-frames"), value("reference-frames"))
    else {
        show_value(&widgets.echo_status, &tr("echo-not-in-chain"));
        return;
    };
    let (last_capture, last_reference) = widgets.echo_frames.replace((capture, reference));
    let level = value("reference").unwrap_or(-120.);
    let subtitle = if capture == last_capture {
        tr("echo-not-running")
    } else if reference == last_reference || level < -60. {
        tr("echo-silent")
    } else {
        // AEC3 estimates in blocks of 4 ms
        let delay = match value("delay").unwrap_or_default() {
            delay if delay < 4. => tr("echo-delay-short"),
            delay => tr_args("echo-delay", &[("delay", format!("{delay:.0}").into())]),
        };
        tr_args(
            "echo-hears",
            &[
                ("level", format!("{level:.0}").into()),
                ("delay", delay.into()),
            ],
        )
    };
    show_value(&widgets.echo_status, &subtitle);
}

fn show(widgets: &Widgets, status: &Status) {
    widgets.updating.set(true);
    for (name, StageRow { row, scales, reset }, _) in widgets.stages.borrow().iter() {
        if let Some((_, enabled)) = status.stages.iter().find(|(stage, _)| stage == name) {
            row.set_enable_expansion(*enabled);
        }
        let changed = status.parameters.iter().any(|param| {
            param.stage == *name && (param.value - param.default).abs() >= param.step / 2.
        });
        reset.set_sensitive(changed);
        for (param_name, scale) in scales {
            let Some(param) = status
                .parameters
                .iter()
                .find(|param| param.stage == *name && param.name == *param_name)
            else {
                continue;
            };
            // not while the user drags it, the read value lags behind by up to a poll
            if (scale.value() - param.value).abs() >= param.step / 2. && !scale.has_focus() {
                scale.set_value(param.value);
            }
        }
    }
    show_devices(widgets, Device::Input, status);
    show_devices(widgets, Device::Output, status);
    show_echo(widgets, status);
    show_levels(widgets, status);
    widgets.enabled.set_active(status.enabled);
    // the switch dims them when it changes; this covers a first state that's already off
    dim_stages(widgets, widgets.enabled.is_active());
    widgets.listen.set_enable_expansion(status.listen);
    widgets.keep_listen.set_active(status.keep_listen);
    let volume = &widgets.listen_volume.1;
    if (volume.value() - status.listen_volume).abs() >= 0.5 && !volume.has_focus() {
        volume.set_value(status.listen_volume);
    }
    widgets.updating.set(false);

    let ms = |part: &str| {
        let ms = status.latency.get(part).copied().unwrap_or_default();
        FluentValue::from(format!("{ms:.1}"))
    };
    show_value(
        &widgets.latency,
        &tr_args(
            "latency-value",
            &[
                ("total", ms("total")),
                ("microphone", ms("microphone")),
                ("bridge", ms("bridge")),
                ("chain", ms("chain")),
            ],
        ),
    );
    show_processing(widgets, status);
    let count = |name: &str| status.counters.get(name).copied().unwrap_or_default();
    // the counters are in samples, which say little to the user
    let count_ms = |name: &str| samples_ms(count(name) as f64);
    show_value(
        &widgets.dropouts,
        &tr_args(
            "dropouts-value",
            &[
                ("silence", format!("{:.0}", count_ms("silence")).into()),
                ("dropped", format!("{:.0}", count_ms("dropped")).into()),
                // a number, for the plural
                ("errors", count("errors").into()),
            ],
        ),
    );
}

/// The columns of the table of the processing time, after the part: the ids of their titles.
const PROCESSING_COLUMNS: [&str; 4] = [
    "processing-mean",
    "processing-p95",
    "processing-p99",
    "processing-longest",
];

/// The table the row of the processing time expands to, a row per part.
struct ProcessingTable {
    grid: gtk::Grid,
    /// Part, its title and its values, in the order of the grid.
    rows: RefCell<Vec<(String, gtk::Label, [gtk::Label; PROCESSING_COLUMNS.len()])>>,
    /// The samples per cycle, below the table.
    cycle: gtk::Label,
    /// The table as text, for the copy button.
    text: Rc<RefCell<String>>,
}

/// The row of the processing time: a summary, expanding to a table of the parts with a column
/// each for mean, percentiles and the longest; the copy button copies the table as text.
fn processing_row(toasts: &adw::ToastOverlay) -> (adw::ExpanderRow, ProcessingTable) {
    let row = adw::ExpanderRow::builder().title(tr("processing")).build();
    let grid = gtk::Grid::builder()
        .column_spacing(18)
        .row_spacing(6)
        .build();
    for (column, title) in PROCESSING_COLUMNS.into_iter().enumerate() {
        let label = gtk::Label::builder()
            .label(tr(title))
            .xalign(1.)
            .css_classes(["caption-heading", "dim-label"])
            .build();
        grid.attach(&label, column as i32 + 1, 0, 1, 1);
    }
    let cycle = gtk::Label::builder()
        .xalign(0.)
        .wrap(true)
        .css_classes(["caption", "dim-label"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let text = Rc::new(RefCell::new(String::new()));
    let copied = text.clone();
    // the cycle below the table, the copy button at its end
    let footer = gtk::Box::builder().spacing(12).build();
    cycle.set_hexpand(true);
    footer.append(&cycle);
    footer.append(&copy_button(toasts, move || Some(copied.borrow().clone())));
    content.append(&grid);
    content.append(&footer);
    row.add_row(&help::text_row(&tr(help::PROCESSING)));
    row.add_row(
        &gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .child(&content)
            .build(),
    );
    let table = ProcessingTable {
        grid,
        rows: RefCell::new(Vec::new()),
        cycle,
        text,
    };
    (row, table)
}

/// The title of a part of the processing time.
fn part_title(part: &str) -> String {
    match part {
        "total" => tr("processing-all"),
        "other" => tr("processing-other"),
        part => stage_title(part),
    }
}

/// Shows the processing time: the summary of all stages in the subtitle, the parts in the table,
/// whose rows are rebuilt only when the parts change.
fn show_processing(widgets: &Widgets, status: &Status) {
    let table = &widgets.processing_table;
    let mut rows = table.rows.borrow_mut();
    let parts = status.frame_times.iter().map(|(part, ..)| part.as_str());
    if !rows.iter().map(|(part, ..)| part.as_str()).eq(parts) {
        for (_, title, values) in rows.drain(..) {
            table.grid.remove(&title);
            for value in values {
                table.grid.remove(&value);
            }
        }
        for (index, (part, ..)) in status.frame_times.iter().enumerate() {
            let line = index as i32 + 1;
            let title = gtk::Label::builder()
                .label(part_title(part))
                .xalign(0.)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            table.grid.attach(&title, 0, line, 1, 1);
            // digits of the same width, so the columns line up and don't jump
            let values = ::std::array::from_fn(|column| {
                let value = gtk::Label::builder()
                    .xalign(1.)
                    .css_classes(["numeric"])
                    .build();
                table.grid.attach(&value, column as i32 + 1, line, 1, 1);
                value
            });
            rows.push((part.clone(), title, values));
        }
    }
    let columns: Vec<String> = PROCESSING_COLUMNS.iter().map(|id| tr(id)).collect();
    let mut text = tr_args(
        "processing-copied",
        &[("columns", columns.join(", ").into())],
    );
    for ((_, _, labels), (part, mean, p95, p99, max)) in rows.iter().zip(&status.frame_times) {
        let values = [mean, p95, p99, max].map(|ms| format!("{ms:.2}"));
        for (label, value) in labels.iter().zip(&values) {
            label.set_label(value);
        }
        text += &format!("\n{}: {}", part_title(part), values.join(", "));
    }
    let cycle = tr_args("processing-cycle", &[("samples", status.quantum.into())]);
    table.cycle.set_label(&cycle);
    *table.text.borrow_mut() = format!("{text}\n{cycle}");
    let total = status
        .frame_times
        .iter()
        .find(|(part, ..)| part == "total")
        .filter(|(.., max)| *max > 0.);
    widgets.processing.set_subtitle(&match total {
        Some((_, mean, .., max)) => {
            let (mean, max) = (format!("{mean:.2}"), format!("{max:.2}"));
            tr_args(
                "processing-summary",
                &[("mean", mean.into()), ("max", max.into())],
            )
        }
        None => tr("processing-nothing"),
    });
}

/// Shows a value in the subtitle of a row, which the user can select and copy: not while they
/// select in it, the new text would drop the selection.
fn show_value(row: &adw::ActionRow, text: &str) {
    let selecting = subtitle_label(row.upcast_ref())
        .is_some_and(|label| label.has_focus() && label.selection_bounds().is_some());
    if !selecting {
        row.set_subtitle(text);
    }
}

/// The label of a row's subtitle, the one with the style class `subtitle`.
fn subtitle_label(widget: &gtk::Widget) -> Option<gtk::Label> {
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk::Label>()
            && label.has_css_class("subtitle")
        {
            return Some(label.clone());
        }
        if let Some(label) = subtitle_label(&widget) {
            return Some(label);
        }
        child = widget.next_sibling();
    }
    None
}
