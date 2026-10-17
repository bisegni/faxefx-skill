//! Native routing presentation; coordinates and cable styling never alter routing.
use crate::{Message, cables, effects::EffectWidgetRegistry, style};
use axe_domain::{Block, Snapshot};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
pub struct RoutingCanvas<'a> {
    pub snapshot: &'a Snapshot,
    pub simple: bool,
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
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
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
        let max_col = self
            .snapshot
            .blocks
            .iter()
            .map(|b| b.position.0)
            .max()
            .unwrap_or(4)
            .max(4) as f32;
        let max_row = self
            .snapshot
            .blocks
            .iter()
            .map(|b| b.position.1)
            .max()
            .unwrap_or(0) as f32;
        let width = 90_f32.min((bounds.width - 40.).max(90.) / 3.);
        let height = 96.;
        let position = |b: &Block| {
            Point::new(
                18. + b.position.0 as f32 / max_col * (bounds.width - width - 36.).max(0.),
                if max_row == 0. {
                    (bounds.height * 0.46 - height / 2.).max(18.)
                } else {
                    40. + b.position.1 as f32 / max_row * (bounds.height - height - 100.).max(0.)
                },
            )
        };
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
            let mut cable = cables::CableSimulation::new(
                cables::Point {
                    x: start.x,
                    y: start.y,
                },
                cables::Point { x: end.x, y: end.y },
            );
            for _ in 0..64 {
                if !cable.step(self.simple) {
                    break;
                }
            }
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
        let selected = self
            .snapshot
            .blocks
            .iter()
            .find(|b| !b.parameters.is_empty())
            .map(|b| b.id.as_str());
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
        vec![f.into_geometry()]
    }
}
