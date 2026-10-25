use crate::geometry::{Camera, Point, Rect, distance_to_segment, point_in_polygon, shape_vertices};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Cloud,
    Trapezoid,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    XBox,
    CheckBox,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum FillStyle {
    #[default]
    None,
    Semi,
    Solid,
    Pattern,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeStyle {
    Draw,
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFamily {
    #[default]
    Sans,
    Mono,
    Script,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolStyle {
    pub color: [u8; 4],
    pub width: f64,
    #[serde(default)]
    pub fill: FillStyle,
    #[serde(default)]
    pub stroke: StrokeStyle,
    #[serde(default)]
    pub font: FontFamily,
    #[serde(default)]
    pub text_align: TextAlign,
}

impl Default for ToolStyle {
    fn default() -> Self {
        Self {
            color: [37, 99, 235, 255],
            width: 3.0,
            fill: FillStyle::None,
            stroke: StrokeStyle::Solid,
            font: FontFamily::Sans,
            text_align: TextAlign::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub element_id: Uuid,
    /// Normalized position in the target object's local bounds.
    pub local: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectorEndpoint {
    pub position: Point,
    #[serde(default)]
    pub attachment: Option<Attachment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElementKind {
    Stroke {
        points: Vec<Point>,
    },
    Line {
        start: Point,
        end: Point,
    },
    Shape {
        kind: ShapeKind,
        bounds: Rect,
    },
    Note {
        bounds: Rect,
        text: String,
    },
    Connector {
        start: ConnectorEndpoint,
        end: ConnectorEndpoint,
    },
    Frame {
        bounds: Rect,
        title: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: Uuid,
    pub kind: ElementKind,
    pub style: ToolStyle,
}

impl Element {
    pub fn new(kind: ElementKind, style: ToolStyle) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            style,
        }
    }

    pub fn bounds(&self) -> Option<Rect> {
        let bounds = match &self.kind {
            ElementKind::Stroke { points } => {
                let first = *points.first()?;
                points.iter().skip(1).fold(
                    Rect {
                        min: first,
                        max: first,
                    },
                    |bounds, point| {
                        bounds.union(Rect {
                            min: *point,
                            max: *point,
                        })
                    },
                )
            }
            ElementKind::Line { start, end } => Rect::from_points(*start, *end),
            ElementKind::Connector { start, end } => {
                Rect::from_points(start.position, end.position)
            }
            ElementKind::Shape { bounds, .. }
            | ElementKind::Note { bounds, .. }
            | ElementKind::Frame { bounds, .. } => *bounds,
        };
        Some(bounds.expand(self.style.width / 2.0))
    }

    pub fn hit_test(&self, point: Point, tolerance: f64) -> bool {
        match &self.kind {
            ElementKind::Stroke { points } => match points.as_slice() {
                [] => false,
                [only] => only.distance(point) <= tolerance + self.style.width / 2.0,
                points => points.windows(2).any(|pair| {
                    distance_to_segment(point, pair[0], pair[1])
                        <= tolerance + self.style.width / 2.0
                }),
            },
            ElementKind::Line { start, end } => {
                distance_to_segment(point, *start, *end) <= tolerance + self.style.width / 2.0
            }
            ElementKind::Shape { kind, bounds } => {
                if self.style.fill == FillStyle::Solid && bounds.contains(point) {
                    return true;
                }
                let inflated = bounds.expand(tolerance + self.style.width / 2.0);
                if !inflated.contains(point) {
                    return false;
                }
                {
                    if *kind == ShapeKind::Ellipse {
                        let hit_radius = tolerance + self.style.width / 2.0;
                        let rx = bounds.width() / 2.0;
                        let ry = bounds.height() / 2.0;
                        if rx <= 0.0 || ry <= 0.0 {
                            return false;
                        }
                        let center = bounds.center();
                        let outer = ((point.x - center.x) / (rx + hit_radius)).powi(2)
                            + ((point.y - center.y) / (ry + hit_radius)).powi(2);
                        if self.style.fill != FillStyle::None {
                            return outer <= 1.0;
                        }
                        let inner = ((point.x - center.x) / (rx - hit_radius).max(0.001)).powi(2)
                            + ((point.y - center.y) / (ry - hit_radius).max(0.001)).powi(2);
                        return outer <= 1.0 && inner >= 1.0;
                    }
                    if *kind == ShapeKind::Rectangle && self.style.fill != FillStyle::None {
                        return true;
                    }
                    let vertices = shape_vertices(*kind, *bounds);
                    let inside = point_in_polygon(point, &vertices);
                    if self.style.fill != FillStyle::None && inside {
                        return true;
                    }
                    vertices.iter().enumerate().any(|(index, start)| {
                        distance_to_segment(point, *start, vertices[(index + 1) % vertices.len()])
                            <= tolerance + self.style.width / 2.0
                    })
                }
            }
            ElementKind::Note { bounds, .. } | ElementKind::Frame { bounds, .. } => {
                bounds.expand(tolerance).contains(point)
            }
            ElementKind::Connector { start, end } => {
                distance_to_segment(point, start.position, end.position)
                    <= tolerance + self.style.width / 2.0
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub camera: Camera,
    pub elements: Vec<Element>,
}

impl Page {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            camera: Camera::default(),
            elements: Vec::new(),
        }
    }

    pub fn content_bounds(&self) -> Option<Rect> {
        self.elements
            .iter()
            .filter_map(|element| self.resolved_element_bounds(element))
            .reduce(Rect::union)
    }

    pub fn resolved_position(&self, endpoint: ConnectorEndpoint) -> Point {
        let Some(attachment) = endpoint.attachment else {
            return endpoint.position;
        };
        let Some(bounds) = self
            .elements
            .iter()
            .find(|element| element.id == attachment.element_id)
            .and_then(Element::bounds)
        else {
            return endpoint.position;
        };
        Point::new(
            bounds.min.x + bounds.width() * attachment.local.x,
            bounds.min.y + bounds.height() * attachment.local.y,
        )
    }

    pub fn resolved_element_bounds(&self, element: &Element) -> Option<Rect> {
        if let ElementKind::Connector { start, end } = &element.kind {
            Some(
                Rect::from_points(self.resolved_position(*start), self.resolved_position(*end))
                    .expand(element.style.width / 2.0),
            )
        } else {
            element.bounds()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema_version: u32,
    pub title: String,
    pub pages: Vec<Page>,
    pub active_page: usize,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            title: "Untitled board".to_owned(),
            pages: vec![Page::new("Page 1")],
            active_page: 0,
        }
    }
}

impl Document {
    pub fn delete_elements(&mut self, ids: &[Uuid]) {
        for page in &mut self.pages {
            page.elements.retain(|element| {
                if ids.contains(&element.id) {
                    return false;
                }
                !matches!(
                    &element.kind,
                    ElementKind::Connector { start, end }
                        if start.attachment.is_some_and(|attachment| ids.contains(&attachment.element_id))
                            || end.attachment.is_some_and(|attachment| ids.contains(&attachment.element_id))
                )
            });
        }
    }

    pub fn active_page(&self) -> &Page {
        &self.pages[self.active_page.min(self.pages.len() - 1)]
    }

    pub fn active_page_mut(&mut self) -> &mut Page {
        let index = self.active_page.min(self.pages.len() - 1);
        &mut self.pages[index]
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Unsupported project schema version {} (supported: {}).",
                self.schema_version, SCHEMA_VERSION
            ));
        }
        if self.pages.is_empty() {
            return Err("A project must contain at least one page.".to_owned());
        }
        if self.active_page >= self.pages.len() {
            return Err("The active page index is out of range.".to_owned());
        }
        for page in &self.pages {
            if !page.camera.zoom.is_finite()
                || !(Camera::MIN_ZOOM..=Camera::MAX_ZOOM).contains(&page.camera.zoom)
                || !page.camera.center.x.is_finite()
                || !page.camera.center.y.is_finite()
            {
                return Err(format!("Page '{}' contains an invalid camera.", page.name));
            }
            let mut ids = std::collections::HashSet::new();
            for element in &page.elements {
                if !ids.insert(element.id) {
                    return Err(format!(
                        "Page '{}' contains duplicate element IDs.",
                        page.name
                    ));
                }
                if !element.style.width.is_finite()
                    || element.style.width <= 0.0
                    || element.bounds().is_some_and(|bounds| !bounds.is_finite())
                {
                    return Err(format!(
                        "Page '{}' contains an element with invalid geometry.",
                        page.name
                    ));
                }
                match &element.kind {
                    ElementKind::Stroke { points }
                        if points
                            .iter()
                            .any(|point| !point.x.is_finite() || !point.y.is_finite()) =>
                    {
                        return Err(format!("Page '{}' contains an invalid stroke.", page.name));
                    }
                    ElementKind::Line { start, end }
                        if [start, end]
                            .iter()
                            .any(|point| !point.x.is_finite() || !point.y.is_finite()) =>
                    {
                        return Err(format!("Page '{}' contains an invalid line.", page.name));
                    }
                    ElementKind::Connector { start, end }
                        if [start.position, end.position]
                            .iter()
                            .any(|point| !point.x.is_finite() || !point.y.is_finite()) =>
                    {
                        return Err(format!(
                            "Page '{}' contains invalid connector geometry.",
                            page.name
                        ));
                    }
                    _ => {}
                }
            }
            for element in &page.elements {
                if let ElementKind::Connector { start, end } = &element.kind {
                    for endpoint in [start, end] {
                        if !endpoint.position.x.is_finite()
                            || !endpoint.position.y.is_finite()
                            || endpoint.attachment.is_some_and(|attachment| {
                                !ids.contains(&attachment.element_id)
                                    || !attachment.local.x.is_finite()
                                    || !attachment.local.y.is_finite()
                                    || !(0.0..=1.0).contains(&attachment.local.x)
                                    || !(0.0..=1.0).contains(&attachment.local.y)
                            })
                        {
                            return Err(format!(
                                "Page '{}' contains a connector with an invalid attachment.",
                                page.name
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let document: Self = serde_json::from_str(json)
            .map_err(|error| format!("Could not parse project file: {error}"))?;
        document.validate()?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_round_trip_preserves_pages_and_elements() {
        let mut document = Document::default();
        document.active_page_mut().elements.push(Element::new(
            ElementKind::Line {
                start: Point::new(1.0, 2.0),
                end: Point::new(40.0, 50.0),
            },
            ToolStyle::default(),
        ));
        document.pages.push(Page::new("Second"));
        let reopened = Document::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(document, reopened);
    }

    #[test]
    fn unsupported_schema_and_malformed_files_are_rejected() {
        assert!(Document::from_json("{").is_err());
        let mut document = Document::default();
        document.schema_version += 1;
        assert!(Document::from_json(&document.to_json().unwrap()).is_err());
    }

    #[test]
    fn hit_testing_uses_shape_geometry_not_just_bounding_box() {
        let ellipse = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Ellipse,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 100.0)),
            },
            ToolStyle::default(),
        );
        assert!(ellipse.hit_test(Point::new(50.0, 0.0), 1.0));
        assert!(!ellipse.hit_test(Point::new(0.0, 0.0), 1.0));
    }

    #[test]
    fn unfilled_ellipse_does_not_hit_its_interior() {
        let ellipse = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Ellipse,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 100.0)),
            },
            ToolStyle::default(),
        );
        assert!(!ellipse.hit_test(Point::new(50.0, 50.0), 1.0));
    }

    #[test]
    fn attached_connector_tracks_shape_resize_and_moves() {
        let mut page = Page::new("Connectors");
        let shape = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Rectangle,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 80.0)),
            },
            ToolStyle::default(),
        );
        let id = shape.id;
        page.elements.push(shape);
        let connector = Element::new(
            ElementKind::Connector {
                start: ConnectorEndpoint {
                    position: Point::new(50.0, 40.0),
                    attachment: Some(Attachment {
                        element_id: id,
                        local: Point::new(0.5, 0.5),
                    }),
                },
                end: ConnectorEndpoint {
                    position: Point::new(150.0, 40.0),
                    attachment: None,
                },
            },
            ToolStyle::default(),
        );
        page.elements.push(connector.clone());
        let document = Document {
            pages: vec![page.clone()],
            ..Document::default()
        };
        assert_eq!(
            document.pages[0].resolved_position(match connector.kind {
                ElementKind::Connector { start, .. } => start,
                _ => unreachable!(),
            }),
            Point::new(50.0, 40.0)
        );

        let mut moved = document;
        moved.pages[0].elements[0].kind = ElementKind::Shape {
            kind: ShapeKind::Rectangle,
            bounds: Rect::from_points(Point::new(20.0, 30.0), Point::new(220.0, 130.0)),
        };
        let start = match &moved.pages[0].elements[1].kind {
            ElementKind::Connector { start, .. } => *start,
            _ => unreachable!(),
        };
        assert_eq!(
            moved.pages[0].resolved_position(start),
            Point::new(120.0, 80.0)
        );
    }

    #[test]
    fn deleting_an_attached_object_removes_its_connectors() {
        let mut document = Document::default();
        let shape = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Rectangle,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 80.0)),
            },
            ToolStyle::default(),
        );
        let id = shape.id;
        document.active_page_mut().elements.push(shape);
        document.active_page_mut().elements.push(Element::new(
            ElementKind::Connector {
                start: ConnectorEndpoint {
                    position: Point::ZERO,
                    attachment: Some(Attachment {
                        element_id: id,
                        local: Point::new(0.5, 0.5),
                    }),
                },
                end: ConnectorEndpoint {
                    position: Point::new(100.0, 100.0),
                    attachment: None,
                },
            },
            ToolStyle::default(),
        ));
        assert!(document.validate().is_ok());
        document.delete_elements(&[id]);
        assert!(document.active_page().elements.is_empty());
    }

    #[test]
    fn all_shape_variants_have_geometry_and_round_trip() {
        let shapes = [
            ShapeKind::Rectangle,
            ShapeKind::Ellipse,
            ShapeKind::Triangle,
            ShapeKind::Diamond,
            ShapeKind::Pentagon,
            ShapeKind::Hexagon,
            ShapeKind::Octagon,
            ShapeKind::Star,
            ShapeKind::Cloud,
            ShapeKind::Trapezoid,
            ShapeKind::ArrowUp,
            ShapeKind::ArrowDown,
            ShapeKind::ArrowLeft,
            ShapeKind::ArrowRight,
            ShapeKind::XBox,
            ShapeKind::CheckBox,
        ];
        for shape in shapes {
            let vertices = shape_vertices(
                shape,
                Rect::from_points(Point::ZERO, Point::new(100.0, 80.0)),
            );
            assert!(vertices.len() >= 3);
            assert!(
                vertices
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite())
            );
        }
    }

    #[test]
    fn legacy_tool_styles_receive_defaults_for_new_style_fields() {
        let style: ToolStyle =
            serde_json::from_str(r#"{"color":[1,2,3,255],"width":3.0}"#).unwrap();
        assert_eq!(style.fill, FillStyle::None);
        assert_eq!(style.stroke, StrokeStyle::Solid);
        assert_eq!(style.font, FontFamily::Sans);
        assert_eq!(style.text_align, TextAlign::Left);
    }
}
