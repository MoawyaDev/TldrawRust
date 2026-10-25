use crate::document::{
    Attachment, ConnectorEndpoint, Document, Element, ElementKind, FillStyle, FontFamily, Page,
    ShapeKind, StrokeStyle, TextAlign, ToolStyle,
};
use crate::export::{export_png, export_svg};
use crate::geometry::{
    Camera, Point, Rect, distance_to_segment, point_in_polygon, shape_vertices, triangulate_polygon,
};
use crate::history::History;
use eframe::egui::{
    self, Align2, Color32, FontId, Id, Key, PointerButton, Pos2, Rect as EguiRect, Sense, Stroke,
    StrokeKind, Vec2,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uuid::Uuid;

const PALETTE: [[u8; 4]; 16] = [
    [31, 41, 55, 255],
    [107, 114, 128, 255],
    [220, 38, 38, 255],
    [234, 88, 12, 255],
    [202, 138, 4, 255],
    [101, 163, 13, 255],
    [22, 163, 74, 255],
    [13, 148, 136, 255],
    [8, 145, 178, 255],
    [37, 99, 235, 255],
    [79, 70, 229, 255],
    [124, 58, 237, 255],
    [192, 38, 211, 255],
    [219, 39, 119, 255],
    [255, 255, 255, 255],
    [0, 0, 0, 255],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Select,
    Lasso,
    Draw,
    Line,
    SmartArrow,
    Shape(ShapeKind),
    Note,
    Frame,
    Laser,
    Eraser,
}

impl Tool {
    const ALL: [(Self, &'static str); 9] = [
        (Self::Select, "↖ Select"),
        (Self::Lasso, "◌ Lasso"),
        (Self::Draw, "✎ Draw"),
        (Self::Line, "／ Line"),
        (Self::SmartArrow, "↗ Arrow"),
        (Self::Note, "▤ Note"),
        (Self::Frame, "▣ Frame"),
        (Self::Laser, "✦ Laser"),
        (Self::Eraser, "⌫ Erase"),
    ];
}

#[derive(Clone)]
enum Interaction {
    Drawing {
        points: Vec<Point>,
        style: ToolStyle,
    },
    Creating {
        tool: Tool,
        start: Point,
        current: Point,
        style: ToolStyle,
    },
    Lasso {
        points: Vec<Point>,
        additive: bool,
    },
    Moving {
        ids: Vec<Uuid>,
        start: Point,
        base: Document,
        selection_bounds: Rect,
    },
    Resizing {
        ids: Vec<Uuid>,
        base_bounds: Rect,
        anchor: Point,
        base: Document,
    },
    Connecting {
        start: ConnectorEndpoint,
        current: Point,
    },
    Laser {
        points: Vec<(Point, Instant)>,
    },
    Erasing {
        last: Point,
        base: Document,
    },
    Panning {
        last: Point,
    },
}

#[derive(Clone, Copy)]
struct CameraMotion {
    from: Camera,
    to: Camera,
    elapsed: f64,
    duration: f64,
}

pub struct GpuiiApp {
    document: Document,
    history: History<Document>,
    selected: Vec<Uuid>,
    tool: Tool,
    style: ToolStyle,
    interaction: Option<Interaction>,
    file_path: Option<PathBuf>,
    dirty: bool,
    grid_enabled: bool,
    status: String,
    dark_theme: bool,
    diagnostics: bool,
    snap_guides: Vec<(Point, Point)>,
    camera_motion: Option<CameraMotion>,
    pan_velocity: Point,
    last_frame: Instant,
    frame_time_seconds: f32,
    laser_trail: Vec<(Point, Instant)>,
    edit_page: Option<Uuid>,
    renamed_page: String,
    text_edit_base: Option<(Uuid, String)>,
    snap_enabled: bool,
    inertia_enabled: bool,
}

impl Default for GpuiiApp {
    fn default() -> Self {
        let document = Document::default();
        Self {
            history: History::new(document.clone(), 100),
            document,
            selected: Vec::new(),
            tool: Tool::Select,
            style: ToolStyle::default(),
            interaction: None,
            file_path: None,
            dirty: false,
            grid_enabled: true,
            status: "Ready".to_owned(),
            dark_theme: false,
            diagnostics: false,
            snap_guides: Vec::new(),
            camera_motion: None,
            pan_velocity: Point::ZERO,
            last_frame: Instant::now(),
            frame_time_seconds: 1.0 / 60.0,
            laser_trail: Vec::new(),
            edit_page: None,
            renamed_page: String::new(),
            text_edit_base: None,
            snap_enabled: true,
            inertia_enabled: true,
        }
    }
}

impl eframe::App for GpuiiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().set_visuals(if self.dark_theme {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let now = Instant::now();
        let frame_delta = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.frame_time_seconds = frame_delta;
        self.tick_animations(ui.ctx(), frame_delta);
        self.handle_shortcuts(ui.ctx());
        self.top_bar(ui);
        self.right_inspector(ui);
        self.status_bar(ui);
        self.bottom_pages(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(self.canvas_color()))
            .show(ui, |ui| self.canvas(ui));
        self.bottom_tool_palette(ui.ctx());
        self.paint_diagnostics(ui.ctx());
    }
}

impl GpuiiApp {
    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    ui.separator();
                    ui.label(format!(
                        "{} objects",
                        self.document.active_page().elements.len()
                    ));
                    ui.separator();
                    ui.label(format!(
                        "{:.0}%",
                        self.document.active_page().camera.zoom * 100.0
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(if self.dirty {
                            "Unsaved changes"
                        } else {
                            "Saved"
                        });
                    });
                });
            });
    }
    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .exact_size(44.0)
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.strong("GPUII");
                    ui.separator();
                    ui.label(if self.dirty {
                        format!("{} *", self.document.title)
                    } else {
                        self.document.title.clone()
                    });
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("New project").clicked() {
                            self.new_project();
                            ui.close();
                        }
                        if ui.button("Open project…").clicked() {
                            self.open_project();
                            ui.close();
                        }
                        if ui.button("Save").clicked() {
                            self.save_project(false);
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.save_project(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Export PNG…").clicked() {
                            self.export_png_dialog();
                            ui.close();
                        }
                        if ui.button("Export SVG…").clicked() {
                            self.export_svg_dialog();
                            ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        if ui.button("Delete selection").clicked() {
                            self.delete_selected();
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.grid_enabled, "Show grid");
                        ui.checkbox(&mut self.dark_theme, "Dark theme");
                        ui.checkbox(&mut self.diagnostics, "Diagnostics");
                        ui.checkbox(&mut self.snap_enabled, "Snap to objects");
                        ui.checkbox(&mut self.inertia_enabled, "Pan inertia");
                        if ui.button("Zoom to fit").clicked() {
                            self.zoom_to_fit();
                            ui.close();
                        }
                        if ui.button("Reset camera").clicked() {
                            self.animate_camera(Camera::default());
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if ui.button("Fit").clicked() {
                        self.zoom_to_fit();
                    }
                });
            });
    }

    fn bottom_tool_palette(&mut self, ctx: &egui::Context) {
        egui::Area::new(Id::unique(egui::IdSalt::new("floating_tool_palette")))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -44.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(if self.dark_theme {
                        Color32::from_rgb(35, 39, 48)
                    } else {
                        Color32::WHITE
                    })
                    .stroke(Stroke::new(1.0, Color32::from_gray(190)))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (tool, label) in Tool::ALL {
                                let response = ui.selectable_label(self.tool == tool, label);
                                if response.clicked() {
                                    self.activate_tool(tool);
                                }
                                response.on_hover_text(format!("{label} tool"));
                            }
                            ui.menu_button("Shapes ▾", |ui| {
                                for kind in [
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
                                ] {
                                    if ui.button(shape_label(kind)).clicked() {
                                        self.activate_tool(Tool::Shape(kind));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
            });
    }

    fn right_inspector(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .resizable(false)
            .default_size(220.0)
            .show(root, |ui| {
                ui.heading("Inspector");
                ui.add_space(8.0);
                if self.selected.len() == 1 {
                    let id = self.selected[0];
                    let Some(element) = self
                        .document
                        .active_page()
                        .elements
                        .iter()
                        .find(|element| element.id == id)
                        .cloned()
                    else {
                        self.selected.clear();
                        ui.label("Nothing selected");
                        return;
                    };
                    ui.label(element_name(&element));
                    let mut style = element.style.clone();
                    let mut changed = false;
                    ui.label("Color");
                    for row in PALETTE.chunks(8) {
                        ui.horizontal(|ui| {
                            for color in row {
                                let button = egui::Button::new("  ").fill(color32(*color)).stroke(
                                    Stroke::new(
                                        if *color == style.color { 2.0 } else { 0.5 },
                                        Color32::DARK_GRAY,
                                    ),
                                );
                                if ui.add(button).clicked() {
                                    style.color = *color;
                                    changed = true;
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.label("Stroke width");
                    for (label, width) in [("S", 2.0), ("M", 3.0), ("L", 6.0), ("XL", 10.0)] {
                        if ui
                            .selectable_label((style.width - width).abs() < 0.1, label)
                            .clicked()
                        {
                            style.width = width;
                            changed = true;
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        for fill in [
                            FillStyle::None,
                            FillStyle::Semi,
                            FillStyle::Solid,
                            FillStyle::Pattern,
                        ] {
                            if ui
                                .selectable_label(style.fill == fill, fill_label(fill))
                                .clicked()
                            {
                                style.fill = fill;
                                changed = true;
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for stroke in [
                            StrokeStyle::Draw,
                            StrokeStyle::Solid,
                            StrokeStyle::Dashed,
                            StrokeStyle::Dotted,
                        ] {
                            if ui
                                .selectable_label(style.stroke == stroke, stroke_label(stroke))
                                .clicked()
                            {
                                style.stroke = stroke;
                                changed = true;
                            }
                        }
                    });
                    if matches!(element.kind, ElementKind::Note { .. }) {
                        ui.label("Typeface");
                        ui.horizontal(|ui| {
                            for font in [FontFamily::Sans, FontFamily::Mono, FontFamily::Script] {
                                if ui
                                    .selectable_label(style.font == font, font_label(font))
                                    .clicked()
                                {
                                    style.font = font;
                                    changed = true;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                                if ui
                                    .selectable_label(
                                        style.text_align == align,
                                        text_align_label(align),
                                    )
                                    .clicked()
                                {
                                    style.text_align = align;
                                    changed = true;
                                }
                            }
                        });
                    }
                    if changed {
                        for item in &mut self.document.active_page_mut().elements {
                            if self.selected.contains(&item.id) {
                                item.style = style.clone();
                            }
                        }
                        self.commit();
                    }
                    match &element.kind {
                        ElementKind::Note { bounds, text } => {
                            let mut edited = text.clone();
                            ui.label("Note text");
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut edited)
                                    .desired_rows(5)
                                    .desired_width(f32::INFINITY),
                            );
                            if response.changed() {
                                self.text_edit_base.get_or_insert((id, text.clone()));
                                if let Some(note) = self
                                    .document
                                    .active_page_mut()
                                    .elements
                                    .iter_mut()
                                    .find(|item| item.id == id)
                                {
                                    note.kind = ElementKind::Note {
                                        bounds: *bounds,
                                        text: edited,
                                    };
                                }
                            }
                            if response.lost_focus()
                                && self
                                    .text_edit_base
                                    .as_ref()
                                    .is_some_and(|(edited_id, _)| *edited_id == id)
                                && self.text_edit_base.take().is_some()
                            {
                                self.commit();
                            }
                        }
                        ElementKind::Frame { bounds, title } => {
                            let mut edited = title.clone();
                            ui.label("Frame title");
                            let response = ui.text_edit_singleline(&mut edited);
                            if response.changed() {
                                self.text_edit_base.get_or_insert((id, title.clone()));
                                if let Some(frame) = self
                                    .document
                                    .active_page_mut()
                                    .elements
                                    .iter_mut()
                                    .find(|item| item.id == id)
                                {
                                    frame.kind = ElementKind::Frame {
                                        bounds: *bounds,
                                        title: edited,
                                    };
                                }
                            }
                            if response.lost_focus()
                                && self
                                    .text_edit_base
                                    .as_ref()
                                    .is_some_and(|(edited_id, _)| *edited_id == id)
                                && self.text_edit_base.take().is_some()
                            {
                                self.commit();
                            }
                        }
                        _ => {}
                    }
                    ui.separator();
                    if ui.button("Delete selected").clicked() {
                        self.delete_selected();
                    }
                } else if self.selected.len() > 1 {
                    ui.label(format!("{} objects selected", self.selected.len()));
                    ui.label("Shared style changes apply to the selection.");
                    let mut style = self
                        .document
                        .active_page()
                        .elements
                        .iter()
                        .find(|element| self.selected.contains(&element.id))
                        .map(|element| element.style.clone())
                        .unwrap_or_default();
                    ui.label("Color");
                    for color in PALETTE {
                        if ui
                            .add(egui::Button::new(" ").fill(color32(color)))
                            .clicked()
                        {
                            style.color = color;
                            self.apply_style_to_selection(style.clone());
                        }
                    }
                    ui.horizontal(|ui| {
                        for (label, width) in [("S", 2.0), ("M", 3.0), ("L", 6.0), ("XL", 10.0)] {
                            if ui
                                .selectable_label((style.width - width).abs() < 0.1, label)
                                .clicked()
                            {
                                style.width = width;
                                self.apply_style_to_selection(style.clone());
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for fill in [
                            FillStyle::None,
                            FillStyle::Semi,
                            FillStyle::Solid,
                            FillStyle::Pattern,
                        ] {
                            if ui
                                .selectable_label(style.fill == fill, fill_label(fill))
                                .clicked()
                            {
                                style.fill = fill;
                                self.apply_style_to_selection(style.clone());
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for stroke in [
                            StrokeStyle::Draw,
                            StrokeStyle::Solid,
                            StrokeStyle::Dashed,
                            StrokeStyle::Dotted,
                        ] {
                            if ui
                                .selectable_label(style.stroke == stroke, stroke_label(stroke))
                                .clicked()
                            {
                                style.stroke = stroke;
                                self.apply_style_to_selection(style.clone());
                            }
                        }
                    });
                    if ui.button("Delete selection").clicked() {
                        self.delete_selected();
                    }
                } else {
                    ui.label("Select an object to edit its style.");
                }
            });
    }

    fn bottom_pages(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("pages")
            .exact_size(34.0)
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let mut action = None;
                    for (index, page) in self.document.pages.iter().enumerate() {
                        let response =
                            ui.selectable_label(index == self.document.active_page, &page.name);
                        if response.clicked() {
                            self.document.active_page = index;
                            self.selected.clear();
                            self.interaction = None;
                        }
                        response.context_menu(|ui| {
                            if ui.button("Rename page").clicked() {
                                action = Some((index, 0));
                                ui.close();
                            }
                            if ui.button("Duplicate page").clicked() {
                                action = Some((index, 1));
                                ui.close();
                            }
                        });
                        if self.edit_page == Some(page.id) {
                            let response = ui.text_edit_singleline(&mut self.renamed_page);
                            if response.lost_focus()
                                || response.has_focus()
                                    && ui.input(|input| input.key_pressed(Key::Enter))
                            {
                                action = Some((index, 2));
                            }
                        }
                    }
                    if let Some((index, action)) = action {
                        match action {
                            0 => {
                                self.edit_page = Some(self.document.pages[index].id);
                                self.renamed_page = self.document.pages[index].name.clone();
                            }
                            1 => {
                                let mut duplicate = self.document.pages[index].clone();
                                duplicate.id = Uuid::new_v4();
                                duplicate.name = format!("{} copy", duplicate.name);
                                let mut ids = std::collections::HashMap::new();
                                for element in &mut duplicate.elements {
                                    ids.insert(element.id, Uuid::new_v4());
                                }
                                for element in &mut duplicate.elements {
                                    let old_id = element.id;
                                    element.id = ids[&old_id];
                                    if let ElementKind::Connector { start, end } = &mut element.kind
                                    {
                                        for endpoint in [start, end] {
                                            if let Some(attachment) = &mut endpoint.attachment
                                                && let Some(new_id) =
                                                    ids.get(&attachment.element_id)
                                            {
                                                attachment.element_id = *new_id;
                                            }
                                        }
                                    }
                                }
                                self.document.pages.insert(index + 1, duplicate);
                                self.document.active_page = index + 1;
                                self.selected.clear();
                                self.commit();
                            }
                            2 => {
                                self.document.pages[index].name =
                                    self.renamed_page.trim().to_owned();
                                self.edit_page = None;
                                self.commit();
                            }
                            _ => {}
                        }
                    }
                    if ui.button("+ Page").clicked() {
                        let number = self.document.pages.len() + 1;
                        self.document
                            .pages
                            .push(crate::document::Page::new(format!("Page {number}")));
                        self.document.active_page = self.document.pages.len() - 1;
                        self.selected.clear();
                        self.commit();
                    }
                    if self.document.pages.len() > 1 && ui.button("Delete page").clicked() {
                        self.document.pages.remove(self.document.active_page);
                        self.document.active_page =
                            self.document.active_page.min(self.document.pages.len() - 1);
                        self.selected.clear();
                        self.commit();
                    }
                });
            });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let viewport = response.rect;
        painter.rect_filled(viewport, 0.0, self.canvas_color());
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.0
                && let Some(pointer) = response.hover_pos()
            {
                let pointer = egui_to_point(pointer);
                let rect = egui_rect_to_rect(viewport);
                self.document.active_page_mut().camera.zoom_at(
                    pointer,
                    rect,
                    (scroll as f64 * 0.0015).exp(),
                );
                self.camera_motion = None;
            }
        }

        let pointer_screen = response.interact_pointer_pos();
        let pointer_world = pointer_screen.map(|point| {
            self.document
                .active_page()
                .camera
                .screen_to_world(egui_to_point(point), egui_rect_to_rect(viewport))
        });
        let pan_pressed = ui.input(|input| {
            input.pointer.button_pressed(PointerButton::Middle)
                || (input.key_down(Key::Space) && input.pointer.primary_pressed())
        });
        let pan_down = ui.input(|input| {
            input.pointer.button_down(PointerButton::Middle)
                || (input.key_down(Key::Space) && input.pointer.primary_down())
        });
        let primary_started = response.drag_started_by(PointerButton::Primary);
        let primary_stopped = response.drag_stopped_by(PointerButton::Primary);
        let additive = ui.input(|input| input.modifiers.command || input.modifiers.shift);

        if pan_pressed {
            if let Some(point) = pointer_screen {
                self.pan_velocity = Point::ZERO;
                self.interaction = Some(Interaction::Panning {
                    last: egui_to_point(point),
                });
            }
        } else if primary_started
            && response.hovered()
            && let Some(point) = pointer_world
        {
            self.begin_interaction(point, additive);
        }

        if let Some(interaction) = self.interaction.clone() {
            match interaction {
                Interaction::Drawing { mut points, style } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                        && points.last().is_none_or(|last| last.distance(point) > 1.0)
                    {
                        points.push(point);
                    }
                    self.interaction = Some(Interaction::Drawing { points, style });
                }
                Interaction::Creating {
                    tool,
                    start,
                    current: _,
                    style,
                } => {
                    let current = pointer_world.unwrap_or(start);
                    self.interaction = Some(Interaction::Creating {
                        tool,
                        start,
                        current,
                        style,
                    });
                }
                Interaction::Lasso {
                    mut points,
                    additive,
                } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                        && points.last().is_none_or(|last| last.distance(point) > 2.0)
                    {
                        points.push(point);
                    }
                    self.interaction = Some(Interaction::Lasso { points, additive });
                }
                Interaction::Moving {
                    ids,
                    start,
                    base,
                    selection_bounds,
                } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                    {
                        self.document = base.clone();
                        let (delta, guides) =
                            self.snapped_delta(selection_bounds, point - start, &ids);
                        self.snap_guides = guides;
                        let page = self.document.active_page_mut();
                        for id in &ids {
                            translate_element(page, *id, delta, &ids);
                        }
                    }
                    self.interaction = Some(Interaction::Moving {
                        ids,
                        start,
                        base,
                        selection_bounds,
                    });
                }
                Interaction::Resizing {
                    ids,
                    base_bounds,
                    anchor,
                    base,
                } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                    {
                        self.document = base.clone();
                        let target = Rect {
                            min: anchor,
                            max: Point::new(
                                point.x.max(anchor.x + 4.0),
                                point.y.max(anchor.y + 4.0),
                            ),
                        };
                        let page = self.document.active_page_mut();
                        for id in &ids {
                            resize_element(page, *id, base_bounds, target);
                        }
                    }
                    self.interaction = Some(Interaction::Resizing {
                        ids,
                        base_bounds,
                        anchor,
                        base,
                    });
                }
                Interaction::Connecting { start, current: _ } => {
                    let current = pointer_world.unwrap_or(start.position);
                    self.interaction = Some(Interaction::Connecting { start, current });
                }
                Interaction::Laser { mut points } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                    {
                        points.push((point, Instant::now()));
                    }
                    self.laser_trail
                        .extend(points.iter().copied().rev().take(1));
                    self.interaction = Some(Interaction::Laser { points });
                }
                Interaction::Erasing { last, base } => {
                    if response.dragged()
                        && let Some(point) = pointer_world
                    {
                        let radius = 14.0 / self.document.active_page().camera.zoom;
                        let hit_ids: Vec<_> = self
                            .document
                            .active_page()
                            .elements
                            .iter()
                            .filter(|element| {
                                hit_by_swept_eraser(
                                    self.document.active_page(),
                                    element,
                                    last,
                                    point,
                                    radius,
                                )
                            })
                            .map(|element| element.id)
                            .collect();
                        self.document.delete_elements(&hit_ids);
                        self.interaction = Some(Interaction::Erasing { last: point, base });
                    }
                }
                Interaction::Panning { last } => {
                    if (pan_down || response.dragged())
                        && let Some(point) = pointer_screen
                    {
                        let current = egui_to_point(point);
                        let delta = current - last;
                        self.document.active_page_mut().camera.pan_screen(delta);
                        let dt = ui.input(|input| input.stable_dt.max(1.0 / 240.0) as f64);
                        self.pan_velocity = delta * (1.0 / dt);
                        self.interaction = Some(Interaction::Panning { last: current });
                    }
                }
            }
        }

        if primary_stopped {
            self.finish_interaction();
        }
        if response.clicked_by(PointerButton::Primary)
            && let Some(point) = pointer_world
        {
            self.handle_click(point, additive);
        }
        let middle_stopped = ui.input(|input| input.pointer.button_released(PointerButton::Middle));
        if middle_stopped {
            self.interaction = None;
        }

        if self.grid_enabled {
            let world_view = egui_to_world_rect(viewport, self.document.active_page().camera);
            self.paint_grid(&painter, viewport, world_view);
        }
        self.paint_elements(&painter, viewport);
        self.paint_interaction_preview(&painter, viewport);
        self.paint_selection(&painter, viewport);
        self.paint_snap_guides(&painter, viewport);
        self.paint_laser(&painter, viewport);
    }

    fn begin_interaction(&mut self, point: Point, additive: bool) {
        let page = self.document.active_page();
        match self.tool {
            Tool::Select => {
                let hit = hit_element(page, point, 6.0 / page.camera.zoom);
                if let Some(id) = hit {
                    if self.selected.len() == 1
                        && self.selected[0] == id
                        && let Some(bounds) = selection_bounds(page, &self.selected)
                    {
                        let handle = Point::new(bounds.max.x, bounds.max.y);
                        if point.distance(handle) <= 10.0 / page.camera.zoom {
                            self.interaction = Some(Interaction::Resizing {
                                ids: self.selected.clone(),
                                base_bounds: bounds,
                                anchor: bounds.min,
                                base: self.document.clone(),
                            });
                            return;
                        }
                    }
                    if additive && !self.selected.contains(&id) {
                        self.selected.push(id);
                    } else if !self.selected.contains(&id) {
                        self.selected.clear();
                        self.selected.push(id);
                    }
                    if let Some(bounds) = selection_bounds(page, &self.selected) {
                        self.interaction = Some(Interaction::Moving {
                            ids: self.selected.clone(),
                            start: point,
                            base: self.document.clone(),
                            selection_bounds: bounds,
                        });
                    }
                } else {
                    self.interaction = Some(Interaction::Lasso {
                        points: vec![point],
                        additive,
                    });
                    if !additive {
                        self.selected.clear();
                    }
                }
            }
            Tool::Lasso => {
                self.interaction = Some(Interaction::Lasso {
                    points: vec![point],
                    additive,
                });
            }
            Tool::Draw => {
                self.interaction = Some(Interaction::Drawing {
                    points: vec![point],
                    style: self.style.clone(),
                });
            }
            Tool::SmartArrow => {
                self.interaction = Some(Interaction::Connecting {
                    start: connector_endpoint(page, point),
                    current: point,
                });
            }
            Tool::Line | Tool::Shape(_) | Tool::Note | Tool::Frame => {
                self.interaction = Some(Interaction::Creating {
                    tool: self.tool,
                    start: point,
                    current: point,
                    style: self.style.clone(),
                });
            }
            Tool::Eraser => {
                self.interaction = Some(Interaction::Erasing {
                    last: point,
                    base: self.document.clone(),
                });
                let radius = 14.0 / page.camera.zoom;
                let ids: Vec<_> = page
                    .elements
                    .iter()
                    .filter(|element| page_hit_test(page, element, point, radius))
                    .map(|element| element.id)
                    .collect();
                self.document.delete_elements(&ids);
            }
            Tool::Laser => {
                let now = Instant::now();
                self.laser_trail.push((point, now));
                self.interaction = Some(Interaction::Laser {
                    points: vec![(point, now)],
                });
            }
        }
    }

    fn handle_click(&mut self, point: Point, additive: bool) {
        match self.tool {
            Tool::Select => {
                if let Some(id) = hit_element(
                    self.document.active_page(),
                    point,
                    6.0 / self.document.active_page().camera.zoom,
                ) {
                    if additive {
                        if let Some(index) =
                            self.selected.iter().position(|selected| *selected == id)
                        {
                            self.selected.remove(index);
                        } else {
                            self.selected.push(id);
                        }
                    } else {
                        self.selected = vec![id];
                    }
                } else if !additive {
                    self.selected.clear();
                }
            }
            Tool::Draw => {
                self.document.active_page_mut().elements.push(Element::new(
                    ElementKind::Stroke {
                        points: vec![point],
                    },
                    self.style.clone(),
                ));
                self.commit();
            }
            Tool::Note => {
                let element = Element::new(
                    ElementKind::Note {
                        bounds: Rect::from_points(
                            point,
                            Point::new(point.x + 160.0, point.y + 110.0),
                        ),
                        text: "Write a note…".to_owned(),
                    },
                    self.style.clone(),
                );
                self.selected = vec![element.id];
                self.document.active_page_mut().elements.push(element);
                self.commit();
            }
            Tool::Eraser => {
                let radius = 14.0 / self.document.active_page().camera.zoom;
                let ids: Vec<_> = self
                    .document
                    .active_page()
                    .elements
                    .iter()
                    .filter(|element| {
                        page_hit_test(self.document.active_page(), element, point, radius)
                    })
                    .map(|element| element.id)
                    .collect();
                if !ids.is_empty() {
                    self.document.delete_elements(&ids);
                    self.commit();
                }
            }
            Tool::Laser => self.laser_trail.push((point, Instant::now())),
            _ => {}
        }
    }

    fn finish_interaction(&mut self) {
        let interaction = self.interaction.take();
        match interaction {
            Some(Interaction::Drawing { points, style }) if !points.is_empty() => {
                self.document
                    .active_page_mut()
                    .elements
                    .push(Element::new(ElementKind::Stroke { points }, style));
                self.commit();
            }
            Some(Interaction::Creating {
                tool,
                start,
                current,
                style,
            }) => {
                let kind = match tool {
                    Tool::Line => Some(ElementKind::Line {
                        start,
                        end: current,
                    }),
                    Tool::Shape(shape_kind) => {
                        let bounds = Rect::from_points(start, current);
                        (bounds.width() >= 2.0 && bounds.height() >= 2.0).then_some(
                            ElementKind::Shape {
                                kind: shape_kind,
                                bounds,
                            },
                        )
                    }
                    Tool::Note => Some(ElementKind::Note {
                        bounds: Rect::from_points(start, current).let_min_size(160.0, 110.0),
                        text: "Write a note…".to_owned(),
                    }),
                    Tool::Frame => Some(ElementKind::Frame {
                        bounds: Rect::from_points(start, current).let_min_size(120.0, 90.0),
                        title: "Frame".to_owned(),
                    }),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let element = Element::new(kind, style);
                    self.selected = vec![element.id];
                    self.document.active_page_mut().elements.push(element);
                    self.commit();
                }
            }
            Some(Interaction::Lasso { points, additive }) => {
                if points.len() > 2 {
                    let mut matches: Vec<_> = self
                        .document
                        .active_page()
                        .elements
                        .iter()
                        .filter(|element| {
                            self.document
                                .active_page()
                                .resolved_element_bounds(element)
                                .is_some_and(|bounds| polygon_intersects_rect(&points, bounds))
                        })
                        .map(|element| element.id)
                        .collect();
                    if additive {
                        for id in matches.drain(..) {
                            if !self.selected.contains(&id) {
                                self.selected.push(id);
                            }
                        }
                    } else {
                        self.selected = matches;
                    }
                }
            }
            Some(Interaction::Connecting { start, current }) => {
                let end = connector_endpoint(self.document.active_page(), current);
                let element =
                    Element::new(ElementKind::Connector { start, end }, self.style.clone());
                self.selected = vec![element.id];
                self.document.active_page_mut().elements.push(element);
                self.commit();
            }
            Some(Interaction::Moving { base, .. })
            | Some(Interaction::Resizing { base, .. })
            | Some(Interaction::Erasing { base, .. }) => {
                if self.document != base {
                    self.commit();
                }
            }
            Some(Interaction::Laser { .. }) => {}
            Some(Interaction::Drawing { .. }) | Some(Interaction::Panning { .. }) | None => {}
        }
    }

    fn paint_grid(&self, painter: &egui::Painter, viewport: EguiRect, world: Rect) {
        let zoom = self.document.active_page().camera.zoom;
        let level = (32.0_f64 / (40.0 * zoom)).log2();
        let lower = level.floor();
        let blend = (level - lower).clamp(0.0, 1.0);
        let spacing_a = 40.0 * 2.0_f64.powf(lower);
        let spacing_b = spacing_a * 2.0;
        let camera = self.document.active_page().camera;
        let screen_viewport = egui_rect_to_rect(viewport);
        paint_grid_level(
            painter,
            world,
            camera,
            screen_viewport,
            spacing_a,
            1.0 - blend,
        );
        paint_grid_level(painter, world, camera, screen_viewport, spacing_b, blend);
    }

    fn paint_elements(&self, painter: &egui::Painter, viewport: EguiRect) {
        let page = self.document.active_page();
        let screen_viewport = egui_rect_to_rect(viewport);
        let visible_world = egui_to_world_rect(viewport, page.camera);
        for element in &page.elements {
            if !page
                .resolved_element_bounds(element)
                .is_some_and(|bounds| bounds.intersects(visible_world))
            {
                continue;
            }
            paint_element(painter, element, page, page.camera, screen_viewport);
        }
        let _ = visible_world;
    }

    fn paint_interaction_preview(&self, painter: &egui::Painter, viewport: EguiRect) {
        let Some(interaction) = &self.interaction else {
            return;
        };
        let page = self.document.active_page();
        let screen_viewport = egui_rect_to_rect(viewport);
        match interaction {
            Interaction::Drawing { points, style } => {
                paint_element(
                    painter,
                    &Element {
                        id: Uuid::nil(),
                        kind: ElementKind::Stroke {
                            points: points.clone(),
                        },
                        style: style.clone(),
                    },
                    page,
                    page.camera,
                    screen_viewport,
                );
            }
            Interaction::Creating {
                tool,
                start,
                current,
                style,
            } => {
                let preview = match tool {
                    Tool::Line => ElementKind::Line {
                        start: *start,
                        end: *current,
                    },
                    Tool::Shape(kind) => ElementKind::Shape {
                        kind: *kind,
                        bounds: Rect::from_points(*start, *current),
                    },
                    Tool::Note => ElementKind::Note {
                        bounds: Rect::from_points(*start, *current),
                        text: "Note".to_owned(),
                    },
                    Tool::Frame => ElementKind::Frame {
                        bounds: Rect::from_points(*start, *current),
                        title: "Frame".to_owned(),
                    },
                    _ => return,
                };
                paint_element(
                    painter,
                    &Element {
                        id: Uuid::nil(),
                        kind: preview,
                        style: style.clone(),
                    },
                    page,
                    page.camera,
                    screen_viewport,
                );
            }
            Interaction::Lasso { points, .. } => {
                let points: Vec<_> = points
                    .iter()
                    .map(|point| {
                        point_to_egui(page.camera.world_to_screen(*point, screen_viewport))
                    })
                    .collect();
                if points.len() > 1 {
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new(1.0, Color32::from_rgb(37, 99, 235)),
                    ));
                }
            }
            Interaction::Connecting { start, current } => {
                let start = page.camera.world_to_screen(start.position, screen_viewport);
                let end = page.camera.world_to_screen(*current, screen_viewport);
                painter.line_segment(
                    [point_to_egui(start), point_to_egui(end)],
                    Stroke::new(2.0, Color32::from_rgb(37, 99, 235)),
                );
            }
            _ => {}
        }
    }

    fn paint_selection(&self, painter: &egui::Painter, viewport: EguiRect) {
        let page = self.document.active_page();
        let bounds = selection_bounds(page, &self.selected);
        let Some(bounds) = bounds else {
            return;
        };
        let camera = page.camera;
        let screen_viewport = egui_rect_to_rect(viewport);
        let min = camera.world_to_screen(bounds.min, screen_viewport);
        let max = camera.world_to_screen(bounds.max, screen_viewport);
        let rect = EguiRect::from_min_max(point_to_egui(min), point_to_egui(max));
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(37, 99, 235)),
            StrokeKind::Outside,
        );
        if self.selected.len() == 1 {
            painter.rect_filled(
                EguiRect::from_center_size(rect.max, Vec2::splat(8.0)),
                1.0,
                Color32::WHITE,
            );
            painter.rect_stroke(
                EguiRect::from_center_size(rect.max, Vec2::splat(8.0)),
                1.0,
                Stroke::new(1.0, Color32::from_rgb(37, 99, 235)),
                StrokeKind::Middle,
            );
        }
        if self.selected.len() > 1 {
            for element in page
                .elements
                .iter()
                .filter(|element| self.selected.contains(&element.id))
            {
                if let Some(bounds) = element.bounds() {
                    let min = camera.world_to_screen(bounds.min, screen_viewport);
                    let max = camera.world_to_screen(bounds.max, screen_viewport);
                    painter.rect_stroke(
                        EguiRect::from_min_max(point_to_egui(min), point_to_egui(max)),
                        0.0,
                        Stroke::new(1.0, Color32::from_rgb(110, 150, 220)),
                        StrokeKind::Outside,
                    );
                }
            }
        }
    }

    fn paint_snap_guides(&self, painter: &egui::Painter, viewport: EguiRect) {
        let camera = self.document.active_page().camera;
        let screen_viewport = egui_rect_to_rect(viewport);
        for (start, end) in &self.snap_guides {
            painter.line_segment(
                [
                    point_to_egui(camera.world_to_screen(*start, screen_viewport)),
                    point_to_egui(camera.world_to_screen(*end, screen_viewport)),
                ],
                Stroke::new(1.0, Color32::from_rgb(20, 160, 155)),
            );
        }
    }

    fn paint_laser(&mut self, painter: &egui::Painter, viewport: EguiRect) {
        let now = Instant::now();
        self.laser_trail
            .retain(|(_, created)| now.duration_since(*created).as_secs_f32() < 1.2);
        if !self.laser_trail.is_empty() {
            painter.ctx().request_repaint();
        }
        let camera = self.document.active_page().camera;
        let screen_viewport = egui_rect_to_rect(viewport);
        for pair in self.laser_trail.windows(2) {
            let age = now.duration_since(pair[1].1).as_secs_f32();
            let alpha = ((1.2 - age) / 1.2 * 220.0) as u8;
            painter.line_segment(
                [
                    point_to_egui(camera.world_to_screen(pair[0].0, screen_viewport)),
                    point_to_egui(camera.world_to_screen(pair[1].0, screen_viewport)),
                ],
                Stroke::new(3.0, Color32::from_rgba_unmultiplied(255, 40, 70, alpha)),
            );
        }
    }

    fn canvas_color(&self) -> Color32 {
        if self.dark_theme {
            Color32::from_rgb(27, 30, 37)
        } else {
            Color32::from_rgb(250, 251, 253)
        }
    }

    fn activate_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.interaction = None;
        self.snap_guides.clear();
    }

    fn apply_style_to_selection(&mut self, style: ToolStyle) {
        for element in &mut self.document.active_page_mut().elements {
            if self.selected.contains(&element.id) {
                element.style = style.clone();
            }
        }
        self.commit();
    }

    fn animate_camera(&mut self, target: Camera) {
        let from = self.document.active_page().camera;
        self.camera_motion = Some(CameraMotion {
            from,
            to: target,
            elapsed: 0.0,
            duration: 0.24,
        });
    }

    fn tick_animations(&mut self, ctx: &egui::Context, dt: f32) {
        let dt = (dt as f64).clamp(0.0, 0.1);
        if let Some(mut motion) = self.camera_motion {
            motion.elapsed += dt;
            let t = (motion.elapsed / motion.duration).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            let camera = &mut self.document.active_page_mut().camera;
            camera.center = motion.from.center.lerp(motion.to.center, eased);
            camera.zoom = motion.from.zoom + (motion.to.zoom - motion.from.zoom) * eased;
            if t >= 1.0 {
                self.camera_motion = None;
            } else {
                self.camera_motion = Some(motion);
                ctx.request_repaint();
            }
        }
        if self.inertia_enabled
            && self.interaction.is_none()
            && self.pan_velocity.distance(Point::ZERO) > 8.0
        {
            self.document
                .active_page_mut()
                .camera
                .pan_screen(self.pan_velocity * dt);
            self.pan_velocity = self.pan_velocity * (-5.5 * dt).exp();
            ctx.request_repaint();
        } else if !self.inertia_enabled || self.interaction.is_some() {
            self.pan_velocity = Point::ZERO;
        }
    }

    fn snapped_delta(
        &self,
        selection_bounds: Rect,
        mut delta: Point,
        moving_ids: &[Uuid],
    ) -> (Point, Vec<(Point, Point)>) {
        if !self.snap_enabled {
            return (delta, Vec::new());
        }
        let page = self.document.active_page();
        let moved = Rect {
            min: selection_bounds.min + delta,
            max: selection_bounds.max + delta,
        };
        let threshold = 10.0 / page.camera.zoom;
        let mut best_x: Option<(f64, f64)> = None;
        let mut best_y: Option<(f64, f64)> = None;
        let moving_x = [moved.min.x, moved.center().x, moved.max.x];
        let moving_y = [moved.min.y, moved.center().y, moved.max.y];
        for target in page
            .elements
            .iter()
            .filter(|element| !moving_ids.contains(&element.id))
        {
            let Some(bounds) = page.resolved_element_bounds(target) else {
                continue;
            };
            let target_x = [bounds.min.x, bounds.center().x, bounds.max.x];
            let target_y = [bounds.min.y, bounds.center().y, bounds.max.y];
            for x in moving_x {
                for target in target_x {
                    let difference = target - x;
                    if difference.abs() <= threshold
                        && best_x.is_none_or(|(best, _)| difference.abs() < best)
                    {
                        best_x = Some((difference.abs(), difference));
                    }
                }
            }
            for y in moving_y {
                for target in target_y {
                    let difference = target - y;
                    if difference.abs() <= threshold
                        && best_y.is_none_or(|(best, _)| difference.abs() < best)
                    {
                        best_y = Some((difference.abs(), difference));
                    }
                }
            }
        }
        let mut guides = Vec::new();
        if let Some((_, adjustment)) = best_x {
            delta.x += adjustment;
            let x = moved.center().x + adjustment;
            guides.push((
                Point::new(x, moved.min.y - 10000.0),
                Point::new(x, moved.max.y + 10000.0),
            ));
        }
        if let Some((_, adjustment)) = best_y {
            delta.y += adjustment;
            let y = moved.center().y + adjustment;
            guides.push((
                Point::new(moved.min.x - 10000.0, y),
                Point::new(moved.max.x + 10000.0, y),
            ));
        }
        (delta, guides)
    }

    fn paint_diagnostics(&self, ctx: &egui::Context) {
        if !self.diagnostics {
            return;
        }
        let page = self.document.active_page();
        let viewport = ctx
            .input(|input| input.viewport().inner_rect)
            .unwrap_or(EguiRect::NOTHING);
        let view_bounds = egui_to_world_rect(viewport, page.camera);
        let visible = page
            .elements
            .iter()
            .filter(|element| {
                page.resolved_element_bounds(element)
                    .is_some_and(|bounds| bounds.intersects(view_bounds))
            })
            .count();
        egui::Area::new(Id::unique(egui::IdSalt::new("gpuii_diagnostics")))
            .anchor(Align2::LEFT_TOP, Vec2::new(12.0, 56.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(190))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(format!(
                            "FPS: {:.0}",
                            1.0 / self.frame_time_seconds.max(0.001)
                        ));
                        ui.label(format!("Frame: {:.1} ms", self.frame_time_seconds * 1000.0));
                        ui.label(format!("Objects: {}", page.elements.len()));
                        ui.label(format!("Visible: {visible}"));
                        ui.label(format!("Zoom: {:.2}x", page.camera.zoom));
                        ui.label(format!("Tool: {:?}", self.tool));
                        ui.label("Memory: unavailable");
                    });
            });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (command, undo, redo, delete, v, d, e, l, n, f, a) = ctx.input(|input| {
            let command = input.modifiers.command && !ctx.egui_wants_keyboard_input();
            (
                command,
                input.key_pressed(Key::Z),
                input.key_pressed(Key::Y)
                    || (input.modifiers.command
                        && input.key_pressed(Key::Z)
                        && input.modifiers.shift),
                input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace),
                input.key_pressed(Key::V),
                input.key_pressed(Key::D),
                input.key_pressed(Key::E),
                input.key_pressed(Key::L),
                input.key_pressed(Key::N),
                input.key_pressed(Key::F),
                input.key_pressed(Key::A),
            )
        });
        if command && undo {
            self.undo();
        } else if command && redo {
            self.redo();
        } else if command && ctx.input(|input| input.key_pressed(Key::S)) {
            self.save_project(false);
        } else if command && ctx.input(|input| input.key_pressed(Key::O)) {
            self.open_project();
        } else if !ctx.egui_wants_keyboard_input() {
            if delete {
                self.delete_selected();
            } else if v {
                self.activate_tool(Tool::Select);
            } else if a {
                self.activate_tool(Tool::SmartArrow);
            } else if d {
                self.activate_tool(Tool::Draw);
            } else if e {
                self.activate_tool(Tool::Eraser);
            } else if l {
                self.activate_tool(Tool::Line);
            } else if n {
                self.activate_tool(Tool::Note);
            } else if f {
                self.activate_tool(Tool::Frame);
            }
        }
    }

    fn undo(&mut self) {
        if let Some(document) = self.history.undo() {
            self.document = document;
            self.selected.clear();
            self.dirty = true;
            self.status = "Undid the last change".to_owned();
        }
    }

    fn redo(&mut self) {
        if let Some(document) = self.history.redo() {
            self.document = document;
            self.selected.clear();
            self.dirty = true;
            self.status = "Redid the last change".to_owned();
        }
    }

    fn commit(&mut self) {
        self.history.push(self.document.clone());
        self.dirty = true;
        self.status = "Changes recorded".to_owned();
    }

    fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let ids = std::mem::take(&mut self.selected);
        self.document.delete_elements(&ids);
        self.commit();
    }

    fn zoom_to_fit(&mut self) {
        let Some(bounds) = selection_bounds(self.document.active_page(), &self.selected)
            .or_else(|| self.document.active_page().content_bounds())
        else {
            self.animate_camera(Camera::default());
            return;
        };
        let zoom = (800.0 / (bounds.width() + 80.0))
            .min(600.0 / (bounds.height() + 80.0))
            .clamp(Camera::MIN_ZOOM, Camera::MAX_ZOOM);
        self.animate_camera(Camera {
            center: bounds.center(),
            zoom,
        });
    }

    fn new_project(&mut self) {
        if self.dirty
            && rfd::MessageDialog::new()
                .set_title("Discard unsaved changes?")
                .set_description("The current project has unsaved changes.")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return;
        }
        self.document = Document::default();
        self.history = History::new(self.document.clone(), 100);
        self.file_path = None;
        self.selected.clear();
        self.dirty = false;
        self.status = "Created a new project".to_owned();
    }

    fn open_project(&mut self) {
        if self.dirty
            && rfd::MessageDialog::new()
                .set_title("Discard unsaved changes?")
                .set_description("Discard this project and open another one?")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GPUII project", &["tldr"])
            .pick_file()
        else {
            return;
        };
        match fs::read_to_string(&path)
            .and_then(|text| crate::Document::from_json(&text).map_err(std::io::Error::other))
        {
            Ok(document) => {
                self.document = document;
                self.history = History::new(self.document.clone(), 100);
                self.file_path = Some(path);
                self.selected.clear();
                self.dirty = false;
                self.status = "Project opened".to_owned();
            }
            Err(error) => self.status = format!("Could not open project: {error}"),
        }
    }

    fn save_project(&mut self, choose_path: bool) {
        let path = if choose_path || self.file_path.is_none() {
            rfd::FileDialog::new()
                .add_filter("GPUII project", &["tldr"])
                .set_file_name(format!("{}.tldr", safe_filename(&self.document.title)))
                .save_file()
        } else {
            self.file_path.clone()
        };
        let Some(path) = path else {
            return;
        };
        let result = self
            .document
            .to_json()
            .map_err(std::io::Error::other)
            .and_then(|text| write_atomically(&path, text.as_bytes()));
        match result {
            Ok(()) => {
                self.file_path = Some(path);
                self.dirty = false;
                self.status = "Project saved".to_owned();
            }
            Err(error) => self.status = format!("Could not save project: {error}"),
        }
    }

    fn export_png_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(format!("{}.png", safe_filename(&self.document.title)))
            .save_file()
        else {
            return;
        };
        match export_png(self.document.active_page(), 1.0) {
            Ok(bytes) => match write_atomically(&path, &bytes) {
                Ok(()) => self.status = "PNG exported".to_owned(),
                Err(error) => self.status = format!("Could not write PNG: {error}"),
            },
            Err(error) => self.status = format!("Could not export PNG: {error}"),
        }
    }

    fn export_svg_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG image", &["svg"])
            .set_file_name(format!("{}.svg", safe_filename(&self.document.title)))
            .save_file()
        else {
            return;
        };
        let svg = export_svg(&self.document, self.document.active_page());
        match write_atomically(&path, svg.as_bytes()) {
            Ok(()) => self.status = "SVG exported".to_owned(),
            Err(error) => self.status = format!("Could not write SVG: {error}"),
        }
    }
}

