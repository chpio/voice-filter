use ::std::cell::{Cell, RefCell};
use ::std::rc::Rc;
use ::std::time::Duration;

use ::adw::prelude::*;
use ::gtk::{gio, glib};
use ::zbus::proxy::CacheProperties;

use super::help;
use super::proxy::{Parameter, Status, VoiceFilterProxy};
use crate::samples_ms;

/// The stages the window shows: name in the chain and title; what they do is in the descriptions
/// of their groups and behind the info button of their row ([`help`]), so the rows stay short.
const STAGES: [(&str, &str); 9] = [
    ("echo", "Echo Cancellation"),
    ("denoise", "Noise Suppression"),
    ("highpass", "High-Pass Filter"),
    ("eq", "Equalizer"),
    ("deesser", "De-Esser"),
    ("agc", "Automatic Gain Control"),
    ("compressor", "Compressor"),
    ("expander", "Expander"),
    ("limiter", "Limiter"),
];

/// The groups of the stages: title, description and their stages.
const GROUPS: [(&str, &str, &[&str]); 4] = [
    (
        "Echo Cancellation",
        "Removes the echo of the speakers chosen here from the microphone, for calls without \
         headphones.",
        &["echo"],
    ),
    (
        "Noise Suppression",
        "Removes background noise. The limit keeps part of it: lower sounds more natural, 100 dB \
         removes it all.",
        &["denoise"],
    ),
    (
        "Tone",
        "Removes rumble, makes the voice clearer (less boom in the low mids, more presence) and \
         tames harsh S sounds.",
        &["highpass", "eq", "deesser"],
    ),
    (
        "Level",
        "Brings the voice to a constant level, evens out the loudness within words, keeps the \
         noise down in speech pauses and the peaks below −1 dBFS.",
        &["agc", "compressor", "expander", "limiter"],
    ),
];

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
    processing: adw::ActionRow,
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
    // a row whose subtitle shows a value, selectable and with a button that copies it
    let row = |title: &str| {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle_selectable(true)
            .build();
        row.add_suffix(&copy_button(&row, &toasts));
        row
    };
    // a status row with an info button
    let status_row = |title: &str, text: &str| {
        let row = row(title);
        row.add_suffix(&help::button(text));
        row
    };
    let widgets = Rc::new(Widgets {
        stack: gtk::Stack::new(),
        missing: adw::StatusPage::builder()
            .icon_name("audio-input-microphone-symbolic")
            .title("Voice Filter Isn't Running")
            .description("Start it with <tt>voice-filter run</tt>")
            .build(),
        groups: GROUPS
            .iter()
            .map(|(title, description, _)| {
                adw::PreferencesGroup::builder()
                    .title(*title)
                    .description(*description)
                    .build()
            })
            .collect(),
        stages: RefCell::new(Vec::new()),
        layout: RefCell::new(Vec::new()),
        // the chosen microphone as the subtitle, over the width of the row, instead of squeezed
        // next to the title
        input: adw::ComboRow::builder()
            .title("Microphone")
            .use_subtitle(true)
            .model(&gtk::StringList::new(&[]))
            .build(),
        input_names: RefCell::new(Vec::new()),
        output: adw::ComboRow::builder()
            .title("Speakers")
            .use_subtitle(true)
            .model(&gtk::StringList::new(&[]))
            .build(),
        output_names: RefCell::new(Vec::new()),
        echo_status: row("Status"),
        echo_frames: Cell::new((0., 0.)),
        listen: adw::ExpanderRow::builder()
            .title("Listen")
            .subtitle("Plays it on the default output, use headphones")
            .show_enable_switch(true)
            .build(),
        listen_volume: slider(&listen_volume()),
        keep_listen: adw::SwitchRow::builder()
            .title("Keep Listening")
            .subtitle(
                "Listens again after a restart. The microphone stays in use and the filter runs \
                 all the time.",
            )
            .build(),
        input_level: meter_row("Microphone", help::INPUT_LEVEL, Some(&toasts)),
        output_level: meter_row("Voice Filter", help::OUTPUT_LEVEL, None),
        running: Cell::new(false),
        frames: Cell::new(0),
        noisy_polls: Cell::new(0),
        latency: status_row("Latency", help::LATENCY),
        processing: status_row("Processing Time", help::PROCESSING),
        dropouts: status_row("Dropouts Since the Start", help::DROPOUTS),
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
    let volume_row = adw::ActionRow::builder().title("Volume").build();
    volume_row.add_suffix(&widgets.listen_volume.0);
    volume_row.add_suffix(&help::button(help::LISTEN_VOLUME));
    widgets.listen.add_row(&volume_row);
    widgets.listen.add_row(&widgets.keep_listen);
    let page = adw::PreferencesPage::new();
    page.add(&group(
        "Input",
        Some("The filter reads from this microphone, apps read from “Voice Filter”."),
        &[widgets.input.upcast_ref(), widgets.listen.upcast_ref()],
    ));
    page.add(&group(
        "Levels",
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
        "Status",
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
        .title("Voice Filter")
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
        row.add_suffix(&copy_button(&row, toasts));
    }
    row.add_suffix(&bar);
    row.add_suffix(&help::button(help));
    (row, bar)
}

/// A button that copies the value in the subtitle of a row, and says so in a toast.
fn copy_button(row: &adw::ActionRow, toasts: &adw::ToastOverlay) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy")
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    // weak: the button is in the row, it would keep it alive
    let (row, toasts) = (row.downgrade(), toasts.downgrade());
    button.connect_clicked(move |button| {
        let Some(row) = row.upgrade() else {
            return;
        };
        button
            .clipboard()
            .set_text(&row.subtitle().unwrap_or_default());
        if let Some(toasts) = toasts.upgrade() {
            toasts.add_toast(adw::Toast::builder().title("Copied").timeout(2).build());
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
        "Not running: no app records from “Voice Filter”".to_owned()
    } else if noisy_polls >= NOISY_POLLS {
        format!(
            "Background noise at {floor:.0} dBFS, very loud: the noise suppression leaves a hiss. \
             Lower the microphone boost, if it's up, or use a headset"
        )
    } else {
        format!("Background noise at {floor:.0} dBFS")
    };
    show_value(&widgets.input_level.0, &subtitle);
}

/// Sends what the user changes to the filter.
fn connect_controls(widgets: &Rc<Widgets>) {
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
            Some("Reset the Filter?"),
            Some(
                "Switches all stages on and sets their settings to the defaults. The microphone \
                 and the speakers stay as they are.",
            ),
        );
        dialog.add_response("cancel", "_Cancel");
        dialog.add_response("reset", "_Reset");
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
    menu.append(Some("_Reset the Filter…"), Some("win.reset"));
    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main Menu")
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
    for ((_, _, names), group) in GROUPS.iter().zip(&widgets.groups) {
        for name in *names {
            if !status.stages.iter().any(|(stage, _)| stage == name) {
                continue;
            }
            let title = STAGES
                .iter()
                .find(|(stage, _)| stage == name)
                .map_or(*name, |(_, title)| title);
            let params: Vec<&Parameter> = status
                .parameters
                .iter()
                .filter(|param| param.stage == *name)
                .collect();
            let row = adw::ExpanderRow::builder()
                .title(title)
                .show_enable_switch(true)
                .build();
            if let Some(text) = help::stage(name) {
                row.add_suffix(&help::button(text));
            }
            let scales = params
                .iter()
                .map(|param| {
                    let (slider, scale) = scale(&weak, param);
                    let param_row = adw::ActionRow::builder().title(&param.title).build();
                    param_row.add_suffix(&slider);
                    if let Some(text) = help::param(name, &param.name) {
                        param_row.add_suffix(&help::button(text));
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
                .label("Reset")
                .valign(gtk::Align::Center)
                .build();
            let weak_reset = weak.clone();
            reset_button.connect_clicked(move |_| reset(&weak_reset, Vec::new(), defaults.clone()));
            let reset_row = adw::ActionRow::builder().title("Default Settings").build();
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
            widgets.missing.set_description(Some(&format!(
                "Could not connect to the session bus: {err}"
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
            widgets
                .missing
                .set_description(Some(&format!("Could not create the proxy: {err}")));
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
        Device::Input => (&status.inputs, &status.input, "Default Input"),
        Device::Output => (&status.outputs, &status.output, "Default Output"),
    };
    let (row, known) = device.picker(widgets);
    let mut names = vec![String::new()];
    let mut labels = vec![default.to_owned()];
    for (name, description, _) in devices {
        names.push(name.clone());
        labels.push(description.clone());
    }
    if !names.contains(chosen) {
        names.push(chosen.clone());
        labels.push(format!("{chosen} (unplugged)"));
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
        show_value(&widgets.echo_status, "Not in the chain");
        return;
    };
    let (last_capture, last_reference) = widgets.echo_frames.replace((capture, reference));
    let level = value("reference").unwrap_or(-120.);
    let subtitle = if capture == last_capture {
        "Not running: switched off, or no app records from “Voice Filter”".to_owned()
    } else if reference == last_reference || level < -60. {
        "The speakers are silent, there's no echo to remove".to_owned()
    } else {
        // AEC3 estimates in blocks of 4 ms
        let delay = match value("delay").unwrap_or_default() {
            delay if delay < 4. => "less than 4 ms".to_owned(),
            delay => format!("about {delay:.0} ms"),
        };
        format!("Hears the speakers at {level:.0} dBFS, their echo comes back after {delay}")
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
    widgets.listen.set_enable_expansion(status.listen);
    widgets.keep_listen.set_active(status.keep_listen);
    let volume = &widgets.listen_volume.1;
    if (volume.value() - status.listen_volume).abs() >= 0.5 && !volume.has_focus() {
        volume.set_value(status.listen_volume);
    }
    widgets.updating.set(false);

    let ms = |part: &str| status.latency.get(part).copied().unwrap_or_default();
    show_value(
        &widgets.latency,
        &format!(
            "{:.1} ms until the app: microphone {:.1}, buffer {:.1}, processing {:.1}",
            ms("total"),
            ms("microphone"),
            ms("bridge"),
            ms("chain"),
        ),
    );
    show_value(
        &widgets.processing,
        &processing_text(&status.frame_times, status.quantum),
    );
    let count = |name: &str| status.counters.get(name).copied().unwrap_or_default();
    // the counters are in samples, which say little to the user
    let count_ms = |name: &str| samples_ms(count(name) as f64);
    show_value(
        &widgets.dropouts,
        &format!(
            "{:.0} ms of silence, {:.0} ms dropped, {} errors",
            count_ms("silence"),
            count_ms("dropped"),
            count("errors"),
        ),
    );
}

/// The processing time of a frame in its parts, a line each: mean, 95 % and 99 % percentile and
/// the longest, in ms.
fn processing_text(times: &[(String, f64, f64, f64, f64)], quantum: u64) -> String {
    let mut text = String::from("Per 10 ms frame in ms: mean, 95 %, 99 %, longest");
    for (part, mean, p95, p99, max) in times {
        let title = match part.as_str() {
            "total" => "All stages",
            "other" => "The other stages",
            part => STAGES
                .iter()
                .find(|(stage, _)| *stage == part)
                .map_or(part, |(_, title)| title),
        };
        text += &format!("\n{title}: {mean:.2}, {p95:.2}, {p99:.2}, {max:.2}");
    }
    text + &format!("\n{quantum} samples per cycle")
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
