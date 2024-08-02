use std::{
    cell::RefCell,
    f32::consts::PI,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use egui_extras::{Column, TableBuilder};
use egui_inspect::{background_task::BackgroundTask, EguiInspect};
use egui_inspect::{
    background_task::Task,
    egui::{
        self, vec2, CentralPanel, Color32, ColorImage, Context, ScrollArea, Slider, Stroke,
        TextureHandle, TextureOptions, Vec2, Window,
    },
};
use egui_plot::{Plot, PlotImage, PlotPoint, PlotUi, Polygon};
use image::{ColorType, ImageResult, RgbaImage};
use imageproc::geometric_transformations::{self, rotate_about_center};
use iter_tools::Itertools;

use rayon::iter::ParallelBridge;
use rayon::prelude::*;

thread_local! {
    static SHARED_STATE: RefCell<SharedState> = Default::default();
}

struct SharedState {
    extents: Extents,
    separator_color: Color32,
    drag_enabled: bool,
    delta_x: f64,
    delta_y: f64,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            extents: Default::default(),
            separator_color: Color32::RED,
            drag_enabled: Default::default(),
            delta_x: 0.005,
            delta_y: 0.005,
        }
    }
}

#[derive(Clone)]
struct VertSep {
    x: f64,
}

impl VertSep {
    fn translate(&mut self, s: Vec2) {
        self.x += s.x as f64;
    }
    fn in_bounds(&mut self, delta_x: f64, extents: &Extents, pointer: PlotPoint) -> bool {
        self.x - delta_x < pointer.x
            && pointer.x < self.x + delta_x
            && extents.ymin < pointer.y
            && pointer.y < extents.ymax
    }
    fn plot_inspect(&mut self, pui: &mut PlotUi) {
        SHARED_STATE.with_borrow_mut(|ss| {
            pui.polygon(
                Polygon::new(vec![
                    [self.x - ss.delta_x, ss.extents.ymin - ss.delta_y],
                    [self.x + ss.delta_x, ss.extents.ymin - ss.delta_y],
                    [self.x + ss.delta_x, ss.extents.ymax + ss.delta_y],
                    [self.x - ss.delta_x, ss.extents.ymax + ss.delta_y],
                ])
                .fill_color(ss.separator_color)
                .stroke(Stroke::NONE),
            );

            if let Some(pointer) = pui.pointer_coordinate() {
                if self.in_bounds(ss.delta_x, &ss.extents, pointer) && ss.drag_enabled {
                    ss.drag_enabled = false;
                    self.translate(pui.pointer_coordinate_drag_delta());
                }
            }
        });
    }
}

#[derive(Clone)]
struct HorizSep {
    y: f64,
}

impl HorizSep {
    fn translate(&mut self, s: Vec2) {
        self.y += s.y as f64;
    }
    fn in_bounds(&mut self, delta_y: f64, extents: &Extents, pointer: PlotPoint) -> bool {
        self.y - delta_y < pointer.y
            && pointer.y < self.y + delta_y
            && extents.xmin < pointer.x
            && pointer.x < extents.xmax
    }
    fn plot_inspect(&mut self, pui: &mut PlotUi) {
        SHARED_STATE.with_borrow_mut(|ss| {
            pui.polygon(
                Polygon::new(vec![
                    [ss.extents.xmin - ss.delta_x, self.y - ss.delta_y],
                    [ss.extents.xmin - ss.delta_x, self.y + ss.delta_y],
                    [ss.extents.xmax + ss.delta_x, self.y + ss.delta_y],
                    [ss.extents.xmax + ss.delta_x, self.y - ss.delta_y],
                ])
                .fill_color(ss.separator_color)
                .stroke(Stroke::NONE),
            );

            if let Some(pointer) = pui.pointer_coordinate() {
                if self.in_bounds(ss.delta_y, &ss.extents, pointer) && ss.drag_enabled {
                    ss.drag_enabled = false;
                    self.translate(pui.pointer_coordinate_drag_delta());
                }
            }
        });
    }
}

#[derive(Clone)]
struct Grid {
    horizontals: Vec<HorizSep>,
    verticals: Vec<VertSep>,
}