fn paint_element(
    painter: &egui::Painter,
    element: &Element,
    page: &Page,
    camera: Camera,
    viewport: Rect,
) {
    let screen = |point| point_to_egui(camera.world_to_screen(point, viewport));
    let color = color32(element.style.color);
    let width = (element.style.width * camera.zoom).max(0.5) as f32;
    let stroke = Stroke::new(width, color);
    match &element.kind {
        ElementKind::Stroke { points } => {
            paint_polyline(painter, points, &screen, stroke, element.style.stroke);
        }
        ElementKind::Line { start, end } => {
            paint_polyline(
                painter,
                &[*start, *end],
                &screen,
                stroke,
                element.style.stroke,
            );
        }
        ElementKind::Connector { start, end } => {
            let start = page.resolved_position(*start);
            let end = page.resolved_position(*end);
            paint_polyline(
                painter,
                &[start, end],
                &screen,
                stroke,
                element.style.stroke,
            );
            let direction = end - start;
            let angle = direction.y.atan2(direction.x);
            let size = 12.0 / camera.zoom;
            let left = Point::new(
                end.x - size * (angle - 0.45).cos(),
                end.y - size * (angle - 0.45).sin(),
            );
            let right = Point::new(
                end.x - size * (angle + 0.45).cos(),
                end.y - size * (angle + 0.45).sin(),
            );
            painter.line_segment([screen(left), screen(end)], stroke);
            painter.line_segment([screen(right), screen(end)], stroke);
        }
        ElementKind::Shape { kind, bounds } => {
            let vertices = shape_vertices(*kind, *bounds);
            let fill = match element.style.fill {
                FillStyle::None => Color32::TRANSPARENT,
                FillStyle::Semi => Color32::from_rgba_unmultiplied(
                    element.style.color[0],
                    element.style.color[1],
                    element.style.color[2],
                    (element.style.color[3] as f32 * 0.2) as u8,
                ),
                FillStyle::Solid => color,
                FillStyle::Pattern => Color32::from_rgba_unmultiplied(
                    element.style.color[0],
                    element.style.color[1],
                    element.style.color[2],
                    (element.style.color[3] as f32 * 0.14) as u8,
                ),
            };
            if fill != Color32::TRANSPARENT {
                for triangle in triangulate_polygon(&vertices) {
                    painter.add(egui::Shape::convex_polygon(
                        triangle.map(screen).to_vec(),
                        fill,
                        Stroke::NONE,
                    ));
                }
            }
            if element.style.fill == FillStyle::Pattern {
                paint_pattern(painter, &vertices, &screen, camera.zoom, color);
            }
            paint_polyline(
                painter,
                &vertices_with_close(&vertices),
                &screen,
                stroke,
                element.style.stroke,
            );
            if *kind == ShapeKind::CheckBox {
                let tick = [
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
                paint_polyline(
                    painter,
                    &tick,
                    &screen,
                    Stroke::new(width.max(1.5), color),
                    StrokeStyle::Solid,
                );
            }
        }
        ElementKind::Note { bounds, text } => {
            let min = screen(bounds.min);
            let max = screen(bounds.max);
            let rect = EguiRect::from_min_max(min, max);
            painter.rect_filled(rect, 4.0, Color32::from_rgb(255, 241, 168));
            painter.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(215, 184, 62)),
                StrokeKind::Middle,
            );
            paint_note_text(painter, rect, text, element, camera.zoom);
        }
        ElementKind::Frame { bounds, title } => {
            let min = screen(bounds.min);
            let max = screen(bounds.max);
            let rect = EguiRect::from_min_max(min, max);
            painter.rect_stroke(
                rect,
                2.0,
                Stroke::new(1.5, Color32::from_rgb(130, 145, 166)),
                StrokeKind::Middle,
            );
            painter.text(
                rect.min + Vec2::new(6.0, 4.0),
                Align2::LEFT_TOP,
                title,
                FontId::proportional(14.0),
                color,
            );
        }
    }
}

