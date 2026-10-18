use crate::{Message, Studio, integration::Lane, style};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Element, Length};
fn control<'a>(name: &'a str, action: Option<Message>) -> iced::widget::Button<'a, Message> {
    button(text(name).size(12))
        .padding([6, 10])
        .style(style::neutral)
        .on_press_maybe(action)
}
impl Studio {
    pub(super) fn chat_view(&self) -> Element<'_, Message> {
        let available = self.snapshot.is_some() && !self.agent_busy;
        let providers = row![
            control(
                "Codex",
                available.then_some(Message::Choose(axe_agents::Provider::Codex))
            )
            .width(Length::Fill),
            control(
                "Claude",
                available.then_some(Message::Choose(axe_agents::Provider::Claude))
            )
            .width(Length::Fill)
        ]
        .spacing(8);
        let mut history = column![].spacing(12).width(Length::Fill);
        if self.history.is_empty() {
            history = history.push(
                container(
                    column![
                        text("Shape your sound").size(18),
                        text(if self.snapshot.is_none() {
                            "Read a preset, then choose your assistant."
                        } else {
                            "Connect an assistant, then describe what you hear."
                        })
                        .size(13)
                        .color(style::MUTED)
                    ]
                    .spacing(10),
                )
                .padding(16)
                .style(style::inset),
            );
        }
        for m in &self.history {
            let role = if m.role == "user" {
                "You"
            } else if m.role == "tool" {
                "Activity"
            } else {
                m.provider.as_str()
            };
            let mut bubble = column![
                text(role).size(11).color(if m.role == "user" {
                    style::BLUE
                } else {
                    style::MUTED
                }),
                text(if m.role == "tool" {
                    m.body.lines().next().unwrap_or("")
                } else {
                    &m.body
                })
                .size(if m.role == "tool" { 11 } else { 13 })
            ]
            .spacing(6);
            if m.status == "not_sent" {
                bubble = bubble.push(
                    text("Cancelled before submission · not replayed")
                        .size(10)
                        .color(style::AMBER),
                );
            }
            if m.status == "interrupted" {
                bubble = bubble.push(
                    text("Interrupted · retained locally")
                        .size(10)
                        .color(style::AMBER),
                );
            }
            let content =
                container(bubble)
                    .padding(12)
                    .width(Length::Fill)
                    .style(if m.role == "user" {
                        style::selected
                    } else {
                        style::inset
                    });
            history = history.push(content);
        }
        let composer = text_input("Tell me what you hear…", &self.composer)
            .on_input(Message::Composer)
            .on_submit_maybe((self.agent_ready && !self.agent_busy).then_some(Message::Send))
            .size(13)
            .padding(12);
        column![
            providers,
            text_input("Claude API key (session only)", &self.claude_key)
                .secure(true)
                .on_input(Message::ClaudeKey)
                .size(11)
                .padding(8),
            text(self.agent_error.as_deref().unwrap_or(""))
                .size(11)
                .color(style::AMBER),
            scrollable(history).spacing(8).height(Length::Fill),
            text(if self.agent_busy {
                "Assistant working…"
            } else if self.agent_ready {
                "Connected · axefx-live initialized"
            } else {
                "Choose a provider to connect"
            })
            .size(11)
            .color(style::MUTED),
            composer,
            row![
                text(format!("{} attached summaries", self.attached.len()))
                    .size(10)
                    .color(style::MUTED),
                space::horizontal(),
                control("Stop", self.agent_busy.then_some(Message::Stop)),
                control(
                    "Send",
                    (self.agent_ready && !self.agent_busy && !self.composer.trim().is_empty())
                        .then_some(Message::Send)
                )
            ]
            .spacing(6)
            .align_y(iced::Center)
        ]
        .spacing(10)
        .into()
    }
    fn audio_lane<'a>(
        &'a self,
        which: Lane,
        title: &'static str,
        artifact: Option<&'a axe_audio::artifact::Artifact>,
    ) -> Element<'a, Message> {
        let mut c = column![
            row![
                text(title).size(12),
                space::horizontal(),
                control("Import", Some(Message::Import(which)))
            ]
            .align_y(iced::Center)
        ]
        .spacing(5);
        if let Some(a) = artifact {
            c = c
                .push(
                    text(format!("{} · {:.1}s", a.name, a.duration))
                        .size(10)
                        .color(style::MUTED),
                )
                .push(
                    iced::widget::canvas(crate::waveform::Waveform {
                        peaks: &a.waveform,
                        blue: which == Lane::Recording,
                    })
                    .width(Length::Fill)
                    .height(30),
                );
            let mut actions = row![
                control("▶", Some(Message::Play(which))),
                control(
                    "Analyze",
                    (!self.audio_busy).then_some(Message::Analyze(which))
                )
            ]
            .spacing(5)
            .align_y(iced::Center);
            if self.analyses.contains_key(&a.id) {
                let id = a.id.clone();
                actions = actions
                    .push(control(
                        "Details",
                        Some(Message::ViewAnalysis(a.id.clone())),
                    ))
                    .push(
                        iced::widget::checkbox(self.attached.contains(&a.id))
                            .label("Attach summary")
                            .text_size(10)
                            .on_toggle(move |v| Message::Attach(id.clone(), v)),
                    );
            }
            c = c.push(actions);
        } else {
            c = c.push(
                container(text("No file loaded").size(12).color(style::MUTED))
                    .center(Length::Fill)
                    .height(65),
            );
        }
        container(c)
            .padding(8)
            .width(Length::Fill)
            .style(style::inset)
            .into()
    }
    pub(super) fn audio_view(&self) -> Element<'_, Message> {
        row![
            self.audio_lane(Lane::Reference, "Reference guitar", self.reference.as_ref()),
            self.audio_lane(Lane::Recording, "Your recording", self.recording.as_ref()),
            column![
                row![
                    text_input("Start s", &self.excerpt_start)
                        .on_input(Message::ExcerptStart)
                        .size(10)
                        .width(62)
                        .padding(5),
                    text_input("Length s", &self.excerpt_duration)
                        .on_input(Message::ExcerptDuration)
                        .size(10)
                        .width(62)
                        .padding(5)
                ]
                .spacing(5),
                row![
                    control(
                        if self.audio_playing {
                            "Pause"
                        } else {
                            "Resume"
                        },
                        Some(if self.audio_playing {
                            Message::PauseAudio
                        } else {
                            Message::ResumeAudio
                        })
                    ),
                    control("Stop", Some(Message::StopAudio)),
                    control(
                        "Compare",
                        (self
                            .reference
                            .as_ref()
                            .is_some_and(|a| self.analyses.contains_key(&a.id))
                            && self
                                .recording
                                .as_ref()
                                .is_some_and(|a| self.analyses.contains_key(&a.id)))
                        .then_some(Message::CompareAudio)
                    )
                ]
                .spacing(5),
                row![
                    text(format!("{:.1}s", self.audio_position)).size(10),
                    iced::widget::slider(0.0..=1.0, self.volume, Message::Volume)
                        .width(88)
                        .step(0.01_f32)
                ]
                .spacing(5),
                iced::widget::checkbox(self.level_match)
                    .label("A/B level match")
                    .text_size(10)
                    .on_toggle(Message::LevelMatch)
            ]
            .spacing(8)
        ]
        .spacing(10)
        .align_y(iced::Center)
        .into()
    }
    pub(super) fn modal<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if self.pending.is_none() && !self.save_dialog && self.analysis_dialog.is_none() {
            return base;
        }
        let dialog = if let Some(report) = &self.analysis_dialog {
            column![
                text("Local audio measurements").size(20),
                scrollable(text(measurement_text(report)).size(12)).height(420),
                control("Close", Some(Message::CancelDialog))
            ]
            .spacing(16)
        } else if self.save_dialog {
            let hardware = self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.context.target == axe_domain::Target::Hardware);
            let destination_key = format!("mock.preset.{}", self.destination);
            let replaces_existing = self
                .store
                .as_ref()
                .and_then(|store| store.setting(&destination_key).ok().flatten())
                .is_some();
            column![
                text("Save tone to destination").size(20),
                text(if hardware {
                    format!("Axe-Fx III · overwrite device preset {} with the current edit buffer. Use a backed-up test destination for your manual verification. The store request is recovered from Axe-Edit; persistence is not yet hardware-verified.", self.destination)
                } else if replaces_existing {
                    format!("Test mock only · destination {} already exists and will be overwritten.", self.destination)
                } else {
                    format!("Test mock only · simulated destination {} will be created.", self.destination)
                }).size(13),
                text_input("Preset destination 0–1023", &self.destination)
                    .on_input(Message::Destination)
                    .padding(10),
                row![
                    control("Cancel", Some(Message::CancelDialog)),
                    control("Save to selected destination", Some(Message::ConfirmSave))
                ]
                .spacing(8)
            ]
            .spacing(16)
        } else {
            column![
                text("Unsaved changes").size(20),
                text(if self.recovery {
                    "Recovery is required. Inspect the device before choosing what to do."
                } else {
                    "Save the edited tone, restore the original values, or leave it unsaved."
                })
                .size(13),
                row![
                    control("Save…", (!self.recovery).then_some(Message::SaveDialog)),
                    control("Restore", (!self.device_busy).then_some(Message::Restore)),
                    control("Leave unsaved", Some(Message::LeaveUnsaved)),
                    control("Cancel", Some(Message::CancelDialog))
                ]
                .spacing(8)
            ]
            .spacing(16)
        };
        iced::widget::stack![
            base,
            iced::widget::opaque(
                container(
                    container(dialog)
                        .padding(24)
                        .max_width(620)
                        .style(style::panel)
                )
                .center(Length::Fill)
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Color::BLACK.scale_alpha(0.75).into()),
                    ..Default::default()
                })
            )
        ]
        .into()
    }
}

