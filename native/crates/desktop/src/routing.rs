//! Native routing view. Presentation coordinates never mutate domain routing.
use crate::{Message, cables, effects::EffectWidgetRegistry};
use axe_domain::Snapshot;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme};
pub struct RoutingCanvas<'a> {
    pub snapshot: &'a Snapshot,
    pub simple: bool,
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
        let mut frame = Frame::new(renderer, bounds.size());
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
            .unwrap_or(1)
            .max(1) as f32;
        let card_width = 110_f32.min((bounds.width - 32.) / 3.);
        let card_height = 62.;
        let position = |b: &axe_domain::Block| {
            Point::new(
                14. + b.position.0 as f32 / max_col * (bounds.width - card_width - 28.).max(0.),
                35. + b.position.1 as f32 / max_row * (bounds.height - card_height - 100.).max(0.),
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
            let start = Point::new(a.x + card_width, a.y + card_height / 2.);
            let end = Point::new(b.x, b.y + card_height / 2.);
            let mut simulation = cables::CableSimulation::new(
                cables::Point {
                    x: start.x,
                    y: start.y,
                },
                cables::Point { x: end.x, y: end.y },
            );
            for _ in 0..64 {
                if !simulation.step(self.simple) {
                    break;
                }
            }
            let path = Path::new(|p| {
                p.move_to(start);
                for point in &simulation.points[1..] {
                    p.line_to(Point::new(point.x, point.y));
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(Color::from_rgb8(32, 42, 54))
                    .with_width(7.),
            );
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(Color::from_rgb8(99, 150, 208))
                    .with_width(2.),
            );
        }
        let registry = EffectWidgetRegistry::default();
        for block in &self.snapshot.blocks {
            let p = position(block);
            let path = Path::rectangle(p, iced::Size::new(card_width, card_height));
            frame.fill(&path, Color::from_rgb8(30, 33, 40));
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(Color::from_rgb8(93, 122, 151))
                    .with_width(1.),
            );
            let style = registry.presentation(&block.family);
            frame.fill_text(canvas::Text {
                content: style.icon.into(),
                position: Point::new(p.x + 10., p.y + 8.),
                color: Color::from_rgb8(135, 181, 225),
                size: 13.into(),
                ..Default::default()
            });
            frame.fill_text(canvas::Text {
                content: block.label.clone(),
                position: Point::new(p.x + 10., p.y + 32.),
                color: Color::WHITE,
                size: 12.into(),
                ..Default::default()
            });
            for x in [p.x, p.x + card_width] {
                frame.fill(
                    &Path::circle(Point::new(x, p.y + card_height / 2.), 4.),
                    Color::from_rgb8(164, 188, 214),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}
