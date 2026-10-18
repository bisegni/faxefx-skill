use crate::{Message, Pane, Studio, routing, style};
use iced::widget::{button, column, container, pane_grid, row, scrollable, space, text};
use iced::{Element, Font, Length};
fn label<'a>(s: impl Into<String>) -> iced::widget::Text<'a> {
    text(s.into()).size(12).color(style::MUTED)
}
fn heading<'a>(s: impl Into<String>) -> iced::widget::Text<'a> {
    text(s.into()).size(14).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    })
}
fn control<'a>(name: &'a str, action: Option<Message>) -> iced::widget::Button<'a, Message> {
    button(text(name).size(12))
        .padding([7, 12])
        .style(style::neutral)
        .on_press_maybe(action)
}
fn badge<'a>(name: impl Into<String>, color: iced::Color) -> Element<'a, Message> {
    container(text(name.into()).size(11).color(color))
        .padding([5, 9])
        .style(style::inset)
        .into()
}
impl Studio {
    pub(super) fn content(&self, p: Pane) -> Element<'_, Message> {
        match p {
            Pane::Presets => {
                let current = if let Some(s) = &self.snapshot {
                    container(
                        row![
                            text(format!("{:03}", s.context.preset)).size(13),
                            text(&s.name).size(13)
                        ]
                        .spacing(12),
                    )
                    .padding(10)
                    .width(Length::Fill)
                    .style(style::selected)
                } else {
                    container(
                        column![
                            heading("Connect your Axe-Fx"),
                            label("Read the current preset to begin.")
                        ]
                        .spacing(7),
                    )
                    .padding(8)
                    .width(Length::Fill)
                    .style(style::inset)
                };
                let mut body = column![
                    current,
                    control("Test mock", (!self.busy).then_some(Message::Mock)).width(Length::Fill),
                    label("Device connection")
                ]
                .spacing(10);
                if let Some(ports) = &self.endpoints {
                    body = body
                        .push(
                            iced::widget::pick_list(
                                ports.inputs.clone(),
                                self.input.clone(),
                                Message::Input,
                            )
                            .placeholder("MIDI input")
                            .text_size(12)
                            .padding(7)
                            .width(Length::Fill),
                        )
                        .push(
                            iced::widget::pick_list(
                                ports.outputs.clone(),
                                self.output.clone(),
                                Message::Output,
                            )
                            .placeholder("MIDI output")
                            .text_size(12)
                            .padding(7)
                            .width(Length::Fill),
                        );
                }
                body = body.push(
                    row![
                        control("Read USB", (!self.busy).then_some(Message::Capture)),
                        control("Retry", (!self.busy).then_some(Message::Retry))
                    ]
                    .spacing(6),
                );
                scrollable(body.width(Length::Fill))
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(4)
                            .scroller_width(4)
                            .spacing(8),
                    ))
                    .height(Length::Fill)
                    .into()
            }
            Pane::Inspector => {
                let selected = self.snapshot.as_ref().and_then(|s| {
                    s.blocks
                        .iter()
                        .find(|b| self.selected_block.as_ref() == Some(&b.id))
                        .or_else(|| s.blocks.iter().find(|b| !b.parameters.is_empty()))
                        .or_else(|| s.blocks.first())
                });
                let mut body = column![].spacing(9);
                if let Some(b) = selected {
                    let registry = crate::effects::EffectWidgetRegistry::default();
                    let family = registry.presentation(&b.family);
                    body = body
                        .push(
                            container(
                                row![
                                    text(family.icon).color(family.accent).size(15),
                                    heading(&b.label),
                                    space::horizontal(),
                                    text("◉").color(style::MUTED)
                                ]
                                .spacing(10)
                                .align_y(iced::Center),
                            )
                            .padding(10)
                            .style(style::inset),
                        )
                        .push(
                            row![
                                label("Channel"),
                                space::horizontal(),
                                badge(
                                    self.snapshot
                                        .as_ref()
                                        .and_then(|s| s.context.channel)
                                        .map(|v| ((b'A' + v) as char).to_string())
                                        .unwrap_or("Unknown".into()),
                                    style::BLUE
                                )
                            ]
                            .align_y(iced::Center),
                        )
                        .push(
                            row![
                                label("Model"),
                                space::horizontal(),
                                badge("Unknown", style::MUTED)
                            ]
                            .align_y(iced::Center),
                        )
                        .push(iced::widget::rule::horizontal(1))
                        .push(label("Observed parameters"));
                    for p in &b.parameters {
                        body = body.push(
                            row![
                                text(&p.label).size(13).width(Length::Fill),
                                iced::widget::canvas(crate::knob::Knob {
                                    value: p.confirmed,
                                    range: p.range
                                })
                                .width(34)
                                .height(34),
                                container(
                                    text(
                                        p.confirmed
                                            .map(|v| format!("{v:.1}"))
                                            .unwrap_or("—".into())
                                    )
                                    .size(12)
                                )
                                .padding([5, 8])
                                .style(style::inset)
                            ]
                            .spacing(10)
                            .align_y(iced::Center),
                        );
                        if let (Some(value), Some((min, max))) = (p.confirmed, p.range)
                            && self
                                .snapshot
                                .as_ref()
                                .is_some_and(|s| s.context.target == axe_domain::Target::TestMock)
                            && p.writable
                            && !self.agent_busy
                        {
                            let block = b.id.clone();
                            let parameter = p.id.clone();
                            body = body.push(
                                iced::widget::slider(
                                    min..=max,
                                    self.pending_parameters
                                        .get(&(block.clone(), parameter.clone()))
                                        .copied()
                                        .unwrap_or(value),
                                    {
                                        let block = block.clone();
                                        let parameter = parameter.clone();
                                        move |v| {
                                            Message::PreviewAdjust(
                                                block.clone(),
                                                parameter.clone(),
                                                v,
                                            )
                                        }
                                    },
                                )
                                .on_release(Message::CommitAdjust(block, parameter))
                                .step(0.1),
                            );
                        }
                    }
                    if b.parameters.is_empty() {
                        body =
                            body.push(label("Parameter values are not available for this block."));
                    }
                    body = body.push(iced::widget::rule::horizontal(1)).push(label(
                        if self
                            .snapshot
                            .as_ref()
                            .is_some_and(|s| s.context.target == axe_domain::Target::TestMock)
                        {
                            "●  Simulated values · live controls"
                        } else {
                            "●  Device capture · read-only"
                        },
                    ));
                } else {
                    body = body.push(heading("No block selected")).push(label(
                        "Connect a device or open Test mock to inspect its blocks.",
                    ));
                }
                scrollable(body.width(Length::Fill))
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(4)
                            .scroller_width(4)
                            .spacing(8),
                    ))
                    .into()
            }
            Pane::Routing => {
                let mut body = column![
                    row![
                        badge(
                            if self.snapshot.is_some() {
                                "●  Captured routing"
                            } else {
                                "○  Awaiting device"
                            },
                            style::MUTED
                        ),
                        space::horizontal(),
                        control(
                            "Auto layout",
                            self.snapshot.as_ref().map(|_| Message::AutoLayout)
                        ),
                        iced::widget::checkbox(self.simple_cables)
                            .label("Simple cables")
                            .text_size(11)
                            .on_toggle(Message::SimpleCables)
                    ]
                    .align_y(iced::Center)
                ]
                .spacing(6);
                if let Some(s) = &self.snapshot {
                    body = body
                        .push(
                            iced::widget::canvas(routing::RoutingCanvas {
                                snapshot: s,
                                simple: self.simple_cables,
                                selected: self.selected_block.as_deref(),
                                positions: &self.block_positions,
                                viewport: self.routing_viewport.clone(),
                            })
                            .width(Length::Fill)
                            .height(Length::Fill),
                        )
                        .push(label(
                            s.edges
                                .iter()
                                .map(|(a, b)| format!("{a} → {b}"))
                                .collect::<Vec<_>>()
                                .join("    "),
                        ));
                } else {
                    body = body.push(
                        container(
                            column![
                                text("Your signal chain").size(21),
                                label("Connect your Axe-Fx to see its current routing."),
                                label("Or choose Test mock to explore the workspace.")
                            ]
                            .spacing(12)
                            .align_x(iced::Center),
                        )
                        .center(Length::Fill)
                        .style(style::inset),
                    );
                }
                body.into()
            }
            Pane::Chat => self.chat_view(),
            Pane::Audio => self.audio_view(),
        }
    }
    pub(super) fn view(&self) -> Element<'_, Message> {
        let target = self.snapshot.as_ref().map(|s| s.context.target);
        let identity = match target {
            Some(axe_domain::Target::TestMock) => "●  Test mock",
            Some(axe_domain::Target::Hardware) => "●  Axe-Fx III · read-only",
            None => "○  Device disconnected",
        };
        let name = self
            .snapshot
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("No preset loaded");
        let slot = self
            .snapshot
            .as_ref()
            .map(|s| format!("{:03}", s.context.preset))
            .unwrap_or("—".into());
        let toolbar = container(
            row![
                column![heading("Axe Studio"), label("Live audition")].spacing(3),
                container(text(slot).size(24)).padding([0, 12]),
                text(name).size(19),
                space::horizontal(),
                badge(
                    identity,
                    if target == Some(axe_domain::Target::TestMock) {
                        style::AMBER
                    } else {
                        style::MUTED
                    }
                ),
                control(
                    "Restore original",
                    (self
                        .snapshot
                        .as_ref()
                        .is_some_and(|s| s.context.target == axe_domain::Target::TestMock)
                        && !self.device_busy
                        && !self.agent_busy)
                        .then_some(Message::Restore)
                ),
                button(text("Save tone").size(12))
                    .padding([8, 18])
                    .style(style::primary)
                    .on_press_maybe(
                        (self
                            .snapshot
                            .as_ref()
                            .is_some_and(|s| s.context.target == axe_domain::Target::TestMock)
                            && !self.agent_busy
                            && !self.recovery)
                            .then_some(Message::SaveDialog)
                    )
            ]
            .spacing(12)
            .align_y(iced::Center),
        )
        .padding([12, 14])
        .style(style::header);
        let layouts = row![
            label("Workspace"),
            iced::widget::text_input("Layout name", &self.layout_name)
                .on_input(Message::LayoutName)
                .size(11)
                .padding(5)
                .width(125),
            control("Save layout", Some(Message::SaveLayout)),
            control("Load", Some(Message::LoadLayout)),
            space::horizontal(),
            label("Focus pane · ⌘/Ctrl + Alt + arrows to swap"),
            control("Reset layout", Some(Message::Reset))
        ]
        .spacing(8)
        .align_y(iced::Center);
        let grid = pane_grid(&self.panes, |_, pane, _| {
            pane_grid::Content::new(
                container(self.content(*pane))
                    .padding(if matches!(pane, Pane::Audio) {
                        8
                    } else if matches!(pane, Pane::Routing) {
                        10
                    } else {
                        12
                    })
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .title_bar(
                pane_grid::TitleBar::new(
                    row![
                        heading(pane.title()),
                        space::horizontal(),
                        text("⠿").size(13).color(style::MUTED)
                    ]
                    .align_y(iced::Center),
                )
                .padding([9, 12])
                .style(style::header),
            )
            .style(style::panel)
        })
        .spacing(8)
        .on_click(Message::FocusPane)
        .on_drag(Message::Drag)
        .on_resize(8, Message::Resize)
        .height(Length::Fill);
        let status = container(
            row![
                text("●").size(10).color(style::AMBER),
                text(&self.status).size(11),
                space::horizontal(),
                label("Development preview")
            ]
            .spacing(8)
            .align_y(iced::Center),
        )
        .padding([7, 10])
        .style(style::inset);
        self.modal(
            column![toolbar, layouts, grid, status]
                .spacing(8)
                .padding(12)
                .into(),
        )
    }
}
