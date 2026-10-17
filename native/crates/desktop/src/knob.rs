//! Read-only parameter indicator. Unknown values draw no active arc.
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Point, Rectangle, Renderer, Theme};
pub struct Knob {
    pub value: Option<f64>,
    pub range: Option<(f64, f64)>,
}
impl canvas::Program<crate::Message> for Knob {
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
        let center = Point::new(bounds.width / 2., bounds.height / 2.);
        let r = 12.;
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        let arc = |end| {
            Path::new(|p| {
                p.arc(canvas::path::Arc {
                    center,
                    radius: r,
                    start_angle: iced::Radians(start),
                    end_angle: iced::Radians(end),
                })
            })
        };
        f.stroke(
            &arc(start + sweep),
            Stroke::default()
                .with_color(iced::color!(0x344451))
                .with_width(4.),
        );
        f.fill(&Path::circle(center, 8.), iced::color!(0x24333e));
        if let (Some(v), Some((min, max))) = (self.value, self.range)
            && v.is_finite()
            && max > min
        {
            let angle = start + sweep * ((v - min) / (max - min)).clamp(0., 1.) as f32;
            f.stroke(
                &arc(angle),
                Stroke::default()
                    .with_color(crate::style::BLUE)
                    .with_width(4.),
            );
            f.stroke(
                &Path::line(
                    center,
                    Point::new(center.x + angle.cos() * 7., center.y + angle.sin() * 7.),
                ),
                Stroke::default()
                    .with_color(crate::style::TEXT)
                    .with_width(1.5),
            );
        }
        vec![f.into_geometry()]
    }
}
