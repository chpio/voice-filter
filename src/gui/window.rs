use ::std::cell::{Cell, RefCell};
use ::std::rc::Rc;
use ::std::time::Duration;

use ::adw::prelude::*;
use ::gtk::glib;
use ::zbus::proxy::CacheProperties;

use super::proxy::{Parameter, Status, VoiceFilterProxy};

/// The stages the window shows: name in the chain and title; what they do is in the descriptions
/// of their groups, so the rows stay short.
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

/// The row of a stage: a switch, or with parameters a row that expands to them, its switch turns
/// the stage on and off.
enum StageRow {
    Switch(adw::SwitchRow),
    Expander {
        row: adw::ExpanderRow,
        /// Parameter name and its scale.
        scales: Vec<(String, gtk::Scale)>,
    },
}

impl StageRow {
    fn widget(&self) -> &gtk::Widget {
        match self {
            Self::Switch(row) => row.upcast_ref(),
            Self::Expander { row, .. } => row.upcast_ref(),
        }
    }

    fn set_on(&self, on: bool) {
        match self {
            Self::Switch(row) => row.set_active(on),
            Self::Expander { row, .. } => row.set_enable_expansion(on),
        }
    }
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
    listen: adw::SwitchRow,
    latency: adw::ActionRow,
    processing: adw::ActionRow,
    dropouts: adw::ActionRow,
    /// Set while the window shows values it read, so the handlers don't send them back.
    updating: Cell<bool>,
    proxy: RefCell<Option<VoiceFilterProxy<'static>>>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let row = |title: &str| adw::ActionRow::builder().title(title).build();
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
        listen: adw::SwitchRow::builder()
            .title("Listen")
            .subtitle("Plays it on the default output, use headphones")
            .build(),
        latency: row("Latency"),
        processing: row("Processing Time"),
        dropouts: row("Dropouts Since the Start"),
        updating: Cell::new(false),
        proxy: RefCell::new(None),
    });

    let group = |title: &str, description: Option<&str>, rows: &[&gtk::Widget]| {
        let group = adw::PreferencesGroup::builder().title(title).build();
        group.set_description(description);
        for row in rows {
            group.add(*row);
        }
        group
    };
    let page = adw::PreferencesPage::new();
    page.add(&group(
        "Input",
        Some("The filter reads from this microphone, apps read from “Voice Filter”."),
        &[widgets.input.upcast_ref(), widgets.listen.upcast_ref()],
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
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&widgets.stack));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Voice Filter")
        .default_width(480)
        .default_height(720)
        .content(&toolbar)
        .build();

    connect_controls(&widgets);
    glib::spawn_future_local(poll(widgets));
    window
}

