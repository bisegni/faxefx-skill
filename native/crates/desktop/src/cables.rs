//! Bounded fixed-step Verlet chain. Visual simulation cannot mutate routing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
pub struct CableSimulation {
    pub points: Vec<Point>,
    previous: Vec<Point>,
    a: Point,
    b: Point,
    settled: bool,
    quiet: u16,
    steps: u16,
    accumulator: f32,
    reduced: bool,
}
impl CableSimulation {
    pub fn new(a: Point, b: Point) -> Self {
        let points = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.;
                Point {
                    x: a.x + (b.x - a.x) * t,
                    y: a.y + (b.y - a.y) * t,
                }
            })
            .collect::<Vec<_>>();
        Self {
            previous: points.clone(),
            points,
            a,
            b,
            settled: false,
            quiet: 0,
            steps: 0,
            accumulator: 0.,
            reduced: false,
        }
    }
    pub fn endpoints(&mut self, a: Point, b: Point) {
        if self.a == a && self.b == b {
            return;
        }
        for (i, p) in self.points.iter_mut().enumerate() {
            let t = i as f32 / 16.;
            p.x += (a.x - self.a.x) * (1. - t) + (b.x - self.b.x) * t;
            p.y += (a.y - self.a.y) * (1. - t) + (b.y - self.b.y) * t;
        }
        self.previous = self.points.clone();
        self.a = a;
        self.b = b;
        self.settled = false;
        self.steps = 0;
        self.quiet = 0;
    }
    pub fn step(&mut self, reduced_motion: bool) -> bool {
        if reduced_motion {
            for (i, p) in self.points.iter_mut().enumerate() {
                let t = i as f32 / 16.;
                p.x = self.a.x + (self.b.x - self.a.x) * t;
                p.y = self.a.y + (self.b.y - self.a.y) * t;
            }
            self.settled = true;
            return false;
        }
        if self.settled {
            return false;
        }
        self.steps += 1;
        let old = self.points.clone();
        for i in 1..16 {
            let p = self.points[i];
            let prior = self.previous[i];
            self.previous[i] = p;
            self.points[i] = Point {
                x: p.x + (p.x - prior.x) * 0.9,
                y: p.y + (p.y - prior.y) * 0.9 + 0.15,
            };
        }
        let distance = ((self.b.x - self.a.x).powi(2) + (self.b.y - self.a.y).powi(2)).sqrt();
        let segment = (distance * 1.03 + 12.) / 16.;
        for _ in 0..8 {
            self.points[0] = self.a;
            self.points[16] = self.b;
            for i in 0..16 {
                let (a, b) = (self.points[i], self.points[i + 1]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let length = (dx * dx + dy * dy).sqrt().max(0.001);
                let correction = (length - segment) / length;
                let factor = if i == 0 || i == 15 { 1. } else { 0.5 };
                if i > 0 {
                    self.points[i].x += dx * correction * factor;
                    self.points[i].y += dy * correction * factor;
                }
                if i < 15 {
                    self.points[i + 1].x -= dx * correction * factor;
                    self.points[i + 1].y -= dy * correction * factor;
                }
            }
        }
        self.points[0] = self.a;
        self.points[16] = self.b;
        let ceiling = self.a.y.max(self.b.y) + 60.;
        for p in &mut self.points[1..16] {
            p.y = p.y.min(ceiling);
        }
        let movement = self
            .points
            .iter()
            .zip(old)
            .map(|(a, b)| (a.x - b.x).abs() + (a.y - b.y).abs())
            .fold(0., f32::max);
        if movement < 0.03 {
            self.quiet += 1
        } else {
            self.quiet = 0
        }
        self.settled = self.quiet > 20 || self.steps >= 480;
        !self.settled
    }
    pub fn advance(&mut self, elapsed: f32, reduced_motion: bool) -> bool {
        if reduced_motion != self.reduced {
            self.reduced = reduced_motion;
            self.settled = false;
            self.steps = 0;
            self.quiet = 0;
            self.previous = self.points.clone();
        }
        if reduced_motion {
            return self.step(true);
        }
        self.accumulator = (self.accumulator + elapsed.clamp(0., 0.1)).min(0.1);
        let mut steps = 0;
        while self.accumulator >= 1. / 60. && steps < 6 {
            self.step(false);
            self.accumulator -= 1. / 60.;
            steps += 1;
        }
        !self.settled
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_fixed_and_settles() {
        let mut s = CableSimulation::new(Point { x: 0., y: 0. }, Point { x: 100., y: 0. });
        for _ in 0..600 {
            s.step(false);
        }
        assert_eq!(s.points[0].y, 0.);
        assert_eq!(s.points[16].x, 100.);
        assert!(!s.step(false));
    }
    #[test]
    fn suspended_time_is_bounded_and_motion_can_be_disabled() {
        let mut s = CableSimulation::new(Point { x: 0., y: 0. }, Point { x: 200., y: 0. });
        s.advance(3600., false);
        assert!(s.steps <= 6);
        s.step(true);
        assert!(s.points.iter().all(|p| p.y == 0.));
        s.endpoints(Point { x: 0., y: 10. }, Point { x: 200., y: 10. });
        assert!(s.step(false));
    }
}
