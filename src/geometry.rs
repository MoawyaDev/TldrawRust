use crate::document::ShapeKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl std::ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn from_points(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn from_center_size(center: Point, width: f64, height: f64) -> Self {
        let half = Point::new(width.abs() / 2.0, height.abs() / 2.0);
        Self {
            min: center - half,
            max: center + half,
        }
    }

    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn center(self) -> Point {
        Point::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    pub fn expand(self, amount: f64) -> Self {
        let delta = Point::new(amount, amount);
        Self {
            min: self.min - delta,
            max: self.max + delta,
        }
    }

    pub fn is_finite(self) -> bool {
        self.min.x.is_finite()
            && self.min.y.is_finite()
            && self.max.x.is_finite()
            && self.max.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub center: Point,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Point::ZERO,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub const MIN_ZOOM: f64 = 0.08;
    pub const MAX_ZOOM: f64 = 8.0;

    pub fn world_to_screen(self, world: Point, viewport: Rect) -> Point {
        let center = viewport.center();
        Point::new(
            center.x + (world.x - self.center.x) * self.zoom,
            center.y + (world.y - self.center.y) * self.zoom,
        )
    }

    pub fn screen_to_world(self, screen: Point, viewport: Rect) -> Point {
        let center = viewport.center();
        Point::new(
            self.center.x + (screen.x - center.x) / self.zoom,
            self.center.y + (screen.y - center.y) / self.zoom,
        )
    }

    pub fn zoom_at(&mut self, screen: Point, viewport: Rect, factor: f64) {
        let anchor = self.screen_to_world(screen, viewport);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let center = viewport.center();
        self.center = Point::new(
            anchor.x - (screen.x - center.x) / self.zoom,
            anchor.y - (screen.y - center.y) / self.zoom,
        );
    }

    pub fn pan_screen(&mut self, delta: Point) {
        self.center = self.center - delta * (1.0 / self.zoom);
    }
}

pub fn distance_to_segment(point: Point, start: Point, end: Point) -> f64 {
    let segment = end - start;
    let length_squared = segment.x * segment.x + segment.y * segment.y;
    if length_squared <= f64::EPSILON {
        return point.distance(start);
    }
    let relative = point - start;
    let t = ((relative.x * segment.x + relative.y * segment.y) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

pub fn shape_vertices(kind: ShapeKind, bounds: Rect) -> Vec<Point> {
    match kind {
        ShapeKind::Rectangle | ShapeKind::CheckBox => vec![
            bounds.min,
            Point::new(bounds.max.x, bounds.min.y),
            bounds.max,
            Point::new(bounds.min.x, bounds.max.y),
        ],
        ShapeKind::Ellipse => (0..64)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / 64.0;
                ellipse_point(bounds, angle, 1.0)
            })
            .collect(),
        ShapeKind::Triangle => regular_polygon(bounds, 3, -std::f64::consts::FRAC_PI_2),
        ShapeKind::Diamond => vec![
            Point::new(bounds.center().x, bounds.min.y),
            Point::new(bounds.max.x, bounds.center().y),
            Point::new(bounds.center().x, bounds.max.y),
            Point::new(bounds.min.x, bounds.center().y),
        ],
        ShapeKind::Pentagon => regular_polygon(bounds, 5, -std::f64::consts::FRAC_PI_2),
        ShapeKind::Hexagon => regular_polygon(bounds, 6, 0.0),
        ShapeKind::Octagon => regular_polygon(bounds, 8, std::f64::consts::FRAC_PI_8),
        ShapeKind::Star => (0..10)
            .map(|index| {
                let radius = if index % 2 == 0 { 1.0 } else { 0.42 };
                let angle =
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * index as f64 / 10.0;
                ellipse_point(bounds, angle, radius)
            })
            .collect(),
        ShapeKind::Cloud => {
            let center = bounds.center();
            (0..20)
                .map(|index| {
                    let angle =
                        std::f64::consts::TAU * index as f64 / 20.0 - std::f64::consts::FRAC_PI_2;
                    let lobe = if index % 2 == 0 { 1.0 } else { 0.78 };
                    Point::new(
                        center.x + bounds.width() / 2.0 * angle.cos() * lobe,
                        center.y + bounds.height() / 2.0 * angle.sin() * lobe,
                    )
                })
                .collect()
        }
        ShapeKind::Trapezoid => vec![
            Point::new(bounds.min.x + bounds.width() * 0.2, bounds.min.y),
            Point::new(bounds.max.x - bounds.width() * 0.2, bounds.min.y),
            Point::new(bounds.max.x, bounds.max.y),
            Point::new(bounds.min.x, bounds.max.y),
        ],
        ShapeKind::ArrowUp
        | ShapeKind::ArrowDown
        | ShapeKind::ArrowLeft
        | ShapeKind::ArrowRight => {
            let mut points = vec![
                Point::new(0.3, 0.0),
                Point::new(0.7, 0.0),
                Point::new(0.7, 0.55),
                Point::new(1.0, 0.55),
                Point::new(0.5, 1.0),
                Point::new(0.0, 0.55),
                Point::new(0.3, 0.55),
            ];
            if matches!(kind, ShapeKind::ArrowDown | ShapeKind::ArrowLeft) {
                points.reverse();
            }
            points
                .into_iter()
                .map(|point| match kind {
                    ShapeKind::ArrowUp | ShapeKind::ArrowDown => Point::new(
                        bounds.min.x + point.x * bounds.width(),
                        bounds.min.y + point.y * bounds.height(),
                    ),
                    ShapeKind::ArrowRight => Point::new(
                        bounds.min.x + point.y * bounds.width(),
                        bounds.min.y + (1.0 - point.x) * bounds.height(),
                    ),
                    _ => Point::new(
                        bounds.min.x + (1.0 - point.y) * bounds.width(),
                        bounds.min.y + point.x * bounds.height(),
                    ),
                })
                .collect()
        }
        ShapeKind::XBox => {
            let x = bounds.width() * 0.25;
            let y = bounds.height() * 0.25;
            vec![
                Point::new(bounds.min.x + x, bounds.min.y),
                Point::new(bounds.center().x, bounds.min.y + y),
                Point::new(bounds.max.x - x, bounds.min.y),
                Point::new(bounds.max.x, bounds.min.y + y),
                Point::new(bounds.center().x + x, bounds.center().y),
                Point::new(bounds.max.x, bounds.max.y - y),
                Point::new(bounds.max.x - x, bounds.max.y),
                Point::new(bounds.center().x, bounds.max.y - y),
                Point::new(bounds.min.x + x, bounds.max.y),
                Point::new(bounds.min.x, bounds.max.y - y),
                Point::new(bounds.center().x - x, bounds.center().y),
                Point::new(bounds.min.x, bounds.min.y + y),
            ]
        }
    }
}

fn regular_polygon(bounds: Rect, sides: usize, start_angle: f64) -> Vec<Point> {
    (0..sides)
        .map(|index| {
            let angle = start_angle + std::f64::consts::TAU * index as f64 / sides as f64;
            ellipse_point(bounds, angle, 1.0)
        })
        .collect()
}

fn ellipse_point(bounds: Rect, angle: f64, radius: f64) -> Point {
    Point::new(
        bounds.center().x + bounds.width() / 2.0 * radius * angle.cos(),
        bounds.center().y + bounds.height() / 2.0 * radius * angle.sin(),
    )
}

pub fn point_in_polygon(point: Point, vertices: &[Point]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..vertices.len() {
        let a = vertices[index];
        let b = vertices[(index + 1) % vertices.len()];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

pub fn triangulate_polygon(vertices: &[Point]) -> Vec<[Point; 3]> {
    if vertices.len() < 3 {
        return Vec::new();
    }
    let area: f64 = vertices
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = vertices[(index + 1) % vertices.len()];
            point.x * next.y - next.x * point.y
        })
        .sum();
    let mut indices: Vec<usize> = (0..vertices.len()).collect();
    if area < 0.0 {
        indices.reverse();
    }
    let mut triangles = Vec::with_capacity(vertices.len() - 2);
    let mut attempts = 0;
    while indices.len() > 3 && attempts < vertices.len() * vertices.len() {
        let mut clipped = false;
        for index in 0..indices.len() {
            let previous = indices[(index + indices.len() - 1) % indices.len()];
            let current = indices[index];
            let next = indices[(index + 1) % indices.len()];
            let triangle = [vertices[previous], vertices[current], vertices[next]];
            if cross(triangle[0], triangle[1], triangle[2]) <= 0.0
                || indices.iter().copied().any(|candidate| {
                    candidate != previous
                        && candidate != current
                        && candidate != next
                        && point_in_triangle(vertices[candidate], triangle)
                })
            {
                continue;
            }
            triangles.push(triangle);
            indices.remove(index);
            clipped = true;
            break;
        }
        if !clipped {
            return Vec::new();
        }
        attempts += 1;
    }
    if indices.len() == 3 {
        triangles.push([
            vertices[indices[0]],
            vertices[indices[1]],
            vertices[indices[2]],
        ]);
    }
    triangles
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn point_in_triangle(point: Point, triangle: [Point; 3]) -> bool {
    let [a, b, c] = triangle;
    let ab = cross(a, b, point);
    let bc = cross(b, c, point);
    let ca = cross(c, a, point);
    ab >= 0.0 && bc >= 0.0 && ca >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_round_trip_preserves_world_coordinates() {
        let viewport = Rect::from_points(Point::new(20.0, 40.0), Point::new(820.0, 640.0));
        let camera = Camera {
            center: Point::new(120.0, -30.0),
            zoom: 1.7,
        };
        let world = Point::new(-80.0, 240.0);
        let restored = camera.screen_to_world(camera.world_to_screen(world, viewport), viewport);
        assert!((restored.x - world.x).abs() < 1e-9);
        assert!((restored.y - world.y).abs() < 1e-9);
    }

    #[test]
    fn zoom_keeps_the_world_point_under_the_pointer_fixed() {
        let viewport = Rect::from_points(Point::new(0.0, 0.0), Point::new(1000.0, 800.0));
        let pointer = Point::new(230.0, 310.0);
        let mut camera = Camera::default();
        let world = camera.screen_to_world(pointer, viewport);
        camera.zoom_at(pointer, viewport, 1.5);
        let after = camera.screen_to_world(pointer, viewport);
        assert!(world.distance(after) < 1e-9);
    }

    #[test]
    fn segment_distance_clamps_to_segment_endpoints() {
        assert_eq!(
            distance_to_segment(Point::new(4.0, 0.0), Point::ZERO, Point::new(2.0, 0.0)),
            2.0
        );
    }
}
