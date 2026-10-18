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
        let width = 90_f32.min((bounds.width - 36.).max(54.) / 3.).max(62.);
        let height = 88.;
        let automatic = auto_layout(self.snapshot, bounds.size());
        (width, height, move |b: &Block| {
            let p = self
                .positions
                .get(&b.id)
                .copied()
                .or_else(|| automatic.get(&b.id).copied())
                .unwrap_or((8., 8.));
            Point::new(
                p.0.clamp(8., (bounds.width - width - 8.).max(8.)),
                p.1.clamp(8., (bounds.height - height - 8.).max(8.)),
            )
        })
    }
}
/// Arrange existing effects into non-overlapping viewport columns. Device grid
/// order is preserved while adjacent source columns are compacted to available width.
pub fn auto_layout(snapshot: &Snapshot, size: Size) -> BTreeMap<String, (f32, f32)> {
    let count = snapshot.blocks.len();
    if count == 0 {
        return BTreeMap::new();
    }
    let aspect = (size.width.max(1.) / size.height.max(1.)).clamp(0.5, 2.0);
    let width = 90.0_f32.min(((size.width - 36.).max(54.) / 3.).max(62.));
    let height = 88.0_f32;
    let capacity_rows = ((size.height - 24.).max(height) / (height + 12.))
        .floor()
        .max(1.) as usize;
    let required_columns = count.div_ceil(capacity_rows);
    let visual_columns = (count as f32 * aspect).sqrt().round() as usize;
    let columns = required_columns.max(visual_columns).clamp(1, count);
    let mut ordered = snapshot.blocks.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|b| (b.position.0, b.position.1, b.id.as_str()));
    let mut buckets = vec![Vec::new(); columns];
    for (index, block) in ordered.into_iter().enumerate() {
        let rank = index * columns / count;
        buckets[rank].push(block);
    }
    let usable_w = (size.width - width - 24.).max(0.);
    let usable_h = (size.height - height - 24.).max(0.);
    let used_columns = buckets
        .iter()
        .rposition(|b| !b.is_empty())
        .map_or(1, |i| i + 1);
    let mut result = BTreeMap::new();
    for (col, blocks) in buckets.iter_mut().enumerate() {
        blocks.sort_by_key(|b| (b.position.1, b.id.as_str()));
        if blocks.is_empty() {
            continue;
        }
        let x = if used_columns <= 1 {
            12. + usable_w / 2.
        } else {
            12. + col as f32 * usable_w / (used_columns - 1) as f32
        };
        for (row, block) in blocks.iter().enumerate() {
            let y = if blocks.len() <= 1 {
                12. + usable_h / 2.
            } else {
                12. + row as f32 * usable_h / (blocks.len() - 1) as f32
            };
            result.insert(block.id.clone(), (x, y));
        }
    }
    result
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
                Point::new(p.x - 13., p.y - 10.),
                Size::new(26., 20.),
                3.into(),
            );
            f.stroke(&body, stroke);
            f.stroke(
                &Path::rectangle(Point::new(p.x - 8., p.y - 3.), Size::new(16., 10.)),
                stroke,
            );
            line(
                f,
                Point::new(p.x - 10., p.y - 5.),
                Point::new(p.x + 10., p.y - 5.),
                color,
                1.,
            );
            for x in [-7., 0., 7.] {
                f.fill(&Path::circle(Point::new(p.x + x, p.y - 7.), 1.), color);
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
        "wire" | "shunt" => line(
            f,
            Point::new(p.x - 12., p.y),
            Point::new(p.x + 12., p.y),
            color,
            1.7,
        ),
        _ => f.fill_text(canvas::Text {
            content: "?".into(),
            position: Point::new(p.x - 5., p.y - 12.),
            color,
            size: 23.into(),
            ..Default::default()
        }),
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
                    if ((p.x - a.x).abs() < 7. || (p.x - a.x - width).abs() < 7.)
                        && (p.y - a.y - height / 2.).abs() < 7.
                    {
                        state.loose = Some(Point::new(
                            if (p.x - a.x).abs() < 7. {
                                a.x
                            } else {
                                a.x + width
                            },
                            a.y + height / 2.,
                        ));
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
        // Fine matrix provides a routing coordinate system without competing with cables.
        for x in (12..bounds.width.max(0.) as usize).step_by(16) {
            for y in (12..bounds.height.max(0.) as usize).step_by(16) {
                f.fill(
                    &Path::circle(Point::new(x as f32, y as f32), 0.55),
                    iced::color!(0x263039),
                );
            }
        }
        let (width, height, position) = self.geometry(bounds);
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
            let start = Point::new(a.x + width, a.y + height / 2.);
            let end = Point::new(b.x, b.y + height / 2.);
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
                    Size::new(4., height - 6.),
                    3.into(),
                ),
                material.accent,
            );
            icon(
                &mut f,
                &b.family,
                Point::new(p.x + width / 2., p.y + 29.),
                material.accent,
            );
            f.fill_text(canvas::Text {
                content: if b.label.chars().count() > 11 {
                    format!("{}…", b.label.chars().take(9).collect::<String>())
                } else {
                    b.label.clone()
                },
                position: Point::new(p.x + 12., p.y + 54.),
                color: style::TEXT,
                size: 12.into(),
                ..Default::default()
            });
            f.fill_text(canvas::Text {
                content: b.id.chars().take(14).collect(),
                position: Point::new(p.x + 12., p.y + 75.),
                color: style::MUTED,
                size: 10.into(),
                ..Default::default()
            });
            for x in [p.x, p.x + width] {
                let pt = Point::new(x, p.y + height / 2.);
                f.fill(&Path::circle(pt, 5.), iced::color!(0x0a1118));
                f.stroke(
                    &Path::circle(pt, 4.5),
                    Stroke::default()
                        .with_color(iced::color!(0x8dd0ff))
                        .with_width(1.3),
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
