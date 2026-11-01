//! The client side of the control interface of `voice-filter run` ([`crate::dbus`], the names and
//! types here must match it).

use ::std::collections::HashMap;

use ::zbus::{DeserializeDict, Type, proxy};

#[proxy(
    interface = "net.b128.VoiceFilter1",
    default_service = "net.b128.VoiceFilter.Daemon",
    default_path = "/net/b128/VoiceFilter"
)]
pub trait VoiceFilter {
    fn set_stage_enabled(&self, name: &str, enabled: bool) -> ::zbus::Result<()>;

    #[zbus(property)]
    fn parameters(&self) -> ::zbus::Result<Vec<ParameterTuple>>;
    fn set_parameter(&self, stage: &str, name: &str, value: f64) -> ::zbus::Result<()>;

    #[zbus(property)]
    fn listen(&self) -> ::zbus::Result<bool>;
    #[zbus(property)]
    fn set_listen(&self, on: bool) -> ::zbus::Result<()>;
    #[zbus(property)]
    fn keep_listen(&self) -> ::zbus::Result<bool>;
    #[zbus(property)]
    fn set_keep_listen(&self, keep: bool) -> ::zbus::Result<()>;
    #[zbus(property)]
    fn listen_volume(&self) -> ::zbus::Result<f64>;
    #[zbus(property)]
    fn set_listen_volume(&self, db: f64) -> ::zbus::Result<()>;

    #[zbus(property)]
    fn stages(&self) -> ::zbus::Result<Vec<(String, bool)>>;
    #[zbus(property)]
    fn input(&self) -> ::zbus::Result<String>;
    #[zbus(property)]
    fn set_input(&self, name: &str) -> ::zbus::Result<()>;
    #[zbus(property)]
    fn inputs(&self) -> ::zbus::Result<Vec<(String, String, i32)>>;
    #[zbus(property)]
    fn output(&self) -> ::zbus::Result<String>;
    #[zbus(property)]
    fn set_output(&self, name: &str) -> ::zbus::Result<()>;
    #[zbus(property)]
    fn outputs(&self) -> ::zbus::Result<Vec<(String, String, i32)>>;
    #[zbus(property)]
    fn latency(&self) -> ::zbus::Result<HashMap<String, f64>>;
    #[zbus(property)]
    fn echo(&self) -> ::zbus::Result<HashMap<String, f64>>;
    #[zbus(property)]
    fn quantum(&self) -> ::zbus::Result<u64>;
    #[zbus(property)]
    fn max_frame_time(&self) -> ::zbus::Result<f64>;
    #[zbus(property)]
    fn frame_times(&self) -> ::zbus::Result<Vec<(String, f64, f64, f64, f64)>>;
    #[zbus(property)]
    fn counters(&self) -> ::zbus::Result<HashMap<String, u64>>;
    #[zbus(property)]
    fn levels(&self) -> ::zbus::Result<HashMap<String, f64>>;
}

/// A parameter as the bus carries it, see [`Parameter`].
type ParameterTuple = (String, String, String, String, f64, f64, f64, f64, f64);

/// A setting of a stage.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub stage: String,
    pub name: String,
    pub title: String,
    /// Shown after the value, e.g. `dB`, `Hz` or `:1`.
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    pub value: f64,
}

impl From<ParameterTuple> for Parameter {
    fn from((stage, name, title, unit, min, max, step, default, value): ParameterTuple) -> Self {
        Self {
            stage,
            name,
            title,
            unit,
            min,
            max,
            step,
            default,
            value,
        }
    }
}

/// Everything the window shows, read in one go.
pub struct Status {
    pub listen: bool,
    pub keep_listen: bool,
    /// In dB.
    pub listen_volume: f64,
    pub stages: Vec<(String, bool)>,
    /// In the order of the chain.
    pub parameters: Vec<Parameter>,
    pub input: String,
    /// `node.name`, description and priority.
    pub inputs: Vec<(String, String, i32)>,
    /// The output device whose echo gets removed, like `input`.
    pub output: String,
    pub outputs: Vec<(String, String, i32)>,
    pub latency: HashMap<String, f64>,
    /// How the echo cancellation does, see `src/dbus.rs`.
    pub echo: HashMap<String, f64>,
    pub quantum: u64,
    pub max_frame_time: f64,
    /// Part, mean, 95 % and 99 % percentile and longest, in ms, see `src/dbus.rs`.
    pub frame_times: Vec<(String, f64, f64, f64, f64)>,
    pub counters: HashMap<String, u64>,
    /// `input`, `output` and `input-floor` in dBFS, see `src/dbus.rs`.
    pub levels: HashMap<String, f64>,
}

/// All properties of the interface as `GetAll` returns them, by their names on the bus.
#[derive(DeserializeDict, Type)]
#[zbus(signature = "a{sv}", rename_all = "PascalCase")]
struct Properties {
    listen: bool,
    keep_listen: bool,
    listen_volume: f64,
    stages: Vec<(String, bool)>,
    parameters: Vec<ParameterTuple>,
    input: String,
    inputs: Vec<(String, String, i32)>,
    output: String,
    outputs: Vec<(String, String, i32)>,
    latency: HashMap<String, f64>,
    echo: HashMap<String, f64>,
    quantum: u64,
    max_frame_time: f64,
    frame_times: Vec<(String, f64, f64, f64, f64)>,
    counters: HashMap<String, u64>,
    levels: HashMap<String, f64>,
}

impl VoiceFilterProxy<'_> {
    /// All of it in one call (`GetAll`), not one per property: the app polls it every second.
    pub async fn status(&self) -> ::zbus::Result<Status> {
        let proxy = self.inner();
        let reply = proxy
            .connection()
            .call_method(
                Some(proxy.destination()),
                proxy.path(),
                Some("org.freedesktop.DBus.Properties"),
                "GetAll",
                &(proxy.interface(),),
            )
            .await?;
        let properties: Properties = reply.body().deserialize()?;
        Ok(Status {
            listen: properties.listen,
            keep_listen: properties.keep_listen,
            listen_volume: properties.listen_volume,
            stages: properties.stages,
            parameters: properties
                .parameters
                .into_iter()
                .map(Parameter::from)
                .collect(),
            input: properties.input,
            inputs: properties.inputs,
            output: properties.output,
            outputs: properties.outputs,
            latency: properties.latency,
            echo: properties.echo,
            quantum: properties.quantum,
            max_frame_time: properties.max_frame_time,
            frame_times: properties.frame_times,
            counters: properties.counters,
            levels: properties.levels,
        })
    }
}
