//! Native routing presentation; coordinates and cable styling never alter routing.
use crate::{Message, effects::EffectWidgetRegistry, style};
use axe_domain::{Block, Snapshot};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::{cell::Cell, collections::BTreeMap};
pub struct RoutingCanvas<'a> {
    pub snapshot: &'a Snapshot,
    pub simple: bool,
    pub selected: Option<&'a str>,
    pub positions: &'a BTreeMap<String, (f32, f32)>,
    pub viewport: std::sync::Arc<std::sync::Mutex<Size>>,
}
pub struct RoutingState {
    drag: Option<(String, Point, Point)>,
    loose: Option<Point>,
    pointer: Point,
    awake: Cell<bool>,
}
impl Default for RoutingState {
    fn default() -> Self {
        Self {
            drag: None,
            loose: None,
            pointer: Point::ORIGIN,
            awake: Cell::new(true),
        }
    }
}
impl RoutingCanvas<'_> {
    fn geometry(&self, bounds: Rectangle) -> (f32, f32, impl Fn(&Block) -> Point + use<'_>) {
        let cell_w = (bounds.width - 24.).max(14.) / 14.;
        let cell_h = (bounds.height - 24.).max(14.) / 6.;
        let width = (cell_w * 0.62).clamp(12., 68.);
        let height = (cell_h * 0.68).clamp(12., 50.);
        let automatic = auto_layout(self.snapshot, bounds.size());
        (width, height, move |block: &Block| {
            let p = self
                .positions
                .get(&block.id)
                .copied()
                .or_else(|| automatic.get(&block.id).copied())
                .unwrap_or((8., 8.));
            snap_to_cell(Point::new(p.0, p.1), bounds.size())
        })
    }
}
/// Keep every block in its Axe-Fx grid cell (14 columns by 6 rows).
/// The grid is scaled to the current pane; card size stays inside each cell.
pub fn auto_layout(snapshot: &Snapshot, size: Size) -> BTreeMap<String, (f32, f32)> {
    let cell_w = (size.width - 24.).max(14.) / 14.;
    let cell_h = (size.height - 24.).max(14.) / 6.;
    let width = (cell_w * 0.62).clamp(12., 68.);
    let height = (cell_h * 0.68).clamp(12., 50.);
    let mut positions = BTreeMap::new();
    for block in &snapshot.blocks {
        let col = usize::from(block.position.0).min(13);
        let row = usize::from(block.position.1).min(5);
        let x = 12. + col as f32 * cell_w + (cell_w - width) / 2.;
        let y = 12. + row as f32 * cell_h + (cell_h - height) / 2.;
        positions.insert(block.id.clone(), (x, y));
    }
    positions
}

/// Snap a tile's top-left corner to the nearest slot in the visible 14 x 6 grid.
fn snap_to_cell(point: Point, size: Size) -> Point {
    let cell_w = (size.width - 24.).max(14.) / 14.;
    let cell_h = (size.height - 24.).max(14.) / 6.;
    let width = (cell_w * 0.62).clamp(12., 68.);
    let height = (cell_h * 0.68).clamp(12., 50.);
    let first_x = 12. + (cell_w - width) / 2.;
    let first_y = 12. + (cell_h - height) / 2.;
    let col = ((point.x - first_x) / cell_w).round().clamp(0., 13.);
    let row = ((point.y - first_y) / cell_h).round().clamp(0., 5.);
    Point::new(first_x + col * cell_w, first_y + row * cell_h)
}