fn paint_polyline(
    painter: &egui::Painter,
    points: &[Point],
    screen: &impl Fn(Point) -> Pos2,
    stroke: Stroke,
    style: StrokeStyle,
) {
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        painter.circle_filled(screen(points[0]), stroke.width / 2.0, stroke.color);
        return;
    }
    for pair in points.windows(2) {
        let start = screen(pair[0]);
        let end = screen(pair[1]);
        if style == StrokeStyle::Dashed || style == StrokeStyle::Dotted {
            let length = start.distance(end);
            let dash = if style == StrokeStyle::Dotted {
                1.0
            } else {
                10.0
            };
            let gap = if style == StrokeStyle::Dotted {
                6.0
            } else {
                7.0
            };
            let direction = (end - start) / length.max(1.0);
            let mut offset = 0.0;
            while offset < length {
                let finish = (offset + dash).min(length);
                painter.line_segment(
                    [start + direction * offset, start + direction * finish],
                    stroke,
                );
                offset += dash + gap;
            }
        } else {
            painter.line_segment([start, end], stroke);
        }
    }
}

fn paint_pattern(
    painter: &egui::Painter,
    vertices: &[Point],
    screen: &impl Fn(Point) -> Pos2,
    zoom: f64,
    color: Color32,
) {
    let Some(bounds) = vertices
        .iter()
        .copied()
        .map(|point| Rect {
            min: point,
            max: point,
        })
        .reduce(Rect::union)
    else {
        return;
    };
    let spacing = (10.0 / zoom.max(0.08))
        .max(bounds.width() / 100.0)
        .max(bounds.height() / 100.0);
    let mut offset = bounds.min.x - bounds.height();
    while offset < bounds.max.x {
        let mut previous = None;
        let steps = ((bounds.height() / (spacing * 0.5)).ceil() as usize).clamp(1, 200);
        for step in 0..=steps {
            let y = bounds.min.y + bounds.height() * step as f64 / steps as f64;
            let point = Point::new(offset + y - bounds.min.y, y);
            let inside = point_in_polygon(point, vertices);
            if inside && previous.is_none() {
                previous = Some(point);
            } else if !inside && let Some(start) = previous.take() {
                painter.line_segment([screen(start), screen(point)], Stroke::new(0.7, color));
            }
        }
        if let Some(start) = previous {
            painter.line_segment(
                [
                    screen(start),
                    screen(Point::new(offset + bounds.height(), bounds.max.y)),
                ],
                Stroke::new(0.7, color),
            );
        }
        offset += spacing;
    }
}