impl Default for Grid {
    fn default() -> Self {
        let mut horizontals = vec![];
        let mut verticals = vec![];
        for y in [0.8, 0.9] {
            horizontals.push(HorizSep { y });
        }
        for x in [0.1, 0.2] {
            verticals.push(VertSep { x });
        }
        Self {
            horizontals,
            verticals,
        }
    }
}

impl Grid {
    fn sort_horiz(&mut self) {
        self.horizontals
            .sort_by(|h1, h2| h1.y.partial_cmp(&h2.y).unwrap());
    }
    fn sort_vert(&mut self) {
        self.verticals
            .sort_by(|v1, v2| v1.x.partial_cmp(&v2.x).unwrap());
    }
    fn plot_inspect(&mut self, pui: &mut PlotUi) {
        for horiz in self.horizontals.iter_mut() {
            horiz.plot_inspect(pui);
        }
        for vert in self.verticals.iter_mut() {
            vert.plot_inspect(pui);
        }
    }
}

#[derive(Default)]
struct Extents {
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
}

struct TableEdit {
    items: Vec<Vec<String>>,
}

impl TableEdit {
    fn csv(&self) -> String {
        self.items
            .iter()
            .map(|row| row.iter().map(|item| format!("\"{item}\"")).join(", "))
            .join("\n")
    }
}

impl EguiInspect for TableEdit {
    fn inspect(&self, _label: &str, _ui: &mut egui::Ui) {}

    fn inspect_mut(&mut self, _label: &str, ui: &mut egui::Ui) {
        Window::new("Table").min_width(500.0).show(ui.ctx(), |ui| {
            ScrollArea::both().show(ui, |ui| {
                let mut builder = TableBuilder::new(ui);
                let nrows = self.items.len();
                let ncols = self.items[0].len();

                for _ in 0..ncols {
                    builder = builder.column(Column::auto().resizable(true));
                }

                builder.body(|body| {
                    body.rows(30.0, nrows, |mut row| {
                        let i = row.index();
                        for j in 0..ncols {
                            row.col(|ui| {
                                self.items[i][j].inspect_mut(format!("{i},{j}").as_str(), ui);
                            });
                        }
                    });
                });
            });
            if ui.button("Export csv").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_directory(".").save_file() {
                    fs::write(path, self.csv()).unwrap();
                }
            }
        });
    }
}

#[allow(dead_code)]
#[derive(EguiInspect, PartialEq)]
enum OCROptions {
    Tesseract,
    Cuneiform,
}

impl OCROptions {
    fn cmd_template(&self) -> String {
        match self {
            OCROptions::Tesseract => "tesseract -l eng %img_in% %txt_out%".to_string(),
            OCROptions::Cuneiform => {
                "cuneiform -l eng -f text -o %txt_out%.txt %img_in%".to_string()
            }
        }
    }
}

struct TableImage {
    base: ColorImage,
    rotated: ColorImage,
    theta: f32,
    theta_old: f32,
    base_tex: Option<TextureHandle>,
    rot_tex: Option<TextureHandle>,
}

impl TableImage {
    #[allow(dead_code)]
    fn base_tex(&mut self, ctx: &Context) -> &TextureHandle {
        self.base_tex.get_or_insert_with(|| {
            ctx.load_texture("test_img", self.base.clone(), TextureOptions::LINEAR)
        })
    }
    fn rot_tex(&mut self, ctx: &Context) -> &TextureHandle {
        self.rot_tex.get_or_insert_with(|| {
            ctx.load_texture("test_img", self.rotated.clone(), TextureOptions::LINEAR)
        })
    }
    fn inspect_rotation(&mut self, ui: &mut egui::Ui) {
        ui.label("Rotation");
        ui.add(Slider::new(&mut self.theta, -PI / 16.0..=PI / 16.0));
        if self.theta != self.theta_old {
            let base = RgbaImage::from_fn(
                self.base.width() as u32,
                self.base.height() as u32,
                |i, j| {
                    let color = self.base.pixels[(j as usize) * self.base.width() + (i as usize)];
                    image::Rgba([color.r(), color.g(), color.b(), color.a()])
                },
            );
            let rotated_image = rotate_about_center(
                &base,
                self.theta,
                geometric_transformations::Interpolation::Bicubic,
                image::Rgba([255, 0, 0, 0]),
            );
            self.rotated = img_to_cim(rotated_image.into());
            self.theta_old = self.theta;
            self.rot_tex = None;
        }
    }
}

