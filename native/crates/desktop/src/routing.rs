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
        let automatic = auto_layout(self.snapshot, bounds.size());
        let columns = automatic
            .values()
            .map(|(x, _)| x.to_bits())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            .max(1);
        let width = ((bounds.width - 24. - (columns.saturating_sub(1) as f32 * 12.))
            / columns as f32)
            .clamp(44., 78.);
        let height = auto_card_height(self.snapshot, bounds.size());
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
/// Assign each effect a graph layer so connections run from left to right.
pub fn auto_layout(snapshot: &Snapshot, size: Size) -> BTreeMap<String, (f32, f32)> {
    let n = snapshot.blocks.len();
    if n == 0 {
        return BTreeMap::new();
    }
    let ids = snapshot
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.as_str(), i))
        .collect::<std::collections::HashMap<_, _>>();
    let mut adjacency = vec![Vec::new(); n];
    for (from, to) in &snapshot.edges {
        if let (Some(&a), Some(&b)) = (ids.get(from.as_str()), ids.get(to.as_str())) {
            adjacency[a].push(b);
        }
    }
    let mut tarjan = Tarjan::new(&adjacency);
    for v in 0..n {
        if tarjan.indices[v].is_none() {
            tarjan.visit(v);
        }
    }
    let components = tarjan.components;
    let mut component_of = vec![0; n];
    for (c, nodes) in components.iter().enumerate() {
        for &v in nodes {
            component_of[v] = c;
        }
    }
    let mut outgoing = vec![std::collections::BTreeSet::new(); components.len()];
    let mut indegree = vec![0usize; components.len()];
    for v in 0..n {
        for &to in &adjacency[v] {
            let (a, b) = (component_of[v], component_of[to]);
            if a != b && outgoing[a].insert(b) {
                indegree[b] += 1;
            }
        }
    }
    let mut queue = std::collections::VecDeque::new();
    for (c, &d) in indegree.iter().enumerate() {
        if d == 0 {
            queue.push_back(c);
        }
    }
    let mut component_rank = vec![0usize; components.len()];
    while let Some(c) = queue.pop_front() {
        for &to in &outgoing[c] {
            component_rank[to] = component_rank[to].max(component_rank[c] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                queue.push_back(to);
            }
        }
    }
    let mut max_rank = component_rank.iter().copied().max().unwrap_or(0);
    for (index, block) in snapshot.blocks.iter().enumerate() {
        if block.family == "output" {
            max_rank = max_rank.max(component_rank[component_of[index]]);
        }
    }
    for (index, block) in snapshot.blocks.iter().enumerate() {
        if block.family == "input" {
            component_rank[component_of[index]] = 0;
        }
        if block.family == "output" {
            component_rank[component_of[index]] = max_rank;
        }
    }
    let usable_w = (size.width - 24.).max(1.);
    let min_width = 46.;
    let max_columns = ((usable_w + 12.) / (min_width + 12.)).floor().max(1.) as usize;
    let columns = (max_rank + 1).min(max_columns).max(1);
    let display_rank = |rank: usize| {
        if max_rank + 1 <= columns {
            rank
        } else {
            rank * (columns - 1) / max_rank.max(1)
        }
    };
    let mut layers = vec![Vec::new(); columns];
    for (c, nodes) in components.iter().enumerate() {
        let layer = display_rank(component_rank[c]);
        layers[layer].extend(nodes.iter().copied());
    }
    for layer in &mut layers {
        layer.sort_by_key(|&i| {
            (
                snapshot.blocks[i].position.1,
                snapshot.blocks[i].position.0,
                snapshot.blocks[i].id.as_str(),
            )
        });
    }
    // Repeated barycentric sweeps keep connected blocks near their neighbors.
    for _ in 0..4 {
        for l in 1..layers.len() {
            let mut order = vec![0f32; n];
            for (i, layer) in layers.iter().enumerate() {
                for (p, &v) in layer.iter().enumerate() {
                    order[v] = p as f32;
                }
                if i == l {
                    break;
                }
            }
            layers[l].sort_by(|&a, &b| {
                let score = |v: usize| {
                    let parents = adjacency
                        .iter()
                        .enumerate()
                        .filter_map(|(p, outs)| outs.contains(&v).then_some(order[p]))
                        .collect::<Vec<_>>();
                    if parents.is_empty() {
                        snapshot.blocks[v].position.1 as f32
                    } else {
                        parents.iter().sum::<f32>() / parents.len() as f32
                    }
                };
                score(a)
                    .total_cmp(&score(b))
                    .then_with(|| snapshot.blocks[a].id.cmp(&snapshot.blocks[b].id))
            });
        }
        for l in (0..layers.len().saturating_sub(1)).rev() {
            let mut order = vec![0f32; n];
            for (i, layer) in layers.iter().enumerate().rev() {
                for (p, &v) in layer.iter().enumerate() {
                    order[v] = p as f32;
                }
                if i == l {
                    break;
                }
            }
            layers[l].sort_by(|&a, &b| {
                let score = |v: usize| {
                    let children = adjacency[v].iter().map(|&to| order[to]).collect::<Vec<_>>();
                    if children.is_empty() {
                        snapshot.blocks[v].position.1 as f32
                    } else {
                        children.iter().sum::<f32>() / children.len() as f32
                    }
                };
                score(a)
                    .total_cmp(&score(b))
                    .then_with(|| snapshot.blocks[a].id.cmp(&snapshot.blocks[b].id))
            });
        }
    }
    let max_rows = layers.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let usable_h = (size.height - 24.).max(1.);
    let height = (usable_h - 10. * (max_rows.saturating_sub(1) as f32)) / max_rows as f32;
    let height = height.clamp(42., 66.);
    let width = ((usable_w - 12. * (columns.saturating_sub(1) as f32)) / columns as f32)
        .clamp(min_width, 78.);
    let mut result = BTreeMap::new();
    for (col, layer) in layers.iter().enumerate() {
        let x = if columns == 1 {
            12. + (usable_w - width) / 2.
        } else {
            12. + col as f32 * (usable_w - width) / (columns - 1) as f32
        };
        let free = (usable_h - height).max(0.);
        for (row, &index) in layer.iter().enumerate() {
            let y = if layer.len() <= 1 {
                12. + free / 2.
            } else {
                12. + row as f32 * free / (layer.len() - 1) as f32
            };
            result.insert(snapshot.blocks[index].id.clone(), (x, y));
        }
    }
    result
}

