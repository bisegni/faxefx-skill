use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Point, Rectangle, Renderer, Theme};
pub struct Waveform<'a> {
    pub peaks: &'a [f32],
    pub blue: bool,
}
impl canvas::Program<crate::Message> for Waveform<'_> {
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
        let color = if self.blue {
            crate::style::BLUE
        } else {
            crate::style::MUTED
        };
        let center = bounds.height / 2.;
        f.stroke(
            &Path::line(Point::new(0., center), Point::new(bounds.width, center)),
            Stroke::default().with_color(crate::style::EDGE),
        );
        for (i, peak) in self.peaks.iter().enumerate() {
            let x = (i as f32 + 0.5) * bounds.width / self.peaks.len().max(1) as f32;
            let h = peak.clamp(0., 1.) * (center - 2.).max(0.);
            f.stroke(
                &Path::line(
                    Point::new(x, center - h.max(0.5)),
                    Point::new(x, center + h.max(0.5)),
                ),
                Stroke::default()
                    .with_color(color)
                    .with_width((bounds.width / self.peaks.len().max(1) as f32 * 0.7).max(1.)),
            );
        }
        vec![f.into_geometry()]
    }
}