static HELP_STR: &str = "Key bindings for image preview (egui plot).

Left click drag: [on separator] move separator, [off separator] pan preview.

Ctrl + Mouse wheel: zoom.

Mouse wheel drag: translate grid.

Double click left mouse button: reset zoom.

Right click: place new horizontal separator.

Right click + Shift: place new vertical separator.


When finished annotating, hit extract to generate table.";

pub struct TableGrid {
    image_path: Option<PathBuf>,
    image: Option<TableImage>,
    grid: Grid,
    cmd_template: String,
    process_task: BackgroundTask<BackgroundOCR>,
}

impl Default for TableGrid {
    fn default() -> Self {
        Self {
            image_path: Default::default(),
            image: Default::default(),
            grid: Default::default(),
            cmd_template: OCROptions::Tesseract.cmd_template(),
            process_task: Default::default(),
        }
    }
}

fn save_img(buff: &[u8], size: [usize; 2], fpath: impl AsRef<Path>) -> ImageResult<()> {
    image::save_buffer(
        fpath,
        buff,
        size[0] as u32,
        size[1] as u32,
        ColorType::Rgba8,
    )
}

fn clip(x: f64) -> f64 {
    x.max(0.0).min(1.0)
}

fn img_to_cim(image: image::DynamicImage) -> ColorImage {
    let size = [image.width() as _, image.height() as _];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
}

#[derive(Clone, Copy, EguiInspect, Debug)]
#[inspect(collapsible)]
struct CleaningOptions {
    trim_whitespace: bool,
    trim_single_quote: bool,
    trim_double_quote: bool,
    no_newlines: bool,
}

impl Default for CleaningOptions {
    fn default() -> Self {
        Self {
            trim_whitespace: true,
            trim_single_quote: true,
            trim_double_quote: true,
            no_newlines: true,
        }
    }
}

impl TableGrid {
    fn load_image(&mut self) {
        let img_path = self.image_path.take().unwrap();
        let image = image::ImageReader::open(img_path)
            .unwrap()
            .decode()
            .unwrap();
        let cim = img_to_cim(image);

        self.image = Some(TableImage {
            base: cim.clone(),
            rotated: cim,
            theta: 0.0,
            theta_old: 0.0,
            base_tex: None,
            rot_tex: None,
        });
    }
    fn update_extents(&self) {
        SHARED_STATE.with_borrow_mut(|ss| {
            ss.extents = Extents {
                xmin: self.grid.verticals.first().unwrap().x,
                xmax: self.grid.verticals.last().unwrap().x,
                ymin: self.grid.horizontals.first().unwrap().y,
                ymax: self.grid.horizontals.last().unwrap().y,
            };
        })
    }
}

fn crop_buffer(cim: &ColorImage, x1: f64, x2: f64, y1: f64, y2: f64) -> (Vec<u8>, [usize; 2]) {
    let orig_size = cim.size;
    let i0 = (clip(x1.min(x2)) * (orig_size[0] as f64)) as usize;
    let i1 = (clip(x1.max(x2)) * (orig_size[0] as f64)) as usize;
    let j0 = (clip(1.0 - y1.max(y2)) * (orig_size[1] as f64)) as usize;
    let j1 = (clip(1.0 - y1.min(y2)) * (orig_size[1] as f64)) as usize;
    let size = [i1 - i0, j1 - j0];
    let mut out = vec![];
    for j in j0..j1 {
        for i in i0..i1 {
            out.push(cim.pixels[j * orig_size[0] + i].r());
            out.push(cim.pixels[j * orig_size[0] + i].g());
            out.push(cim.pixels[j * orig_size[0] + i].b());
            out.push(cim.pixels[j * orig_size[0] + i].a());
        }
    }
    (out, size)
}

