mod cables;
mod effects;
mod routing;
mod workspace;
use axe_domain::Snapshot;
use axe_session::{Mock, Store, Transport};
use integration::{Lane, Pending};
mod conversation;
mod integration;
mod waveform;
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
#[derive(Clone)]
enum Message {
    Mock,
    Composer(String),
    ClaudeKey(String),
    Send,
    UserPersisted(u64, Result<(String, String), String>),
    SubmissionRecorded(Result<(), String>),
    Stop,
    Agent(u64, axe_agents::runtime::AgentEvent),
    Runtime(u64, u64, bool, Result<integration::RuntimeView, String>),
    SelectBlock(String),
    MoveBlock(String, (f32, f32)),
    UnsupportedRouting,
    Adjust(String, String, f64),
    PreviewAdjust(String, String, f64),
    CommitAdjust(String, String),
    Restore,
    SaveDialog,
    Destination(String),
    ConfirmSave,
    DeviceResult(Result<(), String>),
    CancelDialog,
    LeaveUnsaved,
    RequestClose,
    CloseChecked(Result<(bool, bool), String>),
    Exit,
    Import(Lane),
    ImportCancelled,
    Imported(Lane, Result<axe_audio::artifact::Artifact, String>),
    ExcerptStart(String),
    ExcerptDuration(String),
    Play(Lane),
    PlayReady(Result<(std::path::PathBuf, f64, f64, f32), String>),
    PauseAudio,
    ResumeAudio,
    Volume(f32),
    AudioEvent(axe_audio::playback::PlaybackEvent),
    StopAudio,
    LevelMatch(bool),
    Analyze(Lane),
    Analyzed(Result<(String, serde_json::Value), String>),
    Attach(String, bool),
    ViewAnalysis(String),
    CompareAudio,
    Retry,
    DiscoveryTick,
    AutoLayout,
    Choose(axe_agents::Provider),
    Drag(pane_grid::DragEvent),
    Resize(pane_grid::ResizeEvent),
    Reset,
    FocusPane(pane_grid::Pane),
    SwapPane(pane_grid::Direction),
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
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Studio intent {:?}", std::mem::discriminant(self))
    }
}