fn vertices_with_close(vertices: &[Point]) -> Vec<Point> {
    let mut result = vertices.to_vec();
    if let Some(first) = result.first().copied() {
        result.push(first);
    }
    result
}

fn paint_note_text(
    painter: &egui::Painter,
    rect: EguiRect,
    text: &str,
    element: &Element,
    zoom: f64,
) {
    let family = match element.style.font {
        FontFamily::Sans | FontFamily::Script => egui::FontFamily::Proportional,
        FontFamily::Mono => egui::FontFamily::Monospace,
    };
    let font = FontId::new((14.0 * zoom.max(0.55)) as f32, family);
    let max_width = (rect.width() - 20.0).max(1.0);
    let galley = painter.layout(
        text.to_owned(),
        font,
        Color32::from_rgb(55, 49, 32),
        max_width,
    );
    let x = match element.style.text_align {
        TextAlign::Left => rect.left() + 10.0,
        TextAlign::Center => rect.center().x - galley.size().x / 2.0,
        TextAlign::Right => rect.right() - galley.size().x - 10.0,
    };
    painter.with_clip_rect(rect.shrink(6.0)).galley(
        Pos2::new(x, rect.top() + 10.0),
        galley,
        Color32::from_rgb(55, 49, 32),
    );
}

fn selection_bounds(page: &Page, ids: &[Uuid]) -> Option<Rect> {
    page.elements
        .iter()
        .filter(|element| ids.contains(&element.id))
        .filter_map(|element| page.resolved_element_bounds(element))
        .reduce(Rect::union)
}

