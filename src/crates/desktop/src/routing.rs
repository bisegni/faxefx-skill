//! Native routing presentation; coordinates and cable styling never alter routing.
use crate::{Message, cables, effects::EffectWidgetRegistry, style};
use axe_domain::{Block, Snapshot};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
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
    chains: RefCell<BTreeMap<(String, String), cables::CableSimulation>>,
    last: Cell<Option<std::time::Instant>>,
}
impl Default for RoutingState {
    fn default() -> Self {
        Self {
            drag: None,
            loose: None,
            pointer: Point::ORIGIN,
            awake: Cell::new(true),
            chains: Default::default(),
            last: Cell::new(None),
        }
    }
}
impl RoutingCanvas<'_> {
    fn geometry(&self, bounds: Rectangle) -> (f32, f32, impl Fn(&Block) -> Point + use<'_>) {
        let cell_w = (bounds.width - 24.).max(14.) / 14.;
        let cell_h = (bounds.height - 24.).max(14.) / 6.;
        let width = (cell_w - 20.).clamp(36., 64.);
        let height = (cell_h - 16.).clamp(38., 46.);
        let automatic = auto_layout(self.snapshot, bounds.size());
        (width, height, move |block: &Block| {
            let p = self
                .positions
                .get(&block.id)
                .copied()
                .or_else(|| automatic.get(&block.id).copied())
                .unwrap_or((8., 8.));
            Point::new(
                p.0.clamp(8., (bounds.width - width - 8.).max(8.)),
                p.1.clamp(8., (bounds.height - height - 8.).max(8.)),
            )
        })
    }
}
/// Keep every block in its Axe-Fx grid cell (14 columns by 6 rows).
/// The grid is scaled to the current pane; card size stays inside each cell.
pub fn auto_layout(snapshot: &Snapshot, size: Size) -> BTreeMap<String, (f32, f32)> {
    let cell_w = (size.width - 24.).max(14.) / 14.;
    let cell_h = (size.height - 24.).max(14.) / 6.;
    let width = (cell_w - 20.).clamp(36., 64.);
    let height = (cell_h - 16.).clamp(38., 46.);
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
                        Point::new(a.x + width / 2., a.y),
                        Point::new(a.x + width / 2., a.y + height),
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
                    return Some(
                        canvas::Action::publish(Message::MoveBlock(
                            id.clone(),
                            (origin.x + p.x - start.x, origin.y + p.y - start.y),
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
        f.fill_rectangle(Point::ORIGIN, bounds.size(), iced::color!(0x090f13));
        let (width, height, position) = self.geometry(bounds);
        let cell_w = (bounds.width - 24.).max(14.) / 14.;
        let cell_h = (bounds.height - 24.).max(14.) / 6.;
        for row in 0..6 {
            for col in 0..14 {
                let origin = Point::new(
                    12. + col as f32 * cell_w + 2.,
                    12. + row as f32 * cell_h + 2.,
                );
                let cell = Path::rounded_rectangle(
                    origin,
                    Size::new((cell_w - 4.).max(1.), (cell_h - 4.).max(1.)),
                    3.into(),
                );
                f.fill(&cell, iced::color!(0x0a1115));
                f.stroke(
                    &cell,
                    Stroke::default()
                        .with_color(iced::color!(0x151f26))
                        .with_width(0.5),
                );
            }
        }
        let now = std::time::Instant::now();
        let elapsed = state
            .last
            .replace(Some(now))
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(1. / 60.);
        let mut awake = false;
        let mut chains = state.chains.borrow_mut();
        for (a, b) in &self.snapshot.edges {
            let (Some(source), Some(target)) = (
                self.snapshot.blocks.iter().find(|x| &x.id == a),
                self.snapshot.blocks.iter().find(|x| &x.id == b),
            ) else {
                continue;
            };
            let a = position(source);
            let b = position(target);
            let dx = (b.x + width / 2.) - (a.x + width / 2.);
            let dy = (b.y + height / 2.) - (a.y + height / 2.);
            let (start, end) = if dx.abs() >= dy.abs() {
                if dx >= 0. {
                    (
                        Point::new(a.x + width, a.y + height / 2.),
                        Point::new(b.x, b.y + height / 2.),
                    )
                } else {
                    (
                        Point::new(a.x, a.y + height / 2.),
                        Point::new(b.x + width, b.y + height / 2.),
                    )
                }
            } else if dy >= 0. {
                (
                    Point::new(a.x + width / 2., a.y + height),
                    Point::new(b.x + width / 2., b.y),
                )
            } else {
                (
                    Point::new(a.x + width / 2., a.y),
                    Point::new(b.x + width / 2., b.y + height),
                )
            };
            let key = (source.id.clone(), target.id.clone());
            let cable = chains.entry(key).or_insert_with(|| {
                cables::CableSimulation::new(
                    cables::Point {
                        x: start.x,
                        y: start.y,
                    },
                    cables::Point { x: end.x, y: end.y },
                )
            });
            cable.endpoints(
                cables::Point {
                    x: start.x,
                    y: start.y,
                },
                cables::Point { x: end.x, y: end.y },
            );
            awake |= cable.advance(elapsed, self.simple);
            let path = Path::new(|p| {
                p.move_to(start);
                for pt in &cable.points[1..] {
                    p.line_to(Point::new(pt.x, pt.y));
                }
            });
            f.stroke(
                &path,
                Stroke::default()
                    .with_color(Color::BLACK.scale_alpha(0.8))
                    .with_width(7.),
            );
            f.stroke(
                &path,
                Stroke::default()
                    .with_color(iced::color!(0x607487))
                    .with_width(3.),
            );
            f.stroke(
                &path,
                Stroke::default()
                    .with_color(iced::color!(0xa6c5df))
                    .with_width(1.2),
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
            let rect = Path::rounded_rectangle(p, Size::new(width, height), 6.into());
            f.fill(
                &Path::rounded_rectangle(
                    p + Vector::new(0., 4.),
                    Size::new(width, height),
                    6.into(),
                ),
                Color::BLACK.scale_alpha(0.65),
            );
            if active {
                f.stroke(
                    &rect,
                    Stroke::default()
                        .with_color(style::BLUE.scale_alpha(0.16))
                        .with_width(8.),
                );
            }
            f.fill(
                &rect,
                iced::widget::canvas::Gradient::Linear(
                    iced::widget::canvas::gradient::Linear::new(p, Point::new(p.x, p.y + height))
                        .add_stop(0., iced::color!(0x1e2b36))
                        .add_stop(1., iced::color!(0x101820)),
                ),
            );
            f.stroke(
                &rect,
                Stroke::default()
                    .with_color(if active {
                        style::BLUE
                    } else {
                        iced::color!(0x526674)
                    })
                    .with_width(if active { 2. } else { 1. }),
            );
            f.fill(
                &Path::rounded_rectangle(
                    Point::new(p.x + 1., p.y + 3.),
                    Size::new(2., height - 6.),
                    3.into(),
                ),
                material.accent,
            );
            icon(
                &mut f,
                &b.family,
                Point::new(p.x + width / 2., p.y + 14.),
                material.accent,
            );
            let label = b.label.clone();
            let chars = ((width - 8.) / 5.5).floor().max(3.) as usize;
            f.fill_text(canvas::Text {
                content: if label.chars().count() > chars {
                    format!(
                        "{}…",
                        label
                            .chars()
                            .take(chars.saturating_sub(1))
                            .collect::<String>()
                    )
                } else {
                    label
                },
                position: Point::new(p.x + width / 2., p.y + 29.),
                color: style::TEXT,
                size: 9.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                ..Default::default()
            });
            for pt in [
                Point::new(p.x, p.y + height / 2.),
                Point::new(p.x + width, p.y + height / 2.),
                Point::new(p.x + width / 2., p.y),
                Point::new(p.x + width / 2., p.y + height),
            ] {
                f.fill(&Path::circle(pt, 2.5), iced::color!(0x0a1118));
                f.stroke(
                    &Path::circle(pt, 2.),
                    Stroke::default()
                        .with_color(iced::color!(0x8dd0ff))
                        .with_width(1.),
                );
            }
        }
        if let Some(start) = state.loose {
            f.stroke(
                &Path::line(start, state.pointer),
                Stroke::default().with_color(style::BLUE).with_width(2.),
            );
        }
        state
            .awake
            .set(awake || state.drag.is_some() || state.loose.is_some());
        vec![f.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axe_session::{Mock, Transport};

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
            assert!(width <= 64. && height <= 46.);
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