#[derive(EguiInspect, Default)]
struct BackgroundOCR {
    #[inspect(hide)]
    grid: Grid,
    #[inspect(hide)]
    cim: ColorImage,
    #[inspect(hide)]
    cmd_template: String,
    #[inspect(hide)]
    ready: bool,
    #[inspect(hide)]
    n_tasks: usize,
    cleaning_options: CleaningOptions,
}

impl Task for BackgroundOCR {
    type Return = TableEdit;

    fn exec_with_expected_steps(&self) -> Option<usize> {
        self.ready.then_some(self.n_tasks)
    }

    fn on_exec(&mut self, progress: egui_inspect::background_task::Progress) -> Self::Return {
        let mut items = vec![
            vec![String::new(); self.grid.verticals.len() - 1];
            self.grid.horizontals.len() - 1
        ];
        let co = self.cleaning_options;

        let out_flat: Vec<io::Result<_>> = self
            .grid
            .horizontals
            .windows(2)
            .rev()
            .enumerate()
            .cartesian_product(self.grid.verticals.windows(2).enumerate())
            .par_bridge()
            .map(|((i, hw), (j, vw))| {
                let img_path = format!("/tmp/ocr_crop_{i}_{j}.png");
                let txt_path = format!("/tmp/ocr_out_{i}_{j}");

                let (buff, size) = crop_buffer(&self.cim, vw[0].x, vw[1].x, hw[0].y, hw[1].y);
                save_img(buff.as_slice(), size, Path::new(img_path.as_str())).unwrap();

                let cmd = self
                    .cmd_template
                    .as_str()
                    .replace("%img_in%", img_path.as_str())
                    .replace("%txt_out%", txt_path.as_str());

                let mut cmd_iter = cmd.split_whitespace();

                let prog = cmd_iter.next().unwrap();
                let mut handle = Command::new(prog)
                    .args(cmd_iter)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;

                handle.wait()?;

                let txt_path = format!("{txt_path}.txt");
                let mut ocr_out = fs::read_to_string(txt_path.as_str())?;
                fs::remove_file(img_path.as_str())?;
                fs::remove_file(txt_path.as_str())?;

                if co.trim_whitespace {
                    ocr_out = ocr_out.trim().to_string();
                }
                if co.trim_single_quote {
                    ocr_out = ocr_out.trim_matches('\'').trim_matches('‘').to_string();
                }
                if co.trim_double_quote {
                    ocr_out = ocr_out.trim_matches('"').to_string();
                }
                if co.no_newlines {
                    ocr_out = ocr_out.replace('\n', "").to_string();
                }

                progress.increment();

                Ok((i, j, ocr_out))
            })
            .collect();

        for res in out_flat {
            match res {
                Ok((i, j, s)) => items[i][j] = s,
                Err(e) => {
                    dbg!(e);
                }
            }
        }

        TableEdit { items }
    }
}