fn hit_element(page: &Page, point: Point, tolerance: f64) -> Option<Uuid> {
    page.elements
        .iter()
        .rev()
        .find(|element| page_hit_test(page, element, point, tolerance))
        .map(|element| element.id)
}

fn page_hit_test(page: &Page, element: &Element, point: Point, tolerance: f64) -> bool {
    match &element.kind {
        ElementKind::Connector { start, end } => {
            distance_to_segment(
                point,
                page.resolved_position(*start),
                page.resolved_position(*end),
            ) <= tolerance + element.style.width / 2.0
        }
        _ => element.hit_test(point, tolerance),
    }
}

fn connector_endpoint(page: &Page, point: Point) -> ConnectorEndpoint {
    let attachment = hit_element(page, point, 10.0 / page.camera.zoom).and_then(|element_id| {
        let bounds = page
            .elements
            .iter()
            .find(|element| element.id == element_id)
            .and_then(Element::bounds)?;
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return None;
        }
        Some(Attachment {
            element_id,
            local: Point::new(
                ((point.x - bounds.min.x) / bounds.width()).clamp(0.0, 1.0),
                ((point.y - bounds.min.y) / bounds.height()).clamp(0.0, 1.0),
            ),
        })
    });
    ConnectorEndpoint {
        position: point,
        attachment,
    }
}

