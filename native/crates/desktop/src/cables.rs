//! Bounded visual cable model. It cannot emit device routing mutations.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
pub struct CableSimulation {
    pub points: Vec<Point>,
    settled: bool,
}
impl CableSimulation {
    pub fn new(a: Point, b: Point) -> Self {
        Self {
            points: (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.;
                    Point {
                        x: a.x + (b.x - a.x) * t,
                        y: a.y + (b.y - a.y) * t,
                    }
                })
                .collect(),
            settled: false,
        }
    }
    pub fn step(&mut self, reduced_motion: bool) -> bool {
        if self.settled {
            return false;
        }
        let a = self.points[0];
        let b = self.points[16];
        let mut movement = 0_f32;
        for (i, p) in self.points.iter_mut().enumerate().take(16).skip(1) {
            let t = i as f32 / 16.;
            let goal = a.y
                + (b.y - a.y) * t
                + if reduced_motion {
                    0.
                } else {
                    80. * t * (1. - t)
                };
            let delta = (goal - p.y) * 0.2;
            p.y += delta;
            movement = movement.max(delta.abs());
        }
        self.settled = movement < 0.01;
        !self.settled
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_fixed_and_settles() {
        let mut s = CableSimulation::new(Point { x: 0., y: 0. }, Point { x: 100., y: 0. });
        for _ in 0..100 {
            s.step(false);
        }
        assert_eq!(s.points[0].y, 0.);
        assert_eq!(s.points[16].x, 100.);
        assert!(!s.step(false));
    }
}