fn measurement_text(report: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    let comparison = report["kind"] == "descriptive_comparison";
    let metrics = if comparison {
        report
    } else {
        &report["measurements"]
    };
    if !comparison && let Some(duration) = report["excerpt"]["duration"].as_f64() {
        lines.push(format!("Selected excerpt: {duration:.1} seconds\n"));
    }
    for (key, label, unit) in [
        ("rms_dbfs", "Average level", "dBFS"),
        ("peak_dbfs", "Peak level", "dBFS"),
        ("crest_db", "Crest factor", "dB"),
        ("spectral_centroid_hz", "Spectral centroid", "Hz"),
        ("stereo_correlation", "Stereo correlation", ""),
        (
            "centroid_difference_hz",
            "Recording − reference centroid",
            "Hz",
        ),
        ("crest_difference_db", "Recording − reference crest", "dB"),
        ("rms_difference_db", "Recording − reference level", "dB"),
    ] {
        if let Some(value) = metrics[key].as_f64() {
            lines.push(format!("{label}: {value:.2} {unit}"));
        }
    }
    if let Some(value) = metrics["clipped_sample_fraction"].as_f64() {
        lines.push(format!("Clipped samples: {:.3}%", value * 100.));
    }
    if let Some(bands) = metrics["band_energy"].as_object() {
        lines.push("\nFrequency balance".into());
        for (band, value) in bands {
            if let Some(v) = value.as_f64() {
                lines.push(format!("{}: {:.1}%", band.replace('_', " "), v * 100.));
            }
        }
    }
    if let Some(values) = metrics["envelope_p10_p50_p90"].as_array() {
        let values: Vec<_> = values
            .iter()
            .filter_map(|v| v.as_f64())
            .map(|v| format!("{v:.3}"))
            .collect();
        lines.push(format!(
            "\nEnvelope (10 / 50 / 90%): {}",
            values.join(" / ")
        ));
    }
    if let Some(limits) = report["limitations"].as_array() {
        lines.push("\nListening notes".into());
        lines.extend(limits.iter().filter_map(|v| v.as_str()).map(str::to_owned));
    }
    lines.join("\n\n")
}