fn polygon_intersects_rect(polygon: &[Point], rect: Rect) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let corners = [
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        rect.max,
        Point::new(rect.min.x, rect.max.y),
    ];
    if corners
        .iter()
        .any(|corner| point_in_polygon(*corner, polygon))
        || polygon.iter().any(|point| rect.contains(*point))
    {
        return true;
    }
    let rect_edges = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[0]),
    ];
    (0..polygon.len()).any(|index| {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];
        rect_edges
            .iter()
            .any(|(a, b)| segments_intersect(start, end, *a, *b))
    })
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    fn orientation(a: Point, b: Point, c: Point) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    fn on_segment(a: Point, b: Point, point: Point) -> bool {
        point.x >= a.x.min(b.x)
            && point.x <= a.x.max(b.x)
            && point.y >= a.y.min(b.y)
            && point.y <= a.y.max(b.y)
    }
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    if ((ab_c > 0.0 && ab_d < 0.0) || (ab_c < 0.0 && ab_d > 0.0))
        && ((cd_a > 0.0 && cd_b < 0.0) || (cd_a < 0.0 && cd_b > 0.0))
    {
        return true;
    }
    (ab_c.abs() <= f64::EPSILON && on_segment(a, b, c))
        || (ab_d.abs() <= f64::EPSILON && on_segment(a, b, d))
        || (cd_a.abs() <= f64::EPSILON && on_segment(c, d, a))
        || (cd_b.abs() <= f64::EPSILON && on_segment(c, d, b))
}