/// Sends what the user changes to the filter.
fn connect_controls(widgets: &Rc<Widgets>) {
    let weak = Rc::downgrade(widgets);
    widgets.listen.connect_active_notify(move |row| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let on = row.is_active();
        glib::spawn_future_local(async move {
            if let Err(err) = proxy.set_listen(on).await {
                eprintln!("Could not switch listening: {err}");
            }
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
        glib::spawn_future_local(async move {
            let result = match device {
                Device::Input => proxy.set_input(&name).await,
                Device::Output => proxy.set_output(&name).await,
            };
            if let Err(err) = result {
                eprintln!("Could not switch to {name}: {err}");
            }
        });
    });
}

/// Sends switching a stage on or off.
fn send_enabled(widgets: &::std::rc::Weak<Widgets>, stage: &str, enabled: bool) {
    let Some(proxy) = sendable(widgets) else {
        return;
    };
    let stage = stage.to_owned();
    glib::spawn_future_local(async move {
        if let Err(err) = proxy.set_stage_enabled(&stage, enabled).await {
            eprintln!("Could not switch {stage}: {err}");
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

/// The slider of a parameter, the default marked, sending what the user sets.
fn scale(widgets: &::std::rc::Weak<Widgets>, param: &Parameter) -> gtk::Scale {
    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        param.min,
        param.max,
        param.step,
    );
    let digits = digits(param.step);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Left);
    scale.set_digits(digits as i32);
    let unit = param.unit.clone();
    scale.set_format_value_func(move |_, value| format_value(value, digits, &unit));
    scale.add_mark(param.default, gtk::PositionType::Bottom, None);
    scale.set_width_request(220);
    scale.set_hexpand(true);
    scale.set_valign(gtk::Align::Center);
    scale.set_value(param.value);
    let (stage, name) = (param.stage.clone(), param.name.clone());
    let weak = widgets.clone();
    scale.connect_value_changed(move |scale| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let (stage, name, value) = (stage.clone(), name.clone(), scale.value());
        glib::spawn_future_local(async move {
            if let Err(err) = proxy.set_parameter(&stage, &name, value).await {
                eprintln!("Could not set {name} of {stage}: {err}");
            }
        });
    });
    scale
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
    for (_, row, group) in widgets.stages.take() {
        // the rows of the output device and the status move to the new row of the echo
        // cancellation
        if let StageRow::Expander { row, .. } = &row {
            for moving in [
                widgets.output.upcast_ref::<gtk::Widget>(),
                widgets.echo_status.upcast_ref(),
            ] {
                if moving.parent().is_some() && moving.is_ancestor(row) {
                    row.remove(moving);
                }
            }
        }
        group.remove(row.widget());
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
            // the echo cancellation expands to its output device, even without parameters
            let row = if params.is_empty() && *name != "echo" {
                let row = adw::SwitchRow::builder().title(title).build();
                let (weak, name) = (weak.clone(), *name);
                row.connect_active_notify(move |row| send_enabled(&weak, name, row.is_active()));
                StageRow::Switch(row)
            } else {
                let row = adw::ExpanderRow::builder()
                    .title(title)
                    .show_enable_switch(true)
                    .build();
                let scales = params
                    .iter()
                    .map(|param| {
                        let scale = scale(&weak, param);
                        let param_row = adw::ActionRow::builder().title(&param.title).build();
                        param_row.add_suffix(&scale);
                        row.add_row(&param_row);
                        (param.name.clone(), scale)
                    })
                    .collect();
                if *name == "echo" {
                    row.add_row(&widgets.output);
                    row.add_row(&widgets.echo_status);
                }
                let (weak, name) = (weak.clone(), *name);
                row.connect_enable_expansion_notify(move |row| {
                    send_enabled(&weak, name, row.enables_expansion());
                });
                StageRow::Expander { row, scales }
            };
            group.add(row.widget());
            stages.push((name.to_string(), row, group.clone()));
        }
    }
    widgets.stages.replace(stages);
    widgets.layout.replace(layout);
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
        widgets.echo_status.set_subtitle("Not in the chain");
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
    widgets.echo_status.set_subtitle(&subtitle);
}

fn show(widgets: &Widgets, status: &Status) {
    widgets.updating.set(true);
    for (name, row, _) in widgets.stages.borrow().iter() {
        if let Some((_, enabled)) = status.stages.iter().find(|(stage, _)| stage == name) {
            row.set_on(*enabled);
        }
        let StageRow::Expander { scales, .. } = row else {
            continue;
        };
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
    widgets.listen.set_active(status.listen);
    widgets.updating.set(false);

    let ms = |part: &str| status.latency.get(part).copied().unwrap_or_default();
    widgets.latency.set_subtitle(&format!(
        "{:.1} ms until the app: microphone {:.1}, buffer {:.1}, processing {:.1}",
        ms("total"),
        ms("microphone"),
        ms("bridge"),
        ms("chain"),
    ));
    widgets.processing.set_subtitle(&format!(
        "Up to {:.1} ms per 10 ms frame, {} samples per cycle",
        status.max_frame_time, status.quantum
    ));
    let count = |name: &str| status.counters.get(name).copied().unwrap_or_default();
    widgets.dropouts.set_subtitle(&format!(
        "{} samples of silence, {} dropped, {} errors",
        count("silence"),
        count("dropped"),
        count("errors"),
    ));
}
