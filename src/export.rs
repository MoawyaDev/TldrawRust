use crate::document::{
    Document, Element, ElementKind, FillStyle, FontFamily, Page, ShapeKind, StrokeStyle, TextAlign,
};
use crate::geometry::{Point, Rect, shape_vertices};
use fontdue::{Font, FontSettings, Metrics};
use std::fmt::Write;
use std::sync::OnceLock;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

const EXPORT_PADDING: f64 = 24.0;
const MAX_EXPORT_DIMENSION: f64 = 8192.0;
static EXPORT_FONTS: OnceLock<Option<[Font; 3]>> = OnceLock::new();

pub fn export_bounds(page: &Page) -> Rect {
    page.content_bounds().map_or_else(
        || Rect::from_points(Point::ZERO, Point::new(800.0, 600.0)),
        |bounds| bounds.expand(EXPORT_PADDING),
    )
}

pub fn export_svg(document: &Document, page: &Page) -> String {
    let bounds = export_bounds(page);
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}"><defs><pattern id="hatch" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><path d="M 0 0 V 8" stroke="#777" stroke-opacity=".42" stroke-width="1"/></pattern><marker id="arrow" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto" markerUnits="strokeWidth"><path d="M 0 0 L 10 5 L 0 10 z" fill="#333"/></marker></defs>"##,
        fmt(bounds.min.x),
        fmt(bounds.min.y),
        fmt(bounds.width().max(1.0)),
        fmt(bounds.height().max(1.0))
    );
    let _ = write!(svg, "<title>{}</title>", xml_escape(&document.title));
    for element in &page.elements {
        write_svg_element(&mut svg, page, element);
    }
    svg.push_str("</svg>");
    svg
}

pub fn export_png(page: &Page, scale: f64) -> Result<Vec<u8>, String> {
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("PNG scale must be between 0.25 and 8.".to_owned());
    }
    let bounds = export_bounds(page);
    let width = (bounds.width() * scale).ceil().max(1.0);
    let height = (bounds.height() * scale).ceil().max(1.0);
    if width > MAX_EXPORT_DIMENSION || height > MAX_EXPORT_DIMENSION {
        return Err(format!(
            "PNG dimensions ({width:.0}x{height:.0}) exceed the {} pixel limit.",
            MAX_EXPORT_DIMENSION as u32
        ));
    }
    let mut pixmap = Pixmap::new(width as u32, height as u32)
        .ok_or_else(|| "Could not allocate the PNG export buffer.".to_owned())?;
    for element in &page.elements {
        paint_png_element(&mut pixmap, page, element, bounds.min, scale);
    }
    pixmap
        .encode_png()
        .map_err(|error| format!("Could not encode PNG: {error}"))
}