fn paint_grid_level(
    painter: &egui::Painter,
    visible: Rect,
    camera: Camera,
    viewport: Rect,
    spacing: f64,
    opacity: f64,
) {
    if opacity < 0.015 || spacing <= 0.0 {
        return;
    }
    let first_x = (visible.min.x / spacing).floor() as i64;
    let last_x = (visible.max.x / spacing).ceil() as i64;
    let first_y = (visible.min.y / spacing).floor() as i64;
    let last_y = (visible.max.y / spacing).ceil() as i64;
    let stride_x = ((last_x - first_x + 1) as f64 / 180.0).ceil().max(1.0) as i64;
    let stride_y = ((last_y - first_y + 1) as f64 / 180.0).ceil().max(1.0) as i64;
    let alpha = (opacity.clamp(0.0, 1.0) * 90.0) as u8;
    for x in (first_x..=last_x).step_by(stride_x as usize) {
        for y in (first_y..=last_y).step_by(stride_y as usize) {
            let world = Point::new(x as f64 * spacing, y as f64 * spacing);
            let screen = point_to_egui(camera.world_to_screen(world, viewport));
            painter.circle_filled(
                screen,
                if spacing * camera.zoom >= 40.0 {
                    1.5
                } else {
                    1.0
                },
                Color32::from_rgba_unmultiplied(110, 120, 140, alpha),
            );
        }
    }
}

fn shape_label(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Rectangle => "Rectangle",
        ShapeKind::Ellipse => "Ellipse",
        ShapeKind::Triangle => "Triangle",
        ShapeKind::Diamond => "Diamond",
        ShapeKind::Pentagon => "Pentagon",
        ShapeKind::Hexagon => "Hexagon",
        ShapeKind::Octagon => "Octagon",
        ShapeKind::Star => "Star",
        ShapeKind::Cloud => "Cloud",
        ShapeKind::Trapezoid => "Trapezoid",
        ShapeKind::ArrowUp => "Arrow up",
        ShapeKind::ArrowDown => "Arrow down",
        ShapeKind::ArrowLeft => "Arrow left",
        ShapeKind::ArrowRight => "Arrow right",
        ShapeKind::XBox => "X box",
        ShapeKind::CheckBox => "Checkbox",
    }
}

fn fill_label(fill: FillStyle) -> &'static str {
    match fill {
        FillStyle::None => "None",
        FillStyle::Semi => "Semi",
        FillStyle::Solid => "Solid",
        FillStyle::Pattern => "Pattern",
    }
}

fn stroke_label(stroke: StrokeStyle) -> &'static str {
    match stroke {
        StrokeStyle::Draw => "Draw",
        StrokeStyle::Solid => "Solid",
        StrokeStyle::Dashed => "Dashed",
        StrokeStyle::Dotted => "Dotted",
    }
}

