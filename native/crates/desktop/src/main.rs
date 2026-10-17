mod cables;
mod effects;
mod routing;
mod workspace;
use axe_domain::Snapshot;
use axe_session::{Mock, Store, Transport};
use iced::widget::pane_grid;
mod knob;
mod style;
mod views;
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
enum Pane {
    Presets,
    Inspector,
    Routing,
    Chat,
    Audio,
}
impl Pane {
    fn title(self) -> &'static str {
        match self {
            Self::Presets => "Presets",
            Self::Inspector => "Block inspector",
            Self::Routing => "Signal chain",
            Self::Chat => "Tone assistant",
            Self::Audio => "Reference audio & audition",
        }
    }
}
#[derive(Debug, Clone)]
enum Message {
    Mock,
    Retry,
    Choose(axe_agents::Provider),
    Drag(pane_grid::DragEvent),
    Resize(pane_grid::ResizeEvent),
    Reset,
    Window(iced::Event),
    LayoutName(String),
    SaveLayout,
    LoadLayout,
    SimpleCables(bool),
    Endpoints(Result<axe_session::usb::Endpoints, String>),
    Input(String),
    Output(String),
    Capture,
    Captured(u64, Result<Snapshot, String>),
}
struct Studio {
    panes: pane_grid::State<Pane>,
    snapshot: Option<Snapshot>,
    status: String,
    startup: axe_agents::Startup,
    store: Option<Store>,
    size: iced::Size,
    layout_name: String,
    simple_cables: bool,
    endpoints: Option<axe_session::usb::Endpoints>,
    input: Option<String>,
    output: Option<String>,
    busy: bool,
    epoch: u64,
}
fn layout() -> pane_grid::State<Pane> {
    use pane_grid::{Axis, Configuration as C};
    pane_grid::State::with_configuration(C::Split {
        axis: Axis::Horizontal,
        ratio: 0.78,
        a: Box::new(C::Split {
            axis: Axis::Vertical,
            ratio: 0.23,
            a: Box::new(C::Split {
                axis: Axis::Horizontal,
                ratio: 0.4,
                a: Box::new(C::Pane(Pane::Presets)),
                b: Box::new(C::Pane(Pane::Inspector)),
            }),
            b: Box::new(C::Split {
                axis: Axis::Vertical,
                ratio: 0.64,
                a: Box::new(C::Pane(Pane::Routing)),
                b: Box::new(C::Pane(Pane::Chat)),
            }),
        }),
        b: Box::new(C::Pane(Pane::Audio)),
    })
}
impl Default for Studio {
    fn default() -> Self {
        let store = directories::ProjectDirs::from("org", "AxeStudio", "AxeStudio").and_then(|d| {
            std::fs::create_dir_all(d.data_local_dir()).ok()?;
            Store::open(d.data_local_dir().join("studio.sqlite3")).ok()
        });
        let status = if store.is_some() {
            "USB transport pending verification. Select Test mock explicitly to inspect the native workspace."
        } else {
            "Local storage unavailable. Agent and edits disabled."
        };
        Self {
            panes: store
                .as_ref()
                .and_then(|s| s.setting("layout.current").ok().flatten())
                .and_then(|j| serde_json::from_str::<workspace::Layout>(&j).ok())
                .filter(|l| l.valid(1256., 680.))
                .map(|l| l.state())
                .unwrap_or_else(layout),
            snapshot: None,
            status: status.into(),
            startup: Default::default(),
            store,
            size: iced::Size::new(1440., 960.),
            layout_name: "My layout".into(),
            simple_cables: false,
            endpoints: None,
            input: None,
            output: None,
            busy: false,
            epoch: 0,
        }
    }
}
impl Studio {
    fn new() -> (Self, iced::Task<Message>) {
        let s = Self {
            busy: true,
            status: "Discovering USB MIDI endpoints…".into(),
            ..Self::default()
        };
        (s, Self::discover())
    }
    fn discover() -> iced::Task<Message> {
        iced::Task::perform(
            async {
                tokio::task::spawn_blocking(axe_session::usb::endpoints)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            },
            Message::Endpoints,
        )
    }
    fn update(&mut self, m: Message) -> iced::Task<Message> {
        let before = workspace::Layout::capture(&self.panes);
        let layout_change = matches!(
            &m,
            Message::Drag(pane_grid::DragEvent::Dropped { .. })
                | Message::Resize(_)
                | Message::Reset
                | Message::LoadLayout
        );
        match m {
            Message::Mock => {
                if self.busy {
                    return iced::Task::none();
                }
                self.epoch += 1;
                if let Ok(s) = Mock::default().snapshot() {
                    self.startup.captured(s.context.clone());
                    self.snapshot = Some(s);
                    self.status = "TEST MOCK — no USB endpoint opened. Choose a provider.".into();
                }
            }
            Message::Retry => {
                if self.busy {
                    return iced::Task::none();
                }
                self.epoch += 1;
                self.snapshot = None;
                self.startup = Default::default();
                self.busy = true;
                self.status = "Discovering MIDI endpoints…".into();
                return Self::discover();
            }
            Message::Endpoints(result) => {
                self.busy = false;
                match result {
                    Ok(ports) => {
                        let inputs = ports
                            .inputs
                            .iter()
                            .filter(|n| n.to_lowercase().contains("axe-fx"))
                            .cloned()
                            .collect::<Vec<_>>();
                        let outputs = ports
                            .outputs
                            .iter()
                            .filter(|n| n.to_lowercase().contains("axe-fx"))
                            .cloned()
                            .collect::<Vec<_>>();
                        self.endpoints = Some(ports);
                        if inputs.len() == 1 && outputs.len() == 1 {
                            self.input = Some(inputs[0].clone());
                            self.output = Some(outputs[0].clone());
                            return self.update(Message::Capture);
                        }
                        self.status="Select MIDI input/output and Read USB, or explicitly choose Test mock.".into();
                    }
                    Err(e) => self.status = format!("USB discovery failed: {e}"),
                }
            }
            Message::Input(name) => self.input = Some(name),
            Message::Output(name) => self.output = Some(name),
            Message::Capture => {
                if self.busy {
                    return iced::Task::none();
                }
                if let (Some(input), Some(output)) = (self.input.clone(), self.output.clone()) {
                    self.busy = true;
                    self.epoch += 1;
                    let epoch = self.epoch;
                    self.snapshot = None;
                    self.startup = Default::default();
                    self.status = "Reading device routing (read-only)…".into();
                    return iced::Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                axe_session::usb::capture(&input, &output)
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                        },
                        move |result| Message::Captured(epoch, result),
                    );
                }
                self.status = "Select both MIDI endpoints first.".into();
            }
            Message::Captured(epoch, result) => {
                if epoch != self.epoch {
                    return iced::Task::none();
                }
                self.busy = false;
                match result {
                    Ok(snapshot) => {
                        self.startup.captured(snapshot.context.clone());
                        self.snapshot = Some(snapshot);
                        self.status="USB routing read. Effect IDs preserved; unknown parameters stay unavailable. Choose a provider.".into();
                    }
                    Err(e) => {
                        self.status = format!("USB read failed: {e}. Retry or select endpoints.")
                    }
                }
            }
            Message::Choose(p) => {
                if self.startup.choose(p).is_ok() {
                    self.status = format!(
                        "{p:?} selected. Adapter runtime and tool-isolation verification pending; conversation disabled."
                    );
                }
            }
            Message::Drag(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
            }
            Message::Resize(e) => self.panes.resize(e.split, e.ratio.clamp(0.2, 0.8)),
            Message::SimpleCables(value) => self.simple_cables = value,
            Message::Reset => self.panes = layout(),
            Message::Window(iced::Event::Window(iced::window::Event::Resized(size))) => {
                self.size = size
            }
            Message::LayoutName(name) => self.layout_name = name.chars().take(64).collect(),
            Message::SaveLayout => {
                if let Some(store) = &self.store {
                    match serde_json::to_string(&before)
                        .map_err(|e| e.to_string())
                        .and_then(|j| {
                            store
                                .set_setting(&format!("layout.named.{}", self.layout_name), &j)
                                .map_err(|e| e.to_string())
                        }) {
                        Ok(()) => self.status = "Layout saved".into(),
                        Err(e) => self.status = e,
                    }
                }
            }
            Message::LoadLayout => {
                if let Some(saved) = self
                    .store
                    .as_ref()
                    .and_then(|s| {
                        s.setting(&format!("layout.named.{}", self.layout_name))
                            .ok()
                            .flatten()
                    })
                    .and_then(|j| serde_json::from_str::<workspace::Layout>(&j).ok())
                {
                    self.panes = saved.state();
                } else {
                    self.status = "Named layout unavailable".into();
                }
            }
            _ => {}
        }
        if layout_change {
            let next = workspace::Layout::capture(&self.panes);
            if !next.valid(self.size.width - 24., self.size.height - 170.) {
                self.panes = before.state();
                self.status = "Arrangement needs more space; previous layout retained.".into();
            } else if let Some(store) = &self.store
                && let Ok(j) = serde_json::to_string(&next)
                && let Err(e) = store.set_setting("layout.current", &j)
            {
                self.status = e.to_string();
            }
        }
        iced::Task::none()
    }
}

fn main() -> iced::Result {
    iced::application(Studio::new, Studio::update, Studio::view)
        .title("Axe Studio · development")
        .theme(style::theme())
        .subscription(|_| iced::event::listen().map(Message::Window))
        .window(iced::window::Settings {
            size: iced::Size::new(1440., 960.),
            min_size: Some(iced::Size::new(1280., 850.)),
            ..Default::default()
        })
        .run()
}
