//! The client side of the control interface of `voice-filter run` ([`crate::dbus`], the names and
//! types here must match it).

use ::std::collections::HashMap;

use ::zbus::proxy;

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
    pub counters: HashMap<String, u64>,
    /// `input`, `output` and `input-floor` in dBFS, see `src/dbus.rs`.
    pub levels: HashMap<String, f64>,
}

impl VoiceFilterProxy<'_> {
    pub async fn status(&self) -> ::zbus::Result<Status> {
        Ok(Status {
            listen: self.listen().await?,
            stages: self.stages().await?,
            parameters: self
                .parameters()
                .await?
                .into_iter()
                .map(Parameter::from)
                .collect(),
            input: self.input().await?,
            inputs: self.inputs().await?,
            output: self.output().await?,
            outputs: self.outputs().await?,
            latency: self.latency().await?,
            echo: self.echo().await?,
            quantum: self.quantum().await?,
            max_frame_time: self.max_frame_time().await?,
            counters: self.counters().await?,
            levels: self.levels().await?,
        })
    }
}
