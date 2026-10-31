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
    fn attenuation_limit(&self) -> ::zbus::Result<f64>;
    #[zbus(property)]
    fn set_attenuation_limit(&self, db: f64) -> ::zbus::Result<()>;

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
    fn latency(&self) -> ::zbus::Result<HashMap<String, f64>>;
    #[zbus(property)]
    fn quantum(&self) -> ::zbus::Result<u64>;
    #[zbus(property)]
    fn max_frame_time(&self) -> ::zbus::Result<f64>;
    #[zbus(property)]
    fn counters(&self) -> ::zbus::Result<HashMap<String, u64>>;
}

/// Everything the window shows, read in one go.
pub struct Status {
    pub listen: bool,
    pub stages: Vec<(String, bool)>,
    pub attenuation_limit: f64,
    pub input: String,
    /// `node.name`, description and priority.
    pub inputs: Vec<(String, String, i32)>,
    pub latency: HashMap<String, f64>,
    pub quantum: u64,
    pub max_frame_time: f64,
    pub counters: HashMap<String, u64>,
}

impl VoiceFilterProxy<'_> {
    pub async fn status(&self) -> ::zbus::Result<Status> {
        Ok(Status {
            listen: self.listen().await?,
            stages: self.stages().await?,
            attenuation_limit: self.attenuation_limit().await?,
            input: self.input().await?,
            inputs: self.inputs().await?,
            latency: self.latency().await?,
            quantum: self.quantum().await?,
            max_frame_time: self.max_frame_time().await?,
            counters: self.counters().await?,
        })
    }
}