struct Studio {
    panes: pane_grid::State<Pane>,
    focused_pane: Option<pane_grid::Pane>,
    snapshot: Option<Snapshot>,
    status: String,
    store: Option<Store>,
    size: iced::Size,
    layout_name: String,
    simple_cables: bool,
    endpoints: Option<axe_session::usb::Endpoints>,
    input: Option<String>,
    output: Option<String>,
    busy: bool,
    epoch: u64,
    backend: Option<axe_runtime::Shared>,
    runtime_request: std::cell::Cell<u64>,
    runtime_applied: u64,
    resolve_pending: bool,
    data_root: std::path::PathBuf,
    agent: Option<axe_agents::runtime::AgentHandle>,
    agent_epoch: u64,
    provider: Option<axe_agents::Provider>,
    agent_ready: bool,
    agent_busy: bool,
    composer: String,
    submission_epoch: u64,
    claude_key: String,
    agent_error: Option<String>,
    history_namespace: String,
    analysis_dialog: Option<serde_json::Value>,
    history: Vec<axe_session::ChatMessage>,
    dirty: bool,
    recovery: bool,
    device_busy: bool,
    selected_block: Option<String>,
    block_positions: std::collections::BTreeMap<String, (f32, f32)>,
    routing_viewport: std::sync::Arc<std::sync::Mutex<iced::Size>>,
    pending_parameters: std::collections::BTreeMap<(String, String), f64>,
    pending: Option<Pending>,
    save_dialog: bool,
    destination: String,
    reference: Option<axe_audio::artifact::Artifact>,
    recording: Option<axe_audio::artifact::Artifact>,
    player: Option<(
        axe_audio::playback::Player,
        std::sync::Arc<
            std::sync::Mutex<std::sync::mpsc::Receiver<axe_audio::playback::PlaybackEvent>>,
        >,
    )>,
    excerpt_start: String,
    excerpt_duration: String,
    level_match: bool,
    audio_busy: bool,
    audio_position: f64,
    audio_playing: bool,
    volume: f32,
    analyses: std::collections::BTreeMap<String, serde_json::Value>,
    attached: std::collections::BTreeSet<String>,
}
fn layout() -> pane_grid::State<Pane> {
    use pane_grid::{Axis, Configuration as C};
    pane_grid::State::with_configuration(C::Split {
        axis: Axis::Horizontal,
        ratio: 0.74,
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
        let data_root = directories::ProjectDirs::from("org", "AxeStudio", "AxeStudio")
            .map(|d| d.data_local_dir().to_owned())
            .unwrap_or_else(|| std::env::temp_dir().join("axe-studio"));
        let backend = axe_runtime::Backend::open(data_root.clone()).ok();
        let history_namespace = backend
            .as_ref()
            .and_then(|b| {
                b.lock()
                    .ok()?
                    .store
                    .setting("last_namespace")
                    .ok()
                    .flatten()
            })
            .unwrap_or("hardware".into());
        let history = backend
            .as_ref()
            .and_then(|b| b.lock().ok()?.history(&history_namespace).ok())
            .unwrap_or_default();
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
            focused_pane: None,
            snapshot: None,
            status: status.into(),
            store,
            size: iced::Size::new(1440., 960.),
            layout_name: "My layout".into(),
            simple_cables: false,
            endpoints: None,
            input: None,
            output: None,
            busy: false,
            epoch: 0,
            backend,
            runtime_request: std::cell::Cell::new(0),
            runtime_applied: 0,
            resolve_pending: false,
            data_root,
            agent: None,
            agent_epoch: 0,
            provider: None,
            agent_ready: false,
            agent_busy: false,
            composer: String::new(),
            submission_epoch: 0,
            claude_key: String::new(),
            agent_error: None,
            history_namespace,
            analysis_dialog: None,
            history,
            dirty: false,
            recovery: false,
            device_busy: false,
            selected_block: None,
            block_positions: Default::default(),
            routing_viewport: std::sync::Arc::new(std::sync::Mutex::new(iced::Size::new(
                680., 420.,
            ))),
            pending_parameters: Default::default(),
            pending: None,
            save_dialog: false,
            destination: "0".into(),
            reference: None,
            recording: None,
            player: None,
            excerpt_start: "0".into(),
            excerpt_duration: "30".into(),
            level_match: true,
            audio_busy: false,
            audio_position: 0.,
            audio_playing: false,
            volume: 0.8,
            analyses: Default::default(),
            attached: Default::default(),
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
        if let Some(task) = self.handle_integration(m.clone()) {
            return task;
        }
        let before = workspace::Layout::capture(&self.panes);
        let layout_change = matches!(
            &m,
            Message::Drag(pane_grid::DragEvent::Dropped { .. })
                | Message::Resize(_)
                | Message::Reset
                | Message::LoadLayout
                | Message::SwapPane(_)
        );
        match m {
            Message::Mock => {
                if self.backend.is_none() {
                    self.status =
                        "Application data is unavailable or in use by another Axe Studio instance."
                            .into();
                    return iced::Task::none();
                }
                if self.device_busy || self.agent_busy {
                    return iced::Task::none();
                }
                if self.dirty || self.recovery {
                    self.pending = Some(Pending::Mock);
                    return iced::Task::none();
                }
                if self.busy {
                    return iced::Task::none();
                }
                self.epoch += 1;
                if let Ok(s) = Mock::default().snapshot() {
                    self.snapshot = Some(s);
                    self.status = "TEST MOCK — no USB endpoint opened. Choose a provider.".into();
                    return self.bind_runtime();
                }
            }
            Message::DiscoveryTick => {
                if self.snapshot.is_none() && !self.busy && self.backend.is_some() {
                    self.busy = true;
                    return Self::discover();
                }
            }
            Message::Retry => {
                if self.dirty || self.recovery {
                    self.pending = Some(Pending::Capture);
                    return iced::Task::none();
                }
                self.disconnect_agent();
                if self.busy {
                    return iced::Task::none();
                }
                self.epoch += 1;
                self.snapshot = None;
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
                            .filter(|n| {
                                let name: String = n
                                    .to_lowercase()
                                    .chars()
                                    .filter(char::is_ascii_alphanumeric)
                                    .collect();
                                name.contains("axefx") || name.contains("fractalaudio")
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        let outputs = ports
                            .outputs
                            .iter()
                            .filter(|n| {
                                let name: String = n
                                    .to_lowercase()
                                    .chars()
                                    .filter(char::is_ascii_alphanumeric)
                                    .collect();
                                name.contains("axefx") || name.contains("fractalaudio")
                            })
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
                if self.backend.is_none() {
                    self.status =
                        "Application data is unavailable or in use by another Axe Studio instance."
                            .into();
                    return iced::Task::none();
                }
                if self.device_busy || self.agent_busy {
                    return iced::Task::none();
                }
                if self.dirty || self.recovery {
                    self.pending = Some(Pending::Capture);
                    return iced::Task::none();
                }
                self.disconnect_agent();
                if self.busy {
                    return iced::Task::none();
                }
                if let (Some(input), Some(output)) = (self.input.clone(), self.output.clone()) {
                    self.busy = true;
                    self.epoch += 1;
                    let epoch = self.epoch;
                    self.snapshot = None;
                    self.status =
                        "Reading routing and catalogued block parameters (read-only)…".into();
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
                        let parameter_count: usize = snapshot
                            .blocks
                            .iter()
                            .map(|block| block.parameters.len())
                            .sum();
                        self.snapshot = Some(snapshot);
                        self.status = format!(
                            "USB routing and {parameter_count} catalogued parameter rows captured read-only. Choose a provider."
                        );
                        return self.bind_runtime();
                    }
                    Err(e) => {
                        self.status = format!("USB read failed: {e}. Retry or select endpoints.")
                    }
                }
            }
            Message::Choose(p) => return self.choose_provider(p),
            Message::Drag(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
            }
            Message::Resize(e) => self.panes.resize(e.split, e.ratio.clamp(0.2, 0.8)),
            Message::SimpleCables(value) => self.simple_cables = value,
            Message::AutoLayout => {
                if let Some(snapshot) = &self.snapshot {
                    let viewport =
                        self.routing_viewport
                            .lock()
                            .map(|size| *size)
                            .unwrap_or(iced::Size::new(
                                (self.size.width * 0.5 - 50.).max(300.),
                                (self.size.height * 0.44 - 10.).max(300.),
                            ));
                    self.block_positions = routing::auto_layout(snapshot, viewport);
                }
            }
            Message::FocusPane(p) => self.focused_pane = Some(p),
            Message::SwapPane(direction) => {
                if let Some(p) = self
                    .focused_pane
                    .or_else(|| self.panes.iter().next().map(|(p, _)| *p))
                    && let Some(other) = self.panes.adjacent(p, direction)
                {
                    self.panes.swap(p, other);
                }
            }
            Message::Window(iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                ..
            })) => {
                if modifiers.command() && modifiers.alt() {
                    use iced::keyboard::key::Named;
                    use pane_grid::Direction;
                    let d = match key {
                        iced::keyboard::Key::Named(Named::ArrowLeft) => Some(Direction::Left),
                        iced::keyboard::Key::Named(Named::ArrowRight) => Some(Direction::Right),
                        iced::keyboard::Key::Named(Named::ArrowUp) => Some(Direction::Up),
                        iced::keyboard::Key::Named(Named::ArrowDown) => Some(Direction::Down),
                        _ => None,
                    };
                    if let Some(d) = d {
                        return self.update(Message::SwapPane(d));
                    }
                }
            }
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
        .subscription(|s| {
            let mut subscriptions = vec![
                iced::event::listen().map(|e| {
                    if matches!(e, iced::Event::Window(iced::window::Event::CloseRequested)) {
                        Message::RequestClose
                    } else {
                        Message::Window(e)
                    }
                }),
                s.agent_subscription(),
                s.audio_subscription(),
            ];
            if s.snapshot.is_none() && !s.busy && s.backend.is_some() {
                subscriptions.push(
                    iced::time::every(std::time::Duration::from_secs(2))
                        .map(|_| Message::DiscoveryTick),
                );
            }
            iced::Subscription::batch(subscriptions)
        })
        .window(iced::window::Settings {
            size: iced::Size::new(1440., 960.),
            exit_on_close_request: false,
            min_size: Some(iced::Size::new(1280., 850.)),
            ..Default::default()
        })
        .run()
}