fn font_label(font: FontFamily) -> &'static str {
    match font {
        FontFamily::Sans => "Sans",
        FontFamily::Mono => "Mono",
        FontFamily::Script => "Script",
    }
}

fn text_align_label(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "Left",
        TextAlign::Center => "Center",
        TextAlign::Right => "Right",
    }
}

fn egui_rect_to_rect(rect: EguiRect) -> Rect {
    Rect::from_points(egui_to_point(rect.min), egui_to_point(rect.max))
}

fn translate_element(page: &mut crate::Page, id: Uuid, delta: Point, moving_ids: &[Uuid]) {
    if let Some(element) = page.elements.iter_mut().find(|element| element.id == id) {
        match &mut element.kind {
            ElementKind::Stroke { points } => {
                for point in points {
                    *point = *point + delta;
                }
            }
            ElementKind::Line { start, end } => {
                *start = *start + delta;
                *end = *end + delta;
            }
            ElementKind::Shape { bounds, .. }
            | ElementKind::Note { bounds, .. }
            | ElementKind::Frame { bounds, .. } => {
                bounds.min = bounds.min + delta;
                bounds.max = bounds.max + delta;
            }
            ElementKind::Connector { start, end } => {
                for endpoint in [start, end] {
                    let target_moves = endpoint
                        .attachment
                        .is_some_and(|attachment| moving_ids.contains(&attachment.element_id));
                    if target_moves {
                        endpoint.position = endpoint.position + delta;
                    } else {
                        endpoint.position = endpoint.position + delta;
                        endpoint.attachment = None;
                    }
                }
            }
        }
    }
}

fn resize_element(page: &mut Page, id: Uuid, source: Rect, target: Rect) {
    let map_point = |point: Point| {
        Point::new(
            target.min.x
                + (point.x - source.min.x) / source.width().max(f64::EPSILON) * target.width(),
            target.min.y
                + (point.y - source.min.y) / source.height().max(f64::EPSILON) * target.height(),
        )
    };
    if let Some(element) = page.elements.iter_mut().find(|element| element.id == id) {
        match &mut element.kind {
            ElementKind::Stroke { points } => {
                for point in points {
                    *point = map_point(*point);
                }
            }
            ElementKind::Line { start, end } => {
                *start = map_point(*start);
                *end = map_point(*end);
            }
            ElementKind::Shape { bounds, .. }
            | ElementKind::Note { bounds, .. }
            | ElementKind::Frame { bounds, .. } => {
                bounds.min = map_point(bounds.min);
                bounds.max = map_point(bounds.max);
            }
            ElementKind::Connector { start, end } => {
                for endpoint in [start, end] {
                    endpoint.position = map_point(endpoint.position);
                    endpoint.attachment = None;
                }
            }
        }
    }
}

fn egui_to_world_rect(rect: EguiRect, camera: Camera) -> Rect {
    let viewport = Rect::from_points(egui_to_point(rect.min), egui_to_point(rect.max));
    Rect::from_points(
        camera.screen_to_world(viewport.min, viewport),
        camera.screen_to_world(viewport.max, viewport),
    )
}

fn hit_by_swept_eraser(
    page: &Page,
    element: &Element,
    start: Point,
    end: Point,
    radius: f64,
) -> bool {
    let distance = start.distance(end);
    let spacing = (radius.max(0.5) / 2.0).max(f64::EPSILON);
    let samples = (distance / spacing).ceil().max(1.0) as usize;
    (0..=samples).any(|index| {
        page_hit_test(
            page,
            element,
            start.lerp(end, index as f64 / samples as f64),
            radius,
        )
    })
}

fn egui_to_point(point: Pos2) -> Point {
    Point::new(point.x as f64, point.y as f64)
}

fn point_to_egui(point: Point) -> Pos2 {
    Pos2::new(point.x as f32, point.y as f32)
}

fn color32(color: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3])
}

fn element_name(element: &Element) -> &'static str {
    match element.kind {
        ElementKind::Stroke { .. } => "Freehand stroke",
        ElementKind::Line { .. } => "Line",
        ElementKind::Shape { kind, .. } => shape_label(kind),
        ElementKind::Note { .. } => "Sticky note",
        ElementKind::Connector { .. } => "Smart arrow",
        ElementKind::Frame { .. } => "Frame",
    }
}

fn safe_filename(name: &str) -> String {
    let filename = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if filename.is_empty() {
        "gpuii-project".to_owned()
    } else {
        filename
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gpuii-project");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl Rect {
    fn let_min_size(self, min_width: f64, min_height: f64) -> Self {
        Self {
            max: Point::new(
                self.max.x.max(self.min.x + min_width),
                self.max.y.max(self.min.y + min_height),
            ),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ElementKind;

    #[test]
    fn filename_sanitization_produces_a_fallback_for_empty_titles() {
        assert_eq!(safe_filename("***"), "gpuii-project");
        assert_eq!(safe_filename("My Board 1"), "My-Board-1");
    }

    #[test]
    fn moving_an_element_translates_all_of_its_geometry() {
        let mut page = crate::Page::new("Test");
        let element = Element::new(
            ElementKind::Line {
                start: Point::new(1.0, 2.0),
                end: Point::new(5.0, 6.0),
            },
            ToolStyle::default(),
        );
        let id = element.id;
        page.elements.push(element);
        translate_element(&mut page, id, Point::new(10.0, -3.0), &[id]);
        assert_eq!(
            page.elements[0].kind,
            ElementKind::Line {
                start: Point::new(11.0, -1.0),
                end: Point::new(15.0, 3.0),
            }
        );
    }

    #[test]
    fn swept_eraser_detects_thin_objects_between_pointer_samples() {
        let element = Element::new(
            ElementKind::Line {
                start: Point::new(50.0, -2.0),
                end: Point::new(50.0, 2.0),
            },
            ToolStyle {
                width: 1.0,
                ..ToolStyle::default()
            },
        );
        assert!(hit_by_swept_eraser(
            &crate::Page::new("Test"),
            &element,
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            3.0
        ));
    }

    #[test]
    fn attached_connector_hit_testing_uses_its_resolved_position() {
        let mut page = crate::Page::new("Attached");
        let shape = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Rectangle,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 100.0)),
            },
            ToolStyle::default(),
        );
        let shape_id = shape.id;
        page.elements.push(shape);
        let connector = Element::new(
            ElementKind::Connector {
                start: ConnectorEndpoint {
                    position: Point::new(50.0, 50.0),
                    attachment: Some(Attachment {
                        element_id: shape_id,
                        local: Point::new(0.5, 0.5),
                    }),
                },
                end: ConnectorEndpoint {
                    position: Point::new(200.0, 50.0),
                    attachment: None,
                },
            },
            ToolStyle::default(),
        );
        let connector_id = connector.id;
        page.elements.push(connector);
        translate_element(&mut page, shape_id, Point::new(100.0, 0.0), &[shape_id]);
        assert_eq!(
            hit_element(&page, Point::new(150.0, 50.0), 2.0),
            Some(connector_id)
        );
    }

    #[test]
    fn lasso_intersection_selects_objects_crossed_by_the_polygon() {
        let lasso = [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 20.0),
            Point::new(0.0, 20.0),
        ];
        assert!(polygon_intersects_rect(
            &lasso,
            Rect::from_points(Point::new(18.0, 8.0), Point::new(28.0, 12.0))
        ));
        assert!(!polygon_intersects_rect(
            &lasso,
            Rect::from_points(Point::new(25.0, 25.0), Point::new(30.0, 30.0))
        ));
    }

    #[test]
    fn resizing_a_target_preserves_connector_attachment() {
        let mut page = crate::Page::new("Resize");
        let shape = Element::new(
            ElementKind::Shape {
                kind: ShapeKind::Rectangle,
                bounds: Rect::from_points(Point::ZERO, Point::new(100.0, 100.0)),
            },
            ToolStyle::default(),
        );
        let id = shape.id;
        page.elements.push(shape);
        let connector = Element::new(
            ElementKind::Connector {
                start: ConnectorEndpoint {
                    position: Point::new(50.0, 50.0),
                    attachment: Some(Attachment {
                        element_id: id,
                        local: Point::new(0.5, 0.5),
                    }),
                },
                end: ConnectorEndpoint {
                    position: Point::new(150.0, 50.0),
                    attachment: None,
                },
            },
            ToolStyle::default(),
        );
        let endpoint = match connector.kind {
            ElementKind::Connector { start, .. } => start,
            _ => unreachable!(),
        };
        page.elements.push(connector);

        resize_element(
            &mut page,
            id,
            Rect::from_points(Point::ZERO, Point::new(100.0, 100.0)),
            Rect::from_points(Point::ZERO, Point::new(200.0, 150.0)),
        );
        assert_eq!(page.resolved_position(endpoint), Point::new(100.0, 75.0));
    }
}
