use ::std::cell::{Cell, RefCell};
use ::std::rc::Rc;
use ::std::time::Duration;

use ::adw::prelude::*;
use ::gtk::glib;
use ::zbus::proxy::CacheProperties;

use super::proxy::{Status, VoiceFilterProxy};

/// The stages the window has a switch for: name in the chain and title; what they do is in the
/// descriptions of their groups, so the rows stay short.
const STAGES: [(&str, &str); 5] = [
    ("denoise", "Noise Suppression"),
    ("highpass", "High-Pass Filter"),
    ("eq", "Equalizer"),
    ("agc", "Automatic Gain Control"),
    ("limiter", "Limiter"),
];

/// How often the window reads the state; the status properties don't emit change signals.
const POLL: Duration = Duration::from_secs(1);

struct Widgets {
    stack: gtk::Stack,
    missing: adw::StatusPage,
    switches: Vec<(&'static str, adw::SwitchRow)>,
    atten_lim: gtk::Scale,
    input: adw::ComboRow,
    /// The `node.name` per entry of `input`, the first is the default input (empty).
    input_names: RefCell<Vec<String>>,
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
    let atten_lim = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0., 100., 1.);
    atten_lim.set_draw_value(true);
    atten_lim.set_value_pos(gtk::PositionType::Left);
    atten_lim.set_digits(0);
    atten_lim.set_format_value_func(|_, db| format!("{db:.0} dB"));
    atten_lim.set_width_request(260);
    atten_lim.set_hexpand(true);
    atten_lim.set_valign(gtk::Align::Center);
    let widgets = Rc::new(Widgets {
        stack: gtk::Stack::new(),
        missing: adw::StatusPage::builder()
            .icon_name("audio-input-microphone-symbolic")
            .title("Voice Filter Isn't Running")
            .description("Start it with <tt>voice-filter run</tt>")
            .build(),
        switches: STAGES
            .iter()
            .map(|&(name, title)| (name, adw::SwitchRow::builder().title(title).build()))
            .collect(),
        atten_lim,
        // the chosen microphone as the subtitle, over the width of the row, instead of squeezed
        // next to the title
        input: adw::ComboRow::builder()
            .title("Microphone")
            .use_subtitle(true)
            .model(&gtk::StringList::new(&[]))
            .build(),
        input_names: RefCell::new(Vec::new()),
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

    let atten_lim_row = adw::ActionRow::builder().title("Limit").build();
    atten_lim_row.add_suffix(&widgets.atten_lim);

    let group = |title: &str, description: Option<&str>, rows: &[&gtk::Widget]| {
        let group = adw::PreferencesGroup::builder().title(title).build();
        group.set_description(description);
        for row in rows {
            group.add(*row);
        }
        group
    };
    let switch = |name: &str| -> &gtk::Widget {
        let (_, row) = widgets
            .switches
            .iter()
            .find(|(stage, _)| *stage == name)
            .expect("a switch for every stage the window shows");
        row.upcast_ref()
    };
    let page = adw::PreferencesPage::new();
    page.add(&group(
        "Input",
        Some("The filter reads from this microphone, apps read from “Voice Filter”."),
        &[widgets.input.upcast_ref(), widgets.listen.upcast_ref()],
    ));
    page.add(&group(
        "Noise Suppression",
        Some(
            "Removes background noise. The limit keeps part of it: lower sounds more natural, \
             100 dB removes it all.",
        ),
        &[switch("denoise"), atten_lim_row.upcast_ref()],
    ));
    page.add(&group(
        "Tone",
        Some(
            "Removes rumble below 80 Hz and makes the voice clearer: less boom around 250 Hz, \
             more presence around 3.5 kHz.",
        ),
        &[switch("highpass"), switch("eq")],
    ));
    page.add(&group(
        "Level",
        Some("Brings the voice to a constant level and keeps its peaks below −1 dBFS."),
        &[switch("agc"), switch("limiter")],
    ));
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
    for (name, row) in &widgets.switches {
        let name = *name;
        let widgets = Rc::downgrade(widgets);
        row.connect_active_notify(move |row| {
            let Some(proxy) = sendable(&widgets) else {
                return;
            };
            let enabled = row.is_active();
            glib::spawn_future_local(async move {
                if let Err(err) = proxy.set_stage_enabled(name, enabled).await {
                    eprintln!("Could not switch {name}: {err}");
                }
            });
        });
    }
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
    let weak = Rc::downgrade(widgets);
    widgets.input.connect_selected_notify(move |row| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let Some(widgets) = weak.upgrade() else {
            return;
        };
        let Some(name) = widgets
            .input_names
            .borrow()
            .get(row.selected() as usize)
            .cloned()
        else {
            return;
        };
        glib::spawn_future_local(async move {
            if let Err(err) = proxy.set_input(&name).await {
                eprintln!("Could not switch to {name}: {err}");
            }
        });
    });
    let weak = Rc::downgrade(widgets);
    widgets.atten_lim.connect_value_changed(move |scale| {
        let Some(proxy) = sendable(&weak) else {
            return;
        };
        let db = scale.value();
        glib::spawn_future_local(async move {
            if let Err(err) = proxy.set_attenuation_limit(db).await {
                eprintln!("Could not set the attenuation limit: {err}");
            }
        });
    });
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
                show(&widgets, &status);
                widgets.stack.set_visible_child_name("controls");
            }
            Err(_) => widgets.stack.set_visible_child_name("missing"),
        }
        glib::timeout_future(POLL).await;
    }
}

/// The microphones to choose from, the chosen one selected; one that's unplugged stays in the
/// list, so the row shows what the filter waits for.
fn show_inputs(widgets: &Widgets, status: &Status) {
    let mut names = vec![String::new()];
    let mut labels = vec!["Default Input".to_owned()];
    for (name, description, _) in &status.inputs {
        names.push(name.clone());
        labels.push(description.clone());
    }
    if !names.contains(&status.input) {
        names.push(status.input.clone());
        labels.push(format!("{} (unplugged)", status.input));
    }
    if *widgets.input_names.borrow() != names {
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        widgets
            .input
            .set_model(Some(&gtk::StringList::new(&labels)));
        widgets.input_names.replace(names.clone());
    }
    let selected = names
        .iter()
        .position(|name| *name == status.input)
        .unwrap_or_default() as u32;
    if widgets.input.selected() != selected {
        widgets.input.set_selected(selected);
    }
}

fn show(widgets: &Widgets, status: &Status) {
    widgets.updating.set(true);
    for (name, enabled) in &status.stages {
        if let Some((_, row)) = widgets.switches.iter().find(|(stage, _)| stage == name) {
            row.set_active(*enabled);
        }
    }
    // not while the user drags it, the read value lags behind by up to a poll
    if (widgets.atten_lim.value() - status.attenuation_limit).abs() >= 1.
        && !widgets.atten_lim.has_focus()
    {
        widgets.atten_lim.set_value(status.attenuation_limit);
    }
    show_inputs(widgets, status);
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
