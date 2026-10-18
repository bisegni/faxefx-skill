//! Native intent handlers. Disk/process/audio work runs outside the UI thread.
use crate::{Message, Studio};
use axe_agents::{
    Provider,
    runtime::{AgentEvent, AgentHandle},
};
use axe_domain::{Change, Snapshot};
use axe_session::ChatMessage;
use iced::{Subscription, Task};
use std::sync::{Arc, Mutex, mpsc};
#[derive(Debug, Clone)]
pub struct RuntimeView {
    pub snapshot: Option<Snapshot>,
    pub history: Vec<ChatMessage>,
    pub dirty: bool,
    pub recovery: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Reference,
    Recording,
}
#[derive(Debug, Clone)]
pub enum Pending {
    Close,
    Mock,
    Capture,
}
#[derive(Clone)]
pub struct EventSource {
    pub epoch: u64,
    pub receiver: Arc<Mutex<mpsc::Receiver<AgentEvent>>>,
}
impl std::hash::Hash for EventSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.epoch.hash(state)
    }
}
pub fn agent_events(source: &EventSource) -> impl iced::futures::Stream<Item = Message> + use<> {
    let source = source.clone();
    iced::futures::stream::unfold(source, |source| async move {
        let receiver = source.receiver.clone();
        match tokio::task::spawn_blocking(move || receiver.lock().ok()?.recv().ok())
            .await
            .ok()
            .flatten()
        {
            Some(event) => Some((Message::Agent(source.epoch, event), source)),
            None => None,
        }
    })
}
#[derive(Clone)]
pub struct AudioSource {
    receiver: Arc<Mutex<mpsc::Receiver<axe_audio::playback::PlaybackEvent>>>,
}
impl std::hash::Hash for AudioSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.receiver) as usize).hash(state)
    }
}
fn audio_events(source: &AudioSource) -> impl iced::futures::Stream<Item = Message> + use<> {
    let receiver = source.receiver.clone();
    iced::futures::stream::unfold(receiver, |receiver| async move {
        let r = receiver.clone();
        tokio::task::spawn_blocking(move || r.lock().ok()?.recv().ok())
            .await
            .ok()
            .flatten()
            .map(|e| (Message::AudioEvent(e), receiver))
    })
}
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}
impl Studio {
    pub(super) fn audio_subscription(&self) -> Subscription<Message> {
        self.player
            .as_ref()
            .map(|(_, r)| {
                Subscription::run_with(
                    AudioSource {
                        receiver: r.clone(),
                    },
                    audio_events,
                )
            })
            .unwrap_or_else(Subscription::none)
    }
    pub(super) fn namespace(&self) -> String {
        self.snapshot
            .as_ref()
            .map(|s| s.context.target.namespace())
            .unwrap_or(&self.history_namespace)
            .into()
    }
    pub(super) fn agent_subscription(&self) -> Subscription<Message> {
        self.agent
            .as_ref()
            .map(|agent| {
                Subscription::run_with(
                    EventSource {
                        epoch: self.agent_epoch,
                        receiver: agent.events.clone(),
                    },
                    agent_events,
                )
            })
            .unwrap_or_else(Subscription::none)
    }
    pub(super) fn refresh_runtime(&self) -> Task<Message> {
        let Some(backend) = self.backend.clone() else {
            return Task::none();
        };
        let namespace = self.namespace();
        let epoch = self.epoch;
        let request = self.runtime_request.get() + 1;
        self.runtime_request.set(request);
        Task::perform(
            blocking(move || {
                let b = backend.lock().map_err(|_| "Backend unavailable")?;
                Ok(RuntimeView {
                    snapshot: b.snapshot.clone(),
                    history: b.history(&namespace)?,
                    dirty: b.dirty(),
                    recovery: b.recovery(),
                })
            }),
            move |result| Message::Runtime(epoch, request, false, result),
        )
    }
    pub(super) fn bind_runtime(&mut self) -> Task<Message> {
        self.disconnect_agent();
        let Some(backend) = self.backend.clone() else {
            return Task::none();
        };
        let Some(snapshot) = self.snapshot.clone() else {
            return Task::none();
        };
        let namespace = snapshot.context.target.namespace().to_owned();
        let endpoints = self.input.clone().zip(self.output.clone());
        self.device_busy = true;
        self.attached.clear();
        self.history_namespace = namespace.clone();
        self.block_positions.clear();
        self.pending_parameters.clear();
        let epoch = self.epoch;
        let request = self.runtime_request.get() + 1;
        self.runtime_request.set(request);
        Task::perform(
            blocking(move || {
                let mut b = backend.lock().map_err(|_| "Backend unavailable")?;
                if snapshot.context.target == axe_domain::Target::Hardware {
                    let (input, output) = endpoints.ok_or("Select the captured USB endpoints")?;
                    b.bind_hardware(snapshot, input, output)?;
                } else {
                    b.bind(snapshot)?;
                }
                Ok(RuntimeView {
                    snapshot: b.snapshot.clone(),
                    history: b.history(&namespace)?,
                    dirty: b.dirty(),
                    recovery: b.recovery(),
                })
            }),
            move |result| Message::Runtime(epoch, request, true, result),
        )
    }
    pub(super) fn disconnect_agent(&mut self) {
        if let Some(agent) = self.agent.take() {
            agent.stop();
            std::thread::spawn(move || drop(agent));
        }
        self.agent_epoch += 1;
        self.agent_ready = false;
        self.agent_busy = false;
    }
    pub(super) fn choose_provider(&mut self, provider: Provider) -> Task<Message> {
        if self.snapshot.is_none() || self.device_busy {
            return Task::none();
        }
        if self.agent_busy {
            self.status = "Stop the current turn before switching providers.".into();
            return Task::none();
        }
        self.disconnect_agent();
        self.provider = Some(provider);
        self.agent_error = None;
        self.agent_busy = true;
        self.status = format!("Connecting {provider:?} and initializing axefx-live…");
        let Some(backend) = self.backend.clone() else {
            self.agent_busy = false;
            return Task::none();
        };
        let namespace = self.namespace();
        let epoch = self.agent_epoch;
        // Creating the handle is cheap; all connection and skill work runs in its actor.
        let root = self
            .data_root
            .join("providers")
            .join(format!("{provider:?}"));
        let exe = match axe_agents::runtime::executable(match provider {
            Provider::Codex => "codex",
            Provider::Claude => "claude",
        }) {
            Ok(e) => e,
            Err(e) => {
                self.agent_busy = false;
                self.status = e;
                return Task::none();
            }
        };
        let resume = backend.lock().ok().and_then(|b| {
            b.store
                .setting(&format!("provider.{namespace}.{provider:?}"))
                .ok()
                .flatten()
        });
        let (tools, persist) = axe_runtime::handlers(backend, namespace, provider);
        self.agent = Some(AgentHandle::connect_with_key(
            provider,
            exe,
            root,
            resume,
            tools,
            persist,
            (!self.claude_key.trim().is_empty()).then(|| self.claude_key.clone()),
        ));
        self.agent_epoch = epoch;
        Task::none()
    }
    pub(super) fn send_message(&mut self) -> Task<Message> {
        let Some(provider) = self.provider else {
            return Task::none();
        };
        let Some(_agent) = self.agent.as_ref() else {
            return Task::none();
        };
        if !self.agent_ready || self.agent_busy {
            return Task::none();
        }
        let text = self.composer.trim().to_owned();
        if text.is_empty() || text.chars().count() > 12000 {
            self.status = "Write a message of up to 12000 characters.".into();
            return Task::none();
        }
        let Some(backend) = self.backend.clone() else {
            return Task::none();
        };
        let namespace = self.namespace();
        self.agent_busy = true;
        self.submission_epoch += 1;
        let epoch = self.submission_epoch;
        Task::perform(
            blocking(move || {
                let id = backend
                    .lock()
                    .map_err(|_| "Backend unavailable")?
                    .user_message(&namespace, provider, &text)?;
                Ok((id, text))
            }),
            move |result| Message::UserPersisted(epoch, result),
        )
    }
    pub(super) fn handle_integration(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::Exit => return Some(iced::exit()),
            Message::ClaudeKey(key) => self.claude_key = key.chars().take(1024).collect(),
            Message::Composer(text) => self.composer = text.chars().take(12000).collect(),
            Message::Send => return Some(self.send_message()),
            Message::UserPersisted(epoch,result)=>{
                match result{
                    Ok((id,text))=>{
                        let status=if epoch!=self.submission_epoch{"not_sent"}else{
                            self.composer.clear();let prompt=format!("{text}\n\nApp selection: {}",self.selected_block.as_deref().unwrap_or("none"));
                            match self.agent.as_ref().ok_or("Assistant disconnected".into()).and_then(|agent|agent.submit(&prompt)){Ok(())=>{self.status="Listening to your direction…".into();"sent"},Err(e)=>{self.agent_busy=false;self.status=e;"not_sent"}}
                        };
                        let Some(backend)=self.backend.clone()else{return Some(Task::none())};return Some(Task::perform(blocking(move||backend.lock().map_err(|_|"Backend unavailable")?.store.message_status(&id,status).map_err(|e|e.to_string())),Message::SubmissionRecorded));
                    },Err(e)=>{self.agent_busy=false;self.status=e;}
                }
            }
            Message::SubmissionRecorded(result)=>{if let Err(e)=result{self.status=e;}return Some(self.refresh_runtime())},
            Message::Stop => {
                self.submission_epoch+=1;
                if let Some(agent) = &self.agent {
                    let active=agent.busy.load(std::sync::atomic::Ordering::SeqCst);
                    agent.stop();
                    if !active{self.agent_busy=false;}
                    self.status = "Stopping; revoking tool authority and draining the turn…".into();
                }
            }
            Message::Agent(epoch, event) => {
                if epoch != self.agent_epoch {
                    return Some(Task::none());
                }
                match event {
                    AgentEvent::Ready => {
                        self.agent_ready = true;
                        self.agent_busy = false;
                        self.status = "Assistant ready. Describe the sound you want.".into();
                    }
                    AgentEvent::Completed => {
                        self.agent_busy = false;
                        self.status = "Ready for you to play and listen.".into();
                    }
                    AgentEvent::Interrupted => {
                        self.agent_busy = false;
                        self.status = "Stopped. Partial response retained.".into();
                    }
                    AgentEvent::Failed(error) => {
                        self.agent_ready = false;
                        self.agent_busy = false;
                        self.agent_error = Some(error.clone());
                        self.status = error;
                    }
                    _ => {}
                }
                return Some(self.refresh_runtime());
            }
            Message::Runtime(epoch,request,binding,result) => {
                if epoch!=self.epoch||request<self.runtime_applied{return Some(Task::none())}
                self.runtime_applied=request;if binding{self.device_busy=false;}
                match result {
                Ok(view) => {
                    self.snapshot = view.snapshot.or_else(|| self.snapshot.clone());
                    self.history = view.history;
                    self.dirty = view.dirty;
                    self.recovery = view.recovery;
                }
                Err(e) => self.status = e,
            }
            if self.resolve_pending&&!self.dirty&&!self.recovery{self.resolve_pending=false;let pending=self.pending.take();return Some(self.continue_pending(pending))}
            }
            Message::SelectBlock(id) => self.selected_block = Some(id),
            Message::MoveBlock(id,point)=>{self.block_positions.insert(id,point);},
            Message::UnsupportedRouting=>self.status="Structural routing changes are not verified. The confirmed device connection is unchanged.".into(),
            Message::PreviewAdjust(block, parameter, value) => {
                if !self.device_busy {
                    self.pending_parameters.insert((block, parameter), value);
                }
            }
            Message::CommitAdjust(block, parameter) => {
                if let Some(value) = self
                    .pending_parameters
                    .get(&(block.clone(), parameter.clone()))
                    .copied()
                {
                    return Some(self.update(Message::Adjust(block, parameter, value)));
                }
            }
            Message::Adjust(block, parameter, value) => {
                let Some(backend) = self.backend.clone() else {
                    return Some(Task::none());
                };
                self.device_busy = true;
                let change = Change {
                    block,
                    parameter,
                    value,
                };
                return Some(Task::perform(
                    blocking(move || {
                        let mut b = backend.lock().map_err(|_| "Backend unavailable")?;
                        b.adjust(
                            &format!("manual-{}", uuid::Uuid::new_v4()),
                            &[change],
                            || true,
                        )
                        .map(|_| ())
                    }),
                    Message::DeviceResult,
                ));
            }
            Message::Restore => {
                if self.agent_busy || self.device_busy {
                    return Some(Task::none());
                }
                let Some(backend) = self.backend.clone() else {
                    return Some(Task::none());
                };
                self.device_busy = true;
                return Some(Task::perform(
                    blocking(move || {
                        backend
                            .lock()
                            .map_err(|_| "Backend unavailable")?
                            .restore()
                            .map(|_| ())
                    }),
                    Message::DeviceResult,
                ));
            }
            Message::SaveDialog => {
                if !self.agent_busy && !self.device_busy && !self.recovery && self.snapshot.is_some() {
                    if let Some(snapshot) = &self.snapshot
                        && snapshot.context.target == axe_domain::Target::Hardware {
                        self.destination = snapshot.context.preset.to_string();
                    }
                    self.save_dialog = true;
                }
            }
            Message::Destination(s) => {
                self.destination = s.chars().filter(char::is_ascii_digit).take(4).collect()
            }
            Message::ConfirmSave => {
                if !self.save_dialog || self.device_busy || self.agent_busy || self.recovery {
                    return Some(Task::none());
                }
                let Ok(slot) = self.destination.parse::<u16>() else {
                    self.status = "Choose an explicit destination from 0 to 1023.".into();
                    return Some(Task::none());
                };
                if slot > 1023 {
                    self.status = "Destination must be 0–1023.".into();
                    return Some(Task::none());
                }
                let Some(backend) = self.backend.clone() else {
                    return Some(Task::none());
                };
                self.device_busy = true;
                self.save_dialog = false;
                let epoch = self.epoch;
                self.status = "Checking device context and saving to the selected destination…".into();
                return Some(Task::perform(
                    blocking(move || {
                        backend
                            .lock()
                            .map_err(|_| "Backend unavailable")?
                            .save(slot)
                    }),
                    move |result| Message::SaveResult(epoch, result),
                ));
            }
            Message::SaveResult(epoch, result) => {
                if epoch != self.epoch { return Some(Task::none()); }
                self.device_busy = false;
                self.status = match result {
                    Ok(axe_runtime::SaveOutcome::MockVerified) => {
                        if self.pending.is_some() { self.resolve_pending = true; }
                        "Test mock preset saved and verified. No hardware mutation was sent.".into()
                    }
                    Ok(axe_runtime::SaveOutcome::HardwareSentUnverified { destination }) => format!(
                        "Store request sent once to Axe-Fx preset {destination}. Verify persistence on the device; no automatic reload or retry was performed."
                    ),
                    Err(e) => e,
                };
                return Some(self.refresh_runtime());
            }
            Message::DeviceResult(result) => {
                self.device_busy = false;
                self.pending_parameters.clear();
                if result.is_ok()&&self.pending.is_some(){self.resolve_pending=true;}
                self.status = match result {
                    Ok(()) => "Test mock operation verified. No hardware mutation was sent.".into(),
                    Err(e) => e,
                };
                return Some(self.refresh_runtime());
            }
            Message::CancelDialog => {
                self.analysis_dialog=None;
                self.pending = None;
                self.save_dialog = false;
            }
            Message::LeaveUnsaved => {
                let pending = self.pending.take();
                return Some(self.continue_pending(pending));
            }
            Message::RequestClose => {
                self.submission_epoch += 1;
                self.agent_ready = false;
                let agent = self.agent.take();
                if let Some(a) = &agent { a.stop(); }
                self.agent_epoch += 1;
                let backend = self.backend.clone();
                self.device_busy = true;
                return Some(Task::perform(blocking(move || {
                    drop(agent); // Drain the revoked turn before inspecting durable changes.
                    match backend {
                        Some(b) => {
                            let b = b.lock().map_err(|_| "Backend unavailable")?;
                            Ok((b.dirty(), b.recovery()))
                        }
                        None => Ok((false, false)),
                    }
                }), Message::CloseChecked));
            }
            Message::CloseChecked(result) => {
                self.device_busy = false;
                self.agent_busy = false;
                match result {
                    Ok((dirty, recovery)) => {
                        self.dirty = dirty;
                        self.recovery = recovery;
                        if dirty || recovery { self.pending = Some(Pending::Close); }
                        else { return Some(self.continue_pending(Some(Pending::Close))); }
                    }
                    Err(e) => self.status = e,
                }
            }
            Message::Import(lane) => {
                let root = self.data_root.clone();
                return Some(Task::perform(
                    async move {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .add_filter(
                                "Audio",
                                &["wav", "flac", "aif", "aiff", "mp3", "ogg", "m4a"],
                            )
                            .pick_file()
                            .await
                        else {
                            return Message::ImportCancelled;
                        };
                        let path = file.path().to_owned();
                        let result = blocking(move || {
                            axe_audio::artifact::Library::open(root.join("audio"))?.import(&path)
                        })
                        .await;
                        Message::Imported(lane, result)
                    },
                    |m| m,
                ));
            }
            Message::Imported(lane, result) => match result {
                Ok(artifact) => {
                    if lane == Lane::Reference {
                        self.reference = Some(artifact)
                    } else {
                        self.recording = Some(artifact)
                    }
                    self.status = "Imported locally. No recording was uploaded.".into();
                }
                Err(e) => self.status = e,
            },
            Message::ImportCancelled => {}
            Message::ExcerptStart(s) => self.excerpt_start = s.chars().take(10).collect(),
            Message::ExcerptDuration(s) => self.excerpt_duration = s.chars().take(10).collect(),
            Message::Play(lane) => {
                let artifact = match lane {
                    Lane::Reference => self.reference.as_ref(),
                    Lane::Recording => self.recording.as_ref(),
                };
                let Some(artifact) = artifact.cloned() else {
                    return Some(Task::none());
                };
                let Ok((start, duration)) = self.excerpt() else {
                    self.status = "Choose a finite excerpt of up to 120 seconds.".into();
                    return Some(Task::none());
                };
                let pair=if self.level_match{self.reference.as_ref().zip(self.recording.as_ref()).map(|(a,b)|(a.id.clone(),b.id.clone()))}else{None};
                let root=self.data_root.clone();
                return Some(Task::perform(blocking(move||{
                    let library=axe_audio::artifact::Library::open(root.join("audio"))?;
                    let gains=if let Some((a,b))=pair{axe_audio::artifact::matched_gains(library.excerpt_rms(&a,start,duration)?,library.excerpt_rms(&b,start,duration)?)}else{(1.,1.)};
                    let gain=if lane==Lane::Reference{gains.0}else{gains.1};library.path(&artifact.id).map(|p|(p,start,duration,gain))
                }),Message::PlayReady));
            }
            Message::PlayReady(result) => match result {
                Ok((path, start, duration, gain)) => {
                    let (player, _) = self
                        .player
                        .get_or_insert_with(axe_audio::playback::Player::start);
                    let _=player.send(axe_audio::playback::Playback::Volume(self.volume));
                    if let Err(e) = player.send(axe_audio::playback::Playback::Play {
                        path,
                        start,
                        duration,
                        gain,
                    }) {
                        self.status = e
                    } else {self.status="Opening local audio output…".into();}
                }
                Err(e) => self.status = e,
            },
            Message::AudioEvent(event)=>{use axe_audio::playback::PlaybackEvent;match event{PlaybackEvent::Started=>{self.audio_playing=true;self.status="Playing local excerpt".into();},PlaybackEvent::Paused=>{self.audio_playing=false;self.status="Playback paused".into();},PlaybackEvent::Stopped=>{self.audio_playing=false;self.audio_position=0.;},PlaybackEvent::Position(p)=>self.audio_position=p,PlaybackEvent::Error(e)=>{self.audio_playing=false;self.status=e;}}},
            Message::Volume(v)=>{self.volume=v;if let Some((p,_))=&self.player{let _=p.send(axe_audio::playback::Playback::Volume(v));}},
            Message::ResumeAudio=>{if let Some((p,_))=&self.player{let _=p.send(axe_audio::playback::Playback::Resume);}},
            Message::PauseAudio => {
                if let Some((p, _)) = &self.player {
                    let _ = p.send(axe_audio::playback::Playback::Pause);
                }
            }
            Message::StopAudio => {
                if let Some((p, _)) = &self.player {
                    let _ = p.send(axe_audio::playback::Playback::Stop);
                }
            }
            Message::LevelMatch(value) => self.level_match = value,
            Message::Analyze(lane) => {
                if self.audio_busy {
                    return Some(Task::none());
                }
                let artifact = match lane {
                    Lane::Reference => self.reference.as_ref(),
                    Lane::Recording => self.recording.as_ref(),
                };
                let Some(artifact) = artifact.cloned() else {
                    return Some(Task::none());
                };
                let Ok((start, duration)) = self.excerpt() else {
                    self.status = "Choose a finite excerpt of up to 120 seconds.".into();
                    return Some(Task::none());
                };
                let root = self.data_root.clone();
                self.audio_busy = true;
                self.status = "Measuring local audio…".into();
                return Some(Task::perform(
                    blocking(move || {
                        let library = axe_audio::artifact::Library::open(root.join("audio"))?;
                        let path = library.path(&artifact.id)?;
                        let mut worker = axe_audio::worker::Worker::start(
                            &axe_audio::worker::Worker::bundled_path()?,
                        )?;
                        let report=worker.request("measure",serde_json::json!({"path":path,"artifact":artifact.id,"start":start,"duration":duration}))?;
                        library.save_report(&artifact.id, &report)?;
                        Ok((artifact.id, report))
                    }),
                    Message::Analyzed,
                ));
            }
            Message::Analyzed(result) => {
                self.audio_busy = false;
                match result {
                    Ok((id, report)) => {
                        self.analyses.insert(id, report);
                        self.status =
                            "Measurements ready. Attach a summary to share it with your assistant."
                                .into();
                    }
                    Err(e) => self.status = e,
                }
            }
            Message::ViewAnalysis(id)=>self.analysis_dialog=self.analyses.get(&id).cloned(),
            Message::CompareAudio=>{if let Some(a)=self.reference.as_ref().and_then(|a|self.analyses.get(&a.id))&&let Some(b)=self.recording.as_ref().and_then(|a|self.analyses.get(&a.id)){let am=&a["measurements"];let bm=&b["measurements"];let delta=|key:&str|bm[key].as_f64().unwrap_or(0.)-am[key].as_f64().unwrap_or(0.);self.analysis_dialog=Some(serde_json::json!({"kind":"descriptive_comparison","centroid_difference_hz":delta("spectral_centroid_hz"),"crest_difference_db":delta("crest_db"),"rms_difference_db":delta("rms_dbfs"),"limitations":["Different performances and excerpts affect these numbers. Confirm with listening; there is no exact tone-match score."]}));}},
            Message::Attach(id, checked) => {
                let Some(b) = self.backend.clone() else {
                    return Some(Task::none());
                };
                if let Err(e) = b
                    .lock()
                    .map_err(|_| "Backend unavailable".to_owned())
                    .and_then(|mut b| b.attach(&id, checked))
                {
                    self.status = e
                } else if checked {
                    self.attached.insert(id);
                } else {
                    self.attached.remove(&id);
                }
            }
            _ => return None,
        }
        Some(Task::none())
    }
    pub(super) fn excerpt(&self) -> Result<(f64, f64), String> {
        let start = self
            .excerpt_start
            .parse::<f64>()
            .map_err(|_| "Invalid start")?;
        let duration = self
            .excerpt_duration
            .parse::<f64>()
            .map_err(|_| "Invalid duration")?;
        if !start.is_finite()
            || !(0.0..=86400.).contains(&start)
            || !duration.is_finite()
            || duration <= 0.
            || duration > 120.
        {
            return Err("Invalid excerpt".into());
        }
        Ok((start, duration))
    }
    pub(super) fn continue_pending(&mut self, pending: Option<Pending>) -> Task<Message> {
        match pending {
            Some(Pending::Close) => {
                if let Some(agent) = self.agent.take() {
                    agent.stop();
                    Task::perform(
                        blocking(move || {
                            drop(agent);
                            Ok(())
                        }),
                        |_| Message::Exit,
                    )
                } else {
                    iced::exit()
                }
            }
            Some(Pending::Mock) => {
                self.dirty = false;
                self.update(Message::Mock)
            }
            Some(Pending::Capture) => {
                self.dirty = false;
                self.update(Message::Capture)
            }
            None => Task::none(),
        }
    }
}
