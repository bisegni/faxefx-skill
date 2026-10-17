use crate::Pane;
use iced::widget::pane_grid::{self, Axis, Configuration, Node};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Layout {
    Pane {
        pane: Pane,
    },
    Split {
        vertical: bool,
        ratio: f32,
        a: Box<Layout>,
        b: Box<Layout>,
    },
}
impl Layout {
    pub fn capture(state: &pane_grid::State<Pane>) -> Self {
        fn walk(n: &Node, s: &pane_grid::State<Pane>) -> Layout {
            match n {
                Node::Pane(p) => Layout::Pane {
                    pane: *s.get(*p).expect("pane grid invariant"),
                },
                Node::Split {
                    axis, ratio, a, b, ..
                } => Layout::Split {
                    vertical: *axis == Axis::Vertical,
                    ratio: *ratio,
                    a: Box::new(walk(a, s)),
                    b: Box::new(walk(b, s)),
                },
            }
        }
        walk(state.layout(), state)
    }
    pub fn valid(&self, width: f32, height: f32) -> bool {
        fn walk(l: &Layout, w: f32, h: f32, ids: &mut BTreeSet<Pane>, depth: usize) -> bool {
            if depth > 8 || !w.is_finite() || !h.is_finite() {
                return false;
            }
            match l {
                Layout::Pane { pane } => {
                    let (mw, mh) = match pane {
                        Pane::Presets => (240., 160.),
                        Pane::Inspector => (240., 220.),
                        Pane::Routing => (440., 300.),
                        Pane::Chat => (300., 400.),
                        Pane::Audio => (400., 120.),
                    };
                    ids.insert(*pane) && w >= mw && h >= mh
                }
                Layout::Split {
                    vertical,
                    ratio,
                    a,
                    b,
                } => {
                    if !ratio.is_finite() || *ratio <= 0. || *ratio >= 1. {
                        return false;
                    }
                    let (aw, ah, bw, bh) = if *vertical {
                        ((w - 6.) * ratio, h, (w - 6.) * (1. - ratio), h)
                    } else {
                        (w, (h - 6.) * ratio, w, (h - 6.) * (1. - ratio))
                    };
                    walk(a, aw, ah, ids, depth + 1) && walk(b, bw, bh, ids, depth + 1)
                }
            }
        }
        let mut ids = BTreeSet::new();
        walk(self, width, height, &mut ids, 0) && ids.len() == 5
    }
    pub fn state(&self) -> pane_grid::State<Pane> {
        fn config(l: &Layout) -> Configuration<Pane> {
            match l {
                Layout::Pane { pane } => Configuration::Pane(*pane),
                Layout::Split {
                    vertical,
                    ratio,
                    a,
                    b,
                } => Configuration::Split {
                    axis: if *vertical {
                        Axis::Vertical
                    } else {
                        Axis::Horizontal
                    },
                    ratio: *ratio,
                    a: Box::new(config(a)),
                    b: Box::new(config(b)),
                },
            }
        }
        pane_grid::State::with_configuration(config(self))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_fits_minimum() {
        let l = Layout::capture(&crate::layout());
        assert!(l.valid(1248., 730.));
        assert!(!l.valid(800., 600.));
    }
    #[test]
    fn duplicate_and_nan_rejected() {
        let p = Layout::Pane { pane: Pane::Audio };
        let l = Layout::Split {
            vertical: true,
            ratio: 0.5,
            a: Box::new(p.clone()),
            b: Box::new(p),
        };
        assert!(!l.valid(2000., 2000.));
    }
    #[test]
    fn roundtrip_preserves_all_panes() {
        let l = Layout::capture(&crate::layout());
        let json = serde_json::to_string(&l).unwrap();
        let decoded: Layout = serde_json::from_str(&json).unwrap();
        assert!(Layout::capture(&decoded.state()).valid(1248., 730.));
    }
}