fn auto_card_height(snapshot: &Snapshot, size: Size) -> f32 {
    let positions = auto_layout(snapshot, size);
    let mut counts = std::collections::BTreeMap::<u32, usize>::new();
    for (x, _) in positions.values() {
        *counts.entry(x.to_bits()).or_default() += 1;
    }
    let rows = counts.values().copied().max().unwrap_or(1).max(1);
    let usable = (size.height - 24.).max(1.);
    ((usable - 10. * (rows.saturating_sub(1) as f32)) / rows as f32).clamp(42., 66.)
}

struct Tarjan<'a> {
    adjacency: &'a [Vec<usize>],
    indices: Vec<Option<usize>>,
    low: Vec<usize>,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    components: Vec<Vec<usize>>,
    next: usize,
}
impl<'a> Tarjan<'a> {
    fn new(adjacency: &'a [Vec<usize>]) -> Self {
        let n = adjacency.len();
        Self {
            adjacency,
            indices: vec![None; n],
            low: vec![0; n],
            stack: Vec::new(),
            on_stack: vec![false; n],
            components: Vec::new(),
            next: 0,
        }
    }
    fn visit(&mut self, v: usize) {
        let index = self.next;
        self.next += 1;
        self.indices[v] = Some(index);
        self.low[v] = index;
        self.stack.push(v);
        self.on_stack[v] = true;
        for &to in &self.adjacency[v] {
            if self.indices[to].is_none() {
                self.visit(to);
                self.low[v] = self.low[v].min(self.low[to]);
            } else if self.on_stack[to] {
                self.low[v] = self.low[v].min(self.indices[to].unwrap());
            }
        }
        if self.low[v] == index {
            let mut component = Vec::new();
            loop {
                let node = self.stack.pop().unwrap();
                self.on_stack[node] = false;
                component.push(node);
                if node == v {
                    break;
                }
            }
            self.components.push(component);
        }
    }
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
                Point::new(p.x + width / 2., p.y + height * 0.31),
                material.accent,
            );
            f.fill_text(canvas::Text {
                content: if b.label.chars().count()
                    > ((width / 7.).floor() as usize).saturating_sub(2)
                {
                    format!(
                        "{}…",
                        b.label
                            .chars()
                            .take(((width / 7.).floor() as usize).saturating_sub(3).max(3))
                            .collect::<String>()
                    )
                } else {
                    b.label.clone()
                },
                position: Point::new(p.x + 6., p.y + height * 0.64),
                color: style::TEXT,
                size: 10.into(),
                ..Default::default()
            });
            f.fill_text(canvas::Text {
                content: b
                    .id
                    .strip_prefix("cell-")
                    .unwrap_or(&b.id)
                    .chars()
                    .take(8)
                    .collect(),
                position: Point::new(p.x + 6., p.y + height * 0.84),
                color: style::MUTED,
                size: 8.into(),
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