fn line(f: &mut Frame, a: Point, b: Point, color: Color, width: f32) {
    f.stroke(
        &Path::line(a, b),
        Stroke::default().with_color(color).with_width(width),
    );
}
fn icon(f: &mut Frame, family: &str, p: Point, color: Color) {
    let stroke = Stroke::default().with_color(color).with_width(1.7);
    match family {
        "amp" => {
            let body = Path::rounded_rectangle(
                Point::new(p.x - 11., p.y - 8.),
                Size::new(22., 16.),
                3.into(),
            );
            f.stroke(&body, stroke);
            f.stroke(
                &Path::rectangle(Point::new(p.x - 7., p.y - 2.), Size::new(14., 8.)),
                stroke,
            );
            line(
                f,
                Point::new(p.x - 8., p.y - 4.),
                Point::new(p.x + 8., p.y - 4.),
                color,
                1.,
            );
            for x in [-7., 0., 7.] {
                f.fill(&Path::circle(Point::new(p.x + x, p.y - 5.), 1.), color);
            }
        }
        "input" => {
            let wave = Path::new(|b| {
                b.move_to(Point::new(p.x - 13., p.y));
                b.line_to(Point::new(p.x - 7., p.y));
                b.line_to(Point::new(p.x - 3., p.y - 10.));
                b.line_to(Point::new(p.x + 3., p.y + 10.));
                b.line_to(Point::new(p.x + 7., p.y));
                b.line_to(Point::new(p.x + 13., p.y));
            });
            f.stroke(&wave, stroke);
        }
        "output" => {
            line(
                f,
                Point::new(p.x - 12., p.y),
                Point::new(p.x + 12., p.y),
                color,
                1.7,
            );
            line(
                f,
                Point::new(p.x + 5., p.y - 7.),
                Point::new(p.x + 12., p.y),
                color,
                1.7,
            );
            line(
                f,
                Point::new(p.x + 5., p.y + 7.),
                Point::new(p.x + 12., p.y),
                color,
                1.7,
            );
        }
        "cab" | "ir-player" | "nam-player" => {
            let cabinet = Path::rounded_rectangle(
                Point::new(p.x - 9., p.y - 10.),
                Size::new(18., 20.),
                2.into(),
            );
            f.stroke(&cabinet, stroke);
            f.stroke(&Path::circle(Point::new(p.x, p.y - 3.), 4.5), stroke);
            f.stroke(&Path::circle(Point::new(p.x, p.y + 6.), 2.), stroke);
        }
        "drive" | "dynamic-distortion" | "ringmod" => {
            let bolt = Path::new(|b| {
                b.move_to(Point::new(p.x + 2., p.y - 11.));
                b.line_to(Point::new(p.x - 7., p.y + 1.));
                b.line_to(Point::new(p.x - 1., p.y + 1.));
                b.line_to(Point::new(p.x - 3., p.y + 11.));
                b.line_to(Point::new(p.x + 8., p.y - 3.));
                b.line_to(Point::new(p.x + 2., p.y - 3.));
                b.close();
            });
            f.stroke(&bolt, stroke);
        }
        "comp" | "multicomp" | "gate" => {
            let envelope = Path::new(|b| {
                b.move_to(Point::new(p.x - 11., p.y + 7.));
                b.line_to(Point::new(p.x - 6., p.y + 7.));
                b.line_to(Point::new(p.x - 3., p.y - 7.));
                b.line_to(Point::new(p.x + 1., p.y - 7.));
                b.line_to(Point::new(p.x + 4., p.y + 2.));
                b.line_to(Point::new(p.x + 11., p.y + 2.));
            });
            f.stroke(&envelope, stroke);
            line(
                f,
                Point::new(p.x - 10., p.y - 2.),
                Point::new(p.x + 10., p.y - 2.),
                color,
                0.8,
            );
        }
        "geq" | "peq" => {
            for (x, y) in [(-7., -4.), (0., 3.), (7., -1.)] {
                line(
                    f,
                    Point::new(p.x + x, p.y - 9.),
                    Point::new(p.x + x, p.y + 9.),
                    color,
                    1.2,
                );
                f.fill(&Path::circle(Point::new(p.x + x, p.y + y), 2.2), color);
            }
        }
        "delay" | "multitap" | "megatap" | "tentap" | "feedback-send" | "feedback-return" => {
            for (x, r) in [(-6., 5.), (0., 3.5), (6., 2.)] {
                f.stroke(&Path::circle(Point::new(p.x + x, p.y), r), stroke);
            }
            line(
                f,
                Point::new(p.x - 10., p.y + 8.),
                Point::new(p.x + 10., p.y + 8.),
                color,
                1.,
            );
        }
        "reverb" | "plex" | "resonator" => {
            for r in [3., 6.5, 10.] {
                f.stroke(&Path::circle(Point::new(p.x, p.y), r), stroke);
            }
            f.fill(&Path::circle(Point::new(p.x, p.y), 1.6), color);
        }
        "chorus" | "flanger" | "phaser" | "rotary" | "tremolo" => {
            let wave = Path::new(|b| {
                b.move_to(Point::new(p.x - 12., p.y));
                b.line_to(Point::new(p.x - 9., p.y - 7.));
                b.line_to(Point::new(p.x - 6., p.y));
                b.line_to(Point::new(p.x - 3., p.y + 7.));
                b.line_to(Point::new(p.x, p.y));
                b.line_to(Point::new(p.x + 3., p.y - 7.));
                b.line_to(Point::new(p.x + 6., p.y));
                b.line_to(Point::new(p.x + 9., p.y + 7.));
                b.line_to(Point::new(p.x + 12., p.y));
            });
            f.stroke(&wave, stroke);
        }
        "filter" | "wah" | "formant" => {
            let curve = Path::new(|b| {
                b.move_to(Point::new(p.x - 11., p.y + 7.));
                b.line_to(Point::new(p.x - 6., p.y + 6.));
                b.line_to(Point::new(p.x - 2., p.y - 7.));
                b.line_to(Point::new(p.x + 2., p.y - 9.));
                b.line_to(Point::new(p.x + 6., p.y + 5.));
                b.line_to(Point::new(p.x + 11., p.y + 7.));
            });
            f.stroke(&curve, stroke);
            line(
                f,
                Point::new(p.x - 11., p.y + 9.),
                Point::new(p.x + 11., p.y + 9.),
                color,
                0.8,
            );
        }
        "mixer" | "multiplexer" | "crossover" => {
            let branches = Path::new(|b| {
                b.move_to(Point::new(p.x - 11., p.y));
                b.line_to(Point::new(p.x - 3., p.y));
                b.move_to(Point::new(p.x - 3., p.y));
                b.line_to(Point::new(p.x + 5., p.y - 8.));
                b.move_to(Point::new(p.x - 3., p.y));
                b.line_to(Point::new(p.x + 5., p.y + 8.));
                b.move_to(Point::new(p.x + 5., p.y - 8.));
                b.line_to(Point::new(p.x + 11., p.y - 8.));
                b.move_to(Point::new(p.x + 5., p.y + 8.));
                b.line_to(Point::new(p.x + 11., p.y + 8.));
            });
            f.stroke(&branches, stroke);
            f.fill(&Path::circle(Point::new(p.x - 3., p.y), 2.), color);
        }
        "volume" | "enhancer" => {
            for (x, y) in [(-6., 2.), (0., -4.), (6., 5.)] {
                line(
                    f,
                    Point::new(p.x + x, p.y - 9.),
                    Point::new(p.x + x, p.y + 9.),
                    color,
                    1.1,
                );
                f.fill(
                    &Path::rounded_rectangle(
                        Point::new(p.x + x - 2., p.y + y - 2.),
                        Size::new(4., 4.),
                        1.into(),
                    ),
                    color,
                );
            }
        }
        "pitch" | "synth" | "vocoder" | "tone-match" | "rta" | "tuner" => {
            let meter = Path::rounded_rectangle(
                Point::new(p.x - 11., p.y - 8.),
                Size::new(22., 16.),
                3.into(),
            );
            f.stroke(&meter, stroke);
            let trace = Path::new(|b| {
                b.move_to(Point::new(p.x - 8., p.y + 3.));
                b.line_to(Point::new(p.x - 4., p.y + 3.));
                b.line_to(Point::new(p.x - 1., p.y - 4.));
                b.line_to(Point::new(p.x + 2., p.y + 4.));
                b.line_to(Point::new(p.x + 5., p.y - 2.));
                b.line_to(Point::new(p.x + 8., p.y - 2.));
            });
            f.stroke(&trace, stroke);
        }
        "looper" => {
            f.stroke(&Path::circle(Point::new(p.x, p.y), 8.), stroke);
            let arrow = Path::new(|b| {
                b.move_to(Point::new(p.x + 3., p.y - 10.));
                b.line_to(Point::new(p.x + 10., p.y - 9.));
                b.line_to(Point::new(p.x + 8., p.y - 3.));
            });
            f.fill(&arrow, color);
        }
        "midiblock" | "footcontroller" | "preset-fc" | "perform" => {
            let pedal = Path::rounded_rectangle(
                Point::new(p.x - 10., p.y - 9.),
                Size::new(20., 18.),
                3.into(),
            );
            f.stroke(&pedal, stroke);
            f.stroke(&Path::circle(Point::new(p.x, p.y + 3.), 3.), stroke);
            for x in [-5., 0., 5.] {
                f.fill(&Path::circle(Point::new(p.x + x, p.y - 5.), 0.9), color);
            }
        }
        "ircapture" => {
            let cabinet = Path::rounded_rectangle(
                Point::new(p.x - 9., p.y - 9.),
                Size::new(18., 18.),
                2.into(),
            );
            f.stroke(&cabinet, stroke);
            f.stroke(&Path::circle(Point::new(p.x, p.y), 5.), stroke);
            f.fill(&Path::circle(Point::new(p.x, p.y), 1.5), color);
        }
        "wire" | "shunt" => line(
            f,
            Point::new(p.x - 12., p.y),
            Point::new(p.x + 12., p.y),
            color,
            1.7,
        ),
        _ => {
            // Unknown blocks still get a neutral effect-module glyph; the family
            // label carries identity, without implying unsupported semantics.
            let module = Path::rounded_rectangle(
                Point::new(p.x - 10., p.y - 8.),
                Size::new(20., 16.),
                3.into(),
            );
            f.stroke(&module, stroke);
            for y in [-3., 2., 7.] {
                line(
                    f,
                    Point::new(p.x - 5., p.y + y),
                    Point::new(p.x + 5., p.y + y),
                    color,
                    1.2,
                );
            }
        }
    }
}
impl canvas::Program<Message> for RoutingCanvas<'_> {
    type State = RoutingState;
    fn update(
        &self,
        state: &mut RoutingState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let (width, height, position) = self.geometry(bounds);
        match event {
            canvas::Event::Window(iced::window::Event::RedrawRequested(_)) if state.awake.get() => {
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                state.pointer = p;
                state.awake.set(true);
                for block in self.snapshot.blocks.iter().rev() {
                    let a = position(block);
                    let ports = [
                        Point::new(a.x, a.y + height / 2.),
                        Point::new(a.x + width, a.y + height / 2.),
                    ];
                    if let Some(port) = ports
                        .into_iter()
                        .find(|port| (p.x - port.x).abs() < 7. && (p.y - port.y).abs() < 7.)
                    {
                        state.loose = Some(port);
                        return Some(canvas::Action::request_redraw().and_capture());
                    }
                    if Rectangle::new(a, Size::new(width, height)).contains(p) {
                        state.drag = Some((block.id.clone(), p, a));
                        return Some(
                            canvas::Action::publish(Message::SelectBlock(block.id.clone()))
                                .and_capture(),
                        );
                    }
                }
                None
            }
            canvas::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                let p = cursor.position_in(bounds)?;
                state.pointer = p;
                if let Some((id, start, origin)) = &state.drag {
                    let destination = snap_to_cell(
                        Point::new(origin.x + p.x - start.x, origin.y + p.y - start.y),
                        bounds.size(),
                    );
                    // An occupied slot keeps the effect at its last valid location.
                    if self
                        .snapshot
                        .blocks
                        .iter()
                        .any(|block| &block.id != id && position(block).distance(destination) < 1.)
                    {
                        return Some(canvas::Action::request_redraw().and_capture());
                    }
                    return Some(
                        canvas::Action::publish(Message::MoveBlock(
                            id.clone(),
                            (destination.x, destination.y),
                        ))
                        .and_capture(),
                    );
                }
                if state.loose.is_some() {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                None
            }
            canvas::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                state.drag = None;
                if state.loose.take().is_some() {
                    return Some(
                        canvas::Action::publish(Message::UnsupportedRouting).and_capture(),
                    );
                }
                None
            }
            _ => None,
        }
    }
    fn draw(
        &self,
        state: &RoutingState,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        if let Ok(mut viewport) = self.viewport.lock() {
            *viewport = bounds.size();
        }
        let mut f = Frame::new(renderer, bounds.size());
        f.fill_rectangle(Point::ORIGIN, bounds.size(), iced::color!(0x202020));
        let (width, height, position) = self.geometry(bounds);
        let cell_w = (bounds.width - 24.).max(14.) / 14.;
        let cell_h = (bounds.height - 24.).max(14.) / 6.;
        for row in 0..6 {
            for col in 0..14 {
                let origin = Point::new(
                    12. + col as f32 * cell_w + (cell_w - width) / 2.,
                    12. + row as f32 * cell_h + (cell_h - height) / 2.,
                );
                let cell = Path::rounded_rectangle(origin, Size::new(width, height), 3.into());
                f.fill(&cell, iced::color!(0x0c0c0c));
                f.stroke(
                    &cell,
                    Stroke::default()
                        .with_color(iced::color!(0x333333))
                        .with_width(1.),
                );
            }
        }
        for (a, b) in &self.snapshot.edges {
            let (Some(source), Some(target)) = (
                self.snapshot.blocks.iter().find(|x| &x.id == a),
                self.snapshot.blocks.iter().find(|x| &x.id == b),
            ) else {
                continue;
            };
            let a = position(source);
            let b = position(target);
            let start = Point::new(a.x + width, a.y + height / 2.);
            let end = Point::new(b.x, b.y + height / 2.);
            let path = Path::new(|p| {
                p.move_to(start);
                if self.simple || (start.y - end.y).abs() < 1. {
                    p.line_to(end);
                } else {
                    let bend = ((end.x - start.x).abs() * 0.5).max(cell_w * 0.35);
                    p.bezier_curve_to(
                        Point::new(start.x + bend, start.y),
                        Point::new(end.x - bend, end.y),
                        end,
                    );
                }
            });
            f.stroke(
                &path,
                Stroke::default()
                    .with_color(iced::color!(0xbcbcbc))
                    .with_width(2.),
            );
        }
        let registry = EffectWidgetRegistry::default();
        let selected = self.selected.or_else(|| {
            self.snapshot
                .blocks
                .iter()
                .find(|b| !b.parameters.is_empty())
                .map(|b| b.id.as_str())
        });
        for b in &self.snapshot.blocks {
            let p = position(b);
            let material = registry.presentation(&b.family);
            let active = selected == Some(b.id.as_str());
            let rect = Path::rounded_rectangle(p, Size::new(width, height), 2.into());
            let shunt = matches!(b.family.as_str(), "wire" | "shunt");
            let bypassed = b.bypassed == Some(true);
            let accent = match b.family.as_str() {
                "input" => iced::color!(0x424f7c),
                "output" => iced::color!(0x286548),
                "comp" | "multicomp" | "gate" => iced::color!(0x878525),
                "drive" | "dynamic-distortion" => iced::color!(0x91332b),
                "amp" => iced::color!(0x454238),
                "cab" => iced::color!(0x475c5b),
                "peq" | "geq" => iced::color!(0xa96928),
                "chorus" => iced::color!(0x2683a1),
                "flanger" | "phaser" => iced::color!(0x753c67),
                "delay" | "multitap" => iced::color!(0x274d80),
                "reverb" => iced::color!(0x634e7e),
                _ => material.accent.scale_alpha(0.65),
            };
            if !shunt && !bypassed {
                f.fill(
                    &rect,
                    canvas::Gradient::Linear(
                        canvas::gradient::Linear::new(p, Point::new(p.x, p.y + height))
                            .add_stop(0., accent)
                            .add_stop(0.55, accent.scale_alpha(0.85))
                            .add_stop(1., iced::color!(0x181818)),
                    ),
                );
            }
            f.stroke(
                &rect,
                Stroke::default()
                    .with_color(if active {
                        iced::color!(0xffcc44)
                    } else if bypassed || shunt {
                        iced::color!(0x505050)
                    } else {
                        iced::color!(0xc4c4ba)
                    })
                    .with_width(if active { 2.5 } else { 1.5 }),
            );
            if shunt {
                line(
                    &mut f,
                    Point::new(p.x, p.y + height / 2.),
                    Point::new(p.x + width, p.y + height / 2.),
                    iced::color!(0xbcbcbc),
                    2.,
                );
                continue;
            }
            let ink = if bypassed {
                iced::color!(0x555555)
            } else {
                iced::color!(0xe5e5dc)
            };
            f.with_save(|f| {
                f.translate(Vector::new(p.x + width / 2., p.y + height * 0.28));
                f.scale((width / 48.).min(height / 48.).min(1.));
                icon(
                    f,
                    &b.family,
                    Point::ORIGIN,
                    if bypassed {
                        ink
                    } else {
                        Color::BLACK.scale_alpha(0.7)
                    },
                );
            });
            let label = match b.family.as_str() {
                "input" => b.label.replace("INPUT", "In").replace("Input", "In"),
                "output" => b.label.replace("OUTPUT", "Out").replace("Output", "Out"),
                "peq" => "PEQ".into(),
                "geq" => "GEQ".into(),
                "flanger" => "Flange".into(),
                _ => material.icon.to_string(),
            };
            f.fill_text(canvas::Text {
                content: label,
                position: Point::new(p.x + width / 2., p.y + height * 0.60),
                color: if !bypassed && matches!(b.family.as_str(), "input" | "output") {
                    iced::color!(0x71ff62)
                } else {
                    ink
                },
                size: (width / 4.2).clamp(7., 15.).into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                ..Default::default()
            });
        }
        if let Some(start) = state.loose {
            f.stroke(
                &Path::line(start, state.pointer),
                Stroke::default().with_color(style::BLUE).with_width(2.),
            );
        }
        state
            .awake
            .set(state.drag.is_some() || state.loose.is_some());
        vec![f.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axe_session::{Mock, Transport};

    #[test]
    fn snapping_centers_tiles_in_nearest_slot_and_clamps_edges() {
        for size in [Size::new(700., 300.), Size::new(1128., 730.)] {
            let snapshot = Mock::default().snapshot().unwrap();
            let positions = auto_layout(&snapshot, size);
            let cell_w = (size.width - 24.) / 14.;
            let cell_h = (size.height - 24.) / 6.;
            for (x, y) in positions.values() {
                let exact = Point::new(*x, *y);
                assert!(snap_to_cell(exact, size).distance(exact) < 0.001);
                let dragged = Point::new(x + cell_w * 0.4, y - cell_h * 0.4);
                assert!(snap_to_cell(dragged, size).distance(exact) < 0.001);
            }
            let first = snap_to_cell(Point::ORIGIN, size);
            assert_eq!(snap_to_cell(Point::new(-1000., -1000.), size), first);
            let last = snap_to_cell(Point::new(10000., 10000.), size);
            assert!((last.x - first.x - 13. * cell_w).abs() < 0.001);
            assert!((last.y - first.y - 5. * cell_h).abs() < 0.001);
        }
    }

    #[test]
    fn compact_grid_keeps_all_cells_inside_viewport_and_separate() {
        let mut snapshot = Mock::default().snapshot().unwrap();
        let template = snapshot.blocks[0].clone();
        snapshot.blocks.clear();
        for row in 0..6 {
            for col in 0..14 {
                let mut block = template.clone();
                block.id = format!("{row}-{col}");
                block.position = (col, row);
                snapshot.blocks.push(block);
            }
        }
        for size in [
            Size::new(700., 300.),
            Size::new(1128., 730.),
            Size::new(1800., 1000.),
        ] {
            let positions = BTreeMap::new();
            let canvas = RoutingCanvas {
                snapshot: &snapshot,
                simple: true,
                selected: None,
                positions: &positions,
                viewport: Default::default(),
            };
            let (width, height, position) = canvas.geometry(Rectangle::new(Point::ORIGIN, size));
            assert!(width <= 68. && height <= 50.);
            for block in &snapshot.blocks {
                let p = position(block);
                assert!(
                    p.x >= 0.
                        && p.y >= 0.
                        && p.x + width <= size.width
                        && p.y + height <= size.height
                );
                for other in &snapshot.blocks {
                    if block.id == other.id {
                        continue;
                    }
                    let q = position(other);
                    assert!(
                        p.x + width <= q.x
                            || q.x + width <= p.x
                            || p.y + height <= q.y
                            || q.y + height <= p.y
                    );
                }
            }
        }
    }
}