impl egui_inspect::eframe::App for TableGrid {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut egui_inspect::eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            if ui.button("Select table image").clicked() {
                self.image_path = rfd::FileDialog::new().set_directory(".").pick_file();
                self.load_image();
            }
            if self.image.is_some() {
                self.grid.sort_vert();
                self.grid.sort_horiz();

                self.update_extents();

                Window::new("Annotation").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button("Help", |ui| {
                            ui.horizontal(|ui| {
                                ui.label(HELP_STR);
                            })
                        });

                        self.image.as_mut().unwrap().inspect_rotation(ui);

                        SHARED_STATE.with_borrow_mut(|ss| {
                            ui.label("Separator thickness");
                            ui.add(Slider::new(&mut ss.delta_x, 0.0001..=0.01).logarithmic(true));
                            ss.separator_color.inspect_mut("Separator color", ui);
                        });

                        if ui.button("Remove horiz").clicked() {
                            if self.grid.horizontals.len() > 2 {
                                self.grid.horizontals.remove(0);
                            }
                        }
                        if ui.button("Remove vert").clicked() {
                            if self.grid.verticals.len() > 2 {
                                self.grid.verticals.pop();
                            }
                        }
                        if ui.button("Reset grid").clicked() {
                            self.grid = Default::default();
                        }
                    });
                    ui.horizontal(|ui| {
                        self.cmd_template.inspect_mut("command", ui);
                        ui.menu_button("Preset commands", |ui| {
                            if ui.button("Tessseract").clicked() {
                                self.cmd_template = OCROptions::Tesseract.cmd_template();
                                ui.close_menu();
                            }
                            if ui.button("Cuneiform").clicked() {
                                self.cmd_template = OCROptions::Cuneiform.cmd_template();
                                ui.close_menu();
                            }
                        });
                    });

                    self.process_task.inspect_mut("", ui);
                    let ongoing = match &self.process_task {
                        BackgroundTask::Ongoing { .. } => true,
                        _ => false,
                    };
                    if !ongoing {
                        if ui.button("Extract").clicked() {
                            if let BackgroundTask::Starting { task }
                            | BackgroundTask::Finished { task, .. } = &mut self.process_task
                            {
                                task.grid = self.grid.clone();
                                task.cim = self.image.as_ref().unwrap().rotated.clone();
                                task.cmd_template = self.cmd_template.clone();
                                task.n_tasks = (self.grid.horizontals.len() - 1)
                                    * (self.grid.verticals.len() - 1);
                                task.ready = true;
                            }
                        }
                    }

                    if let BackgroundTask::Finished {
                        result: Ok(table), ..
                    } = &mut self.process_task
                    {
                        table.inspect_mut("", ui);
                    }

                    let middle_held =
                        ui.input(|r| r.pointer.button_down(egui::PointerButton::Middle));
                    let zooming =
                        ui.input(|r| r.pointer.button_down(egui::PointerButton::Secondary));
                    let (new_horiz, new_vert) = ui.input(|r| {
                        let sec = r.pointer.button_clicked(egui::PointerButton::Secondary);
                        let shif = r.modifiers.shift;
                        (sec && !shif, sec && shif)
                    });

                    let texture = self.image.as_mut().unwrap().rot_tex(ui.ctx());
                    let mut drag_enabled = SHARED_STATE.with_borrow(|ss| ss.drag_enabled);

                    Plot::new("plot")
                        .show_axes(false)
                        .show_grid(false)
                        .allow_drag(drag_enabled)
                        .allow_boxed_zoom(false)
                        .view_aspect(texture.aspect_ratio())
                        .data_aspect(1.0 / texture.aspect_ratio())
                        .show(ui, |pui| {
                            if let Some(pointer) = pui.pointer_coordinate() {
                                if 0.0 < pointer.x
                                    && pointer.x < 1.0
                                    && 0.0 < pointer.y
                                    && pointer.y < 1.0
                                {
                                    if new_horiz {
                                        self.grid.horizontals.push(HorizSep { y: pointer.y });
                                    }
                                    if new_vert {
                                        self.grid.verticals.push(VertSep { x: pointer.x });
                                    }
                                }
                            }

                            let plot_img =
                                PlotImage::new(texture, PlotPoint::new(0.5, 0.5), vec2(1.0, 1.0));
                            pui.image(plot_img);

                            drag_enabled = !(middle_held || zooming);

                            if middle_held {
                                // shift all
                                drag_enabled = false;
                                let dd = pui.pointer_coordinate_drag_delta();
                                for v in self.grid.verticals.iter_mut() {
                                    v.translate(dd)
                                }
                                for h in self.grid.horizontals.iter_mut() {
                                    h.translate(dd)
                                }
                            }

                            SHARED_STATE.with_borrow_mut(|ss| {
                                ss.drag_enabled = drag_enabled;
                                ss.delta_y = ss.delta_x * (texture.aspect_ratio() as f64);
                            });
                            self.grid.plot_inspect(pui);
                        });
                });
            } else {
                ui.label("Must load an image first.");
            }
        });
    }
}

fn main() -> egui_inspect::eframe::Result<()> {
    egui_inspect::eframe::run_native(
        "Table OCR",
        Default::default(),
        Box::new(|_cc| Ok(Box::new(TableGrid::default()))),
    )
}