fn write_svg_element(svg: &mut String, page: &Page, element: &Element) {
    let color = element.style.color;
    let mut stroke = format!(
        r##"stroke="#{:02x}{:02x}{:02x}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""##,
        color[0],
        color[1],
        color[2],
        fmt(color[3] as f64 / 255.0),
        fmt(element.style.width)
    );
    match element.style.stroke {
        StrokeStyle::Dashed => stroke.push_str(r#" stroke-dasharray="10 7""#),
        StrokeStyle::Dotted => stroke.push_str(r#" stroke-dasharray="1 6""#),
        StrokeStyle::Draw | StrokeStyle::Solid => {}
    }
    let fill = match element.style.fill {
        FillStyle::None => "fill=\"none\"".to_owned(),
        FillStyle::Semi => format!(
            r##"fill="#{:02x}{:02x}{:02x}" fill-opacity="{}""##,
            color[0],
            color[1],
            color[2],
            fmt(0.2 * color[3] as f64 / 255.0)
        ),
        FillStyle::Solid => format!(
            r##"fill="#{:02x}{:02x}{:02x}" fill-opacity="{}""##,
            color[0],
            color[1],
            color[2],
            fmt(color[3] as f64 / 255.0)
        ),
        FillStyle::Pattern => "fill=\"url(#hatch)\"".to_owned(),
    };
    match &element.kind {
        ElementKind::Stroke { points } => {
            if let Some(first) = points.first() {
                let data = points_path(points, *first, false);
                let _ = write!(svg, r#"<path d="{}" {} fill="none"/>"#, data, stroke);
            }
        }
        ElementKind::Line { start, end } => {
            let _ = write!(
                svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {}/>"#,
                fmt(start.x),
                fmt(start.y),
                fmt(end.x),
                fmt(end.y),
                stroke
            );
        }
        ElementKind::Connector { start, end } => {
            let start = page.resolved_position(*start);
            let end = page.resolved_position(*end);
            let _ = write!(
                svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {} marker-end="url(#arrow)"/>"#,
                fmt(start.x),
                fmt(start.y),
                fmt(end.x),
                fmt(end.y),
                stroke
            );
        }
        ElementKind::Shape { kind, bounds } => {
            write_svg_shape(svg, *kind, *bounds, &fill, &stroke);
        }
        ElementKind::Note { bounds, text } => {
            let _ = write!(
                svg,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#fff1a8" stroke="#d7b83e" stroke-width="1"/>"##,
                fmt(bounds.min.x),
                fmt(bounds.min.y),
                fmt(bounds.width()),
                fmt(bounds.height())
            );
            write_svg_text(
                svg,
                text,
                bounds.min,
                element.style.font,
                element.style.text_align,
                16.0,
            );
        }
        ElementKind::Frame { bounds, title } => {
            let _ = write!(
                svg,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#8b9bb0" stroke-width="2" stroke-dasharray="8 6"/>"##,
                fmt(bounds.min.x),
                fmt(bounds.min.y),
                fmt(bounds.width()),
                fmt(bounds.height())
            );
            write_svg_text(
                svg,
                title,
                Point::new(bounds.min.x + 6.0, bounds.min.y + 20.0),
                FontFamily::Sans,
                TextAlign::Left,
                15.0,
            );
        }
    }
}

fn write_svg_shape(svg: &mut String, kind: ShapeKind, bounds: Rect, fill: &str, stroke: &str) {
    let vertices = shape_vertices(kind, bounds);
    let data = points_path(&vertices, vertices[0], true);
    let _ = write!(svg, r#"<path d="{}" {} {}/>"#, data, fill, stroke);
    if kind == ShapeKind::CheckBox {
        let check = [
            Point::new(
                bounds.min.x + bounds.width() * 0.18,
                bounds.min.y + bounds.height() * 0.52,
            ),
            Point::new(
                bounds.min.x + bounds.width() * 0.43,
                bounds.min.y + bounds.height() * 0.76,
            ),
            Point::new(
                bounds.min.x + bounds.width() * 0.84,
                bounds.min.y + bounds.height() * 0.22,
            ),
        ];
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="#111827" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"##,
            points_path(&check[1..], check[0], false),
            fmt((bounds.width().min(bounds.height()) * 0.08).max(2.0))
        );
    }
}

fn write_svg_text(
    svg: &mut String,
    text: &str,
    origin: Point,
    family: FontFamily,
    align: TextAlign,
    size: f64,
) {
    let family = match family {
        FontFamily::Sans => "sans-serif",
        FontFamily::Mono => "monospace",
        FontFamily::Script => "cursive",
    };
    let anchor = match align {
        TextAlign::Left => "start",
        TextAlign::Center => "middle",
        TextAlign::Right => "end",
    };
    for (index, line) in text.lines().enumerate() {
        let _ = write!(
            svg,
            r##"<text x="{}" y="{}" text-anchor="{}" font-family="{}" font-size="{}" fill="#373120">{}</text>"##,
            fmt(origin.x),
            fmt(origin.y + index as f64 * size * 1.3),
            anchor,
            family,
            fmt(size),
            xml_escape(line)
        );
    }
}

fn points_path(points: &[Point], first: Point, close: bool) -> String {
    let mut data = format!("M {} {}", fmt(first.x), fmt(first.y));
    for point in points.iter().skip(1) {
        let _ = write!(data, " L {} {}", fmt(point.x), fmt(point.y));
    }
    if close {
        data.push_str(" Z");
    }
    data
}

fn paint_png_element(
    pixmap: &mut Pixmap,
    page: &Page,
    element: &Element,
    origin: Point,
    scale: f64,
) {
    let transform = Transform::from_scale(scale as f32, scale as f32).post_translate(
        -(origin.x as f32) * scale as f32,
        -(origin.y as f32) * scale as f32,
    );
    let mut paint = Paint::default();
    let alpha = match element.style.fill {
        FillStyle::Semi | FillStyle::Pattern => (element.style.color[3] as f64 * 0.2) as u8,
        _ => element.style.color[3],
    };
    paint.set_color(Color::from_rgba8(
        element.style.color[0],
        element.style.color[1],
        element.style.color[2],
        alpha,
    ));
    paint.anti_alias = true;
    let stroke = Stroke {
        width: (element.style.width * scale) as f32,
        dash: match element.style.stroke {
            StrokeStyle::Dashed => Some(tiny_skia::StrokeDash::new(vec![10.0, 7.0], 0.0).unwrap()),
            StrokeStyle::Dotted => Some(tiny_skia::StrokeDash::new(vec![1.0, 6.0], 0.0).unwrap()),
            StrokeStyle::Draw | StrokeStyle::Solid => None,
        },
        ..Stroke::default()
    };
    match &element.kind {
        ElementKind::Stroke { points } => {
            if let Some(path) = path_from_points(points, false) {
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }
        ElementKind::Line { start, end } => {
            if let Some(path) = path_from_points(&[*start, *end], false) {
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }
        ElementKind::Connector { start, end } => {
            let start = page.resolved_position(*start);
            let end = page.resolved_position(*end);
            if let Some(path) = path_from_points(&[start, end], false) {
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
            draw_arrowhead(pixmap, start, end, &paint, transform, scale);
        }
        ElementKind::Shape { kind, bounds } => {
            let path = shape_path(*kind, *bounds);
            if element.style.fill != FillStyle::None {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                if element.style.fill == FillStyle::Pattern {
                    draw_hatch(
                        pixmap,
                        &path,
                        *bounds,
                        transform,
                        scale,
                        element.style.color,
                    );
                }
            }
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            if *kind == ShapeKind::CheckBox {
                draw_checkbox_tick(pixmap, *bounds, transform, scale);
            }
        }
        ElementKind::Note { bounds, text } => {
            let mut note_paint = Paint::default();
            note_paint.set_color(Color::from_rgba8(255, 241, 168, 255));
            note_paint.anti_alias = true;
            let path = shape_path(ShapeKind::Rectangle, *bounds);
            pixmap.fill_path(&path, &note_paint, FillRule::Winding, transform, None);
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            draw_text(
                pixmap,
                text,
                *bounds,
                element,
                origin,
                scale,
                [55, 49, 32, 255],
            );
        }
        ElementKind::Frame { bounds, title } => {
            let path = shape_path(ShapeKind::Rectangle, *bounds);
            let mut frame_stroke = stroke.clone();
            frame_stroke.dash = tiny_skia::StrokeDash::new(vec![8.0, 6.0], 0.0);
            pixmap.stroke_path(&path, &paint, &frame_stroke, transform, None);
            draw_text(
                pixmap,
                title,
                *bounds,
                element,
                origin,
                scale,
                element.style.color,
            );
        }
    }
}

fn draw_arrowhead(
    pixmap: &mut Pixmap,
    start: Point,
    end: Point,
    paint: &Paint,
    transform: Transform,
    scale: f64,
) {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let length = 12.0;
    let left = Point::new(
        end.x - length * (angle - 0.45).cos(),
        end.y - length * (angle - 0.45).sin(),
    );
    let right = Point::new(
        end.x - length * (angle + 0.45).cos(),
        end.y - length * (angle + 0.45).sin(),
    );
    if let Some(path) = path_from_points(&[left, end, right], false) {
        pixmap.stroke_path(
            &path,
            paint,
            &Stroke {
                width: (2.0 * scale) as f32,
                ..Stroke::default()
            },
            transform,
            None,
        );
    }
}

fn draw_hatch(
    pixmap: &mut Pixmap,
    shape_path: &tiny_skia::Path,
    bounds: Rect,
    transform: Transform,
    scale: f64,
    color: [u8; 4],
) {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color[0], color[1], color[2], 120));
    paint.anti_alias = true;
    let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
        .expect("mask dimensions match pixmap");
    mask.fill_path(shape_path, FillRule::Winding, true, transform);
    let start = bounds.min.x - bounds.height();
    let end = bounds.max.x;
    let mut x = start;
    while x <= end {
        let line = [
            Point::new(x, bounds.max.y),
            Point::new(x + bounds.height(), bounds.min.y),
        ];
        if let Some(hatch_path) = path_from_points(&line, false) {
            pixmap.stroke_path(
                &hatch_path,
                &paint,
                &Stroke {
                    width: (0.75 * scale) as f32,
                    ..Stroke::default()
                },
                transform,
                Some(&mask),
            );
        }
        x += 8.0;
    }
}

fn draw_checkbox_tick(pixmap: &mut Pixmap, bounds: Rect, transform: Transform, scale: f64) {
    let points = [
        Point::new(
            bounds.min.x + bounds.width() * 0.18,
            bounds.min.y + bounds.height() * 0.52,
        ),
        Point::new(
            bounds.min.x + bounds.width() * 0.43,
            bounds.min.y + bounds.height() * 0.76,
        ),
        Point::new(
            bounds.min.x + bounds.width() * 0.84,
            bounds.min.y + bounds.height() * 0.22,
        ),
    ];
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(17, 24, 39, 255));
    if let Some(path) = path_from_points(&points, false) {
        pixmap.stroke_path(
            &path,
            &paint,
            &Stroke {
                width: (bounds.width().min(bounds.height()) * 0.08 * scale) as f32,
                ..Stroke::default()
            },
            transform,
            None,
        );
    }
}

fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    bounds: Rect,
    element: &Element,
    offset: Point,
    scale: f64,
    color: [u8; 4],
) {
    let Some(fonts) = export_fonts() else {
        return;
    };
    let font = match element.style.font {
        FontFamily::Sans => &fonts[0],
        FontFamily::Mono => &fonts[1],
        FontFamily::Script => &fonts[2],
    };
    let font_size = 16.0 * scale;
    let max_width = ((bounds.width() - 20.0) * scale).max(1.0) as f32;
    let text_width = |value: &str| -> f32 {
        value
            .chars()
            .map(|character| font.metrics(character, font_size as f32).advance_width)
            .sum()
    };
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text_width(&candidate) > max_width {
                lines.push(std::mem::take(&mut line));
                line.push_str(word);
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    let clip = [
        ((bounds.min.x + 6.0 - offset.x) * scale).floor() as i32,
        ((bounds.min.y + 6.0 - offset.y) * scale).floor() as i32,
        ((bounds.max.x - 6.0 - offset.x) * scale).ceil() as i32,
        ((bounds.max.y - 6.0 - offset.y) * scale).ceil() as i32,
    ];
    for (row, line) in lines.iter().enumerate() {
        let width = text_width(line);
        let mut pen_x = match element.style.text_align {
            TextAlign::Left => (bounds.min.x + 10.0) * scale,
            TextAlign::Center => bounds.center().x * scale - width as f64 / 2.0,
            TextAlign::Right => (bounds.max.x - 10.0) * scale - width as f64,
        };
        let baseline = (bounds.min.y + 24.0 + row as f64 * 20.0) * scale;
        if baseline > (bounds.max.y - 6.0) * scale {
            break;
        }
        for character in line.chars() {
            let (metrics, bitmap) = font.rasterize(character, font_size as f32);
            let left = pen_x + metrics.xmin as f64;
            let top = baseline - metrics.height as f64 - metrics.ymin as f64;
            blend_glyph(
                pixmap,
                &bitmap,
                metrics,
                left - offset.x * scale,
                top - offset.y * scale,
                color,
                clip,
            );
            pen_x += metrics.advance_width as f64;
        }
    }
}

fn blend_glyph(
    pixmap: &mut Pixmap,
    bitmap: &[u8],
    metrics: Metrics,
    left: f64,
    top: f64,
    color: [u8; 4],
    clip: [i32; 4],
) {
    let origin_x = left.round() as i32;
    let origin_y = top.round() as i32;
    for y in 0..metrics.height {
        for x in 0..metrics.width {
            let coverage = bitmap[y * metrics.width + x] as u32;
            if coverage == 0 {
                continue;
            }
            let px = origin_x + x as i32;
            let py = origin_y + y as i32;
            if px < 0
                || py < 0
                || px >= pixmap.width() as i32
                || py >= pixmap.height() as i32
                || px < clip[0]
                || py < clip[1]
                || px >= clip[2]
                || py >= clip[3]
            {
                continue;
            }
            let alpha = coverage * color[3] as u32 / 255;
            let index = py as usize * pixmap.width() as usize + px as usize;
            let dest = pixmap.pixels_mut()[index];
            let inverse = 255 - alpha;
            let src = [
                color[0] as u32 * alpha / 255,
                color[1] as u32 * alpha / 255,
                color[2] as u32 * alpha / 255,
                alpha,
            ];
            let dst = [
                dest.red() as u32,
                dest.green() as u32,
                dest.blue() as u32,
                dest.alpha() as u32,
            ];
            let blended = [
                (src[0] + dst[0] * inverse / 255).min(255) as u8,
                (src[1] + dst[1] * inverse / 255).min(255) as u8,
                (src[2] + dst[2] * inverse / 255).min(255) as u8,
                (src[3] + dst[3] * inverse / 255).min(255) as u8,
            ];
            pixmap.pixels_mut()[index] = tiny_skia::PremultipliedColorU8::from_rgba(
                blended[0], blended[1], blended[2], blended[3],
            )
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
    }
}

fn export_fonts() -> Option<&'static [Font; 3]> {
    EXPORT_FONTS
        .get_or_init(|| {
            let sans = load_font(&[
                "C:\\Windows\\Fonts\\arial.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/System/Library/Fonts/Supplemental/Arial.ttf",
            ])
            .or_else(|| bundled_font(epaint_default_fonts::UBUNTU_LIGHT))?;
            let mono = load_font(&[
                "C:\\Windows\\Fonts\\consola.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                "/System/Library/Fonts/Supplemental/Menlo.ttc",
            ])
            .or_else(|| bundled_font(epaint_default_fonts::HACK_REGULAR))
            .unwrap_or_else(|| sans.clone());
            let script = load_font(&[
                "C:\\Windows\\Fonts\\segoesc.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
                "/System/Library/Fonts/Supplemental/Apple Chancery.ttf",
            ])
            .unwrap_or_else(|| sans.clone());
            Some([sans, mono, script])
        })
        .as_ref()
}

fn bundled_font(data: &'static [u8]) -> Option<Font> {
    Font::from_bytes(data, FontSettings::default()).ok()
}

fn load_font(paths: &[&str]) -> Option<Font> {
    paths.iter().find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        Font::from_bytes(bytes, FontSettings::default()).ok()
    })
}

fn shape_path(kind: ShapeKind, bounds: Rect) -> tiny_skia::Path {
    let vertices = shape_vertices(kind, bounds);
    path_from_points(&vertices, true).expect("shape geometry always contains vertices")
}

fn path_from_points(points: &[Point], close: bool) -> Option<tiny_skia::Path> {
    let first = points.first()?;
    let mut builder = PathBuilder::new();
    builder.move_to(first.x as f32, first.y as f32);
    for point in points.iter().skip(1) {
        builder.line_to(point.x as f32, point.y as f32);
    }
    if close {
        builder.close();
    }
    builder.finish()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn fmt(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Attachment, ConnectorEndpoint, ElementKind, ToolStyle};

    #[test]
    fn svg_preserves_extended_geometry_and_styles() {
        let mut document = Document::default();
        let shape = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Star,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 80.0)),
            },
            ToolStyle {
                fill: FillStyle::Pattern,
                stroke: StrokeStyle::Dashed,
                ..ToolStyle::default()
            },
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
                    position: Point::new(140.0, 40.0),
                    attachment: None,
                },
            },
            ToolStyle::default(),
        ));
        let svg = export_svg(&document, document.active_page());
        assert!(svg.contains("<pattern"));
        assert!(svg.contains("stroke-dasharray=\"10 7\""));
        assert!(svg.contains("marker-end"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn svg_escapes_note_text_and_emits_text_nodes() {
        let mut document = Document::default();
        document.active_page_mut().elements.push(Element::new(
            ElementKind::Note {
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 80.0)),
                text: "<hello & goodbye>".to_owned(),
            },
            ToolStyle::default(),
        ));
        let svg = export_svg(&document, document.active_page());
        assert!(svg.contains("&lt;hello &amp; goodbye&gt;"));
        assert!(svg.contains("<text"));
        assert!(!svg.contains("<image"));
    }

    #[test]
    fn png_export_produces_a_real_png_for_empty_and_text_pages() {
        let document = Document::default();
        let png = export_png(document.active_page(), 1.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let mut page = Page::new("Text");
        page.elements.push(Element::new(
            ElementKind::Note {
                bounds: Rect::from_points(Point::ZERO, Point::new(180.0, 110.0)),
                text: "Readable export".to_owned(),
            },
            crate::document::ToolStyle::default(),
        ));
        let png = export_png(&page, 1.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = Pixmap::decode_png(&png).unwrap();
        assert!(
            decoded.pixels().iter().any(|pixel| {
                pixel.alpha() > 0 && pixel.red() < 100 && pixel.green() < 100 && pixel.blue() < 100
            }),
            "PNG output should contain rasterized note glyphs."
        );
        assert!(
            export_fonts().is_some(),
            "A system sans-serif font is required for text rasterization."
        );
    }

    #[test]
    fn export_rejects_unbounded_allocations() {
        let mut page = Page::new("Large");
        page.elements.push(Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Rectangle,
                bounds: Rect::from_points(Point::ZERO, Point::new(100_000.0, 100_000.0)),
            },
            ToolStyle::default(),
        ));
        assert!(export_png(&page, 1.0).is_err());
    }
}
