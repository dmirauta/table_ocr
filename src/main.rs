use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use eframe::NativeOptions;
use egui::{
    vec2, CentralPanel, Color32, ColorImage, Context, Slider, Stroke, TextureHandle,
    TextureOptions, Vec2,
};
use egui_inspect::EguiInspect;
use egui_plot::{Plot, PlotImage, PlotPoint, PlotUi, Polygon};
use image::{ColorType, ImageResult};

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

#[derive(Default)]
pub struct TableGrid {
    image_path: Option<PathBuf>,
    cimage: Option<ColorImage>,
    texture: Option<TextureHandle>,
    grid: Grid,
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

impl TableGrid {
    fn crop_buffer(&self, x1: f64, x2: f64, y1: f64, y2: f64) -> (Vec<u8>, [usize; 2]) {
        let orig_size = self.texture.as_ref().unwrap().size();
        let cim = self.cimage.as_ref().unwrap();
        let i0 = (clip(x1.min(x2)) * (orig_size[0] as f64)) as usize;
        let i1 = (clip(x1.max(x2)) * (orig_size[0] as f64)) as usize;
        let j0 = (clip(1.0 - y1.max(y2)) * (orig_size[1] as f64)) as usize;
        let j1 = (clip(1.0 - y1.min(y2)) * (orig_size[1] as f64)) as usize;
        let size = [i1 - i0, j1 - j0];
        // dbg!(i0, i1, j0, j1, &size);
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
    fn try_set_tex(&mut self, ctx: &Context) {
        let img_path = self.image_path.take().unwrap();
        self.cimage = Some({
            let image = image::io::Reader::open(img_path).unwrap().decode().unwrap();
            let size = [image.width() as _, image.height() as _];
            let image_buffer = image.to_rgba8();
            let pixels = image_buffer.as_flat_samples();
            ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
        });

        self.texture = Some(ctx.load_texture(
            "test_img",
            self.cimage.as_ref().unwrap().clone(),
            TextureOptions::LINEAR,
        ));
    }
    fn process(&self) -> Vec<Vec<String>> {
        let mut out = vec![];

        for (i, hw) in self.grid.horizontals.windows(2).rev().enumerate() {
            let mut row = vec![];
            for (j, vw) in self.grid.verticals.windows(2).enumerate() {
                let img_path = format!("/tmp/ocr_crop_{i}_{j}.jpeg");
                let txt_path = format!("/tmp/ocr_out_{i}_{j}");

                let (buff, size) = self.crop_buffer(vw[0].x, vw[1].x, hw[0].y, hw[1].y);
                save_img(buff.as_slice(), size, Path::new(img_path.as_str())).unwrap();

                let mut handle = Command::new("tesseract")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .arg(img_path.as_str())
                    .arg(txt_path.as_str())
                    .arg("-l")
                    .arg("eng")
                    .spawn()
                    .unwrap();
                handle.wait().unwrap();

                let txt_path = format!("{txt_path}.txt");
                let ocr_out = fs::read_to_string(txt_path.as_str()).unwrap();
                fs::remove_file(img_path.as_str()).unwrap();
                fs::remove_file(txt_path.as_str()).unwrap();

                row.push(ocr_out.trim().trim_matches('‘').to_string());
            }
            out.push(row);
        }

        out
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

// static IMG: &str = "/home/d/Pictures/table2.png";

impl eframe::App for TableGrid {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            if ui.button("Select table image").clicked() {
                self.image_path = rfd::FileDialog::new().set_directory(".").pick_file();
                self.try_set_tex(ui.ctx());
            }
            if self.texture.is_some() {
                self.grid.sort_vert();
                self.grid.sort_horiz();

                self.update_extents();

                ui.horizontal(|ui| {
                    SHARED_STATE.with_borrow_mut(|ss| {
                        ui.label("Separator thickness");
                        ui.add(Slider::new(&mut ss.delta_x, 0.0001..=0.01).logarithmic(true));
                        ss.separator_color.inspect_mut("Separator colors", ui);
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
                    if ui.button("Test save").clicked() {
                        let tab = self.process();
                        dbg!(tab);
                    }
                });

                let middle_held = ui.input(|r| r.pointer.button_down(egui::PointerButton::Middle));
                let zooming = ui.input(|r| r.pointer.button_down(egui::PointerButton::Secondary));
                let (new_horiz, new_vert) = ui.input(|r| {
                    let sec = r.pointer.button_clicked(egui::PointerButton::Secondary);
                    let shif = r.modifiers.shift;
                    (sec && !shif, sec && shif)
                });

                let texture = self.texture.as_ref().unwrap();
                let mut drag_enabled = SHARED_STATE.with_borrow(|ss| ss.drag_enabled);
                Plot::new("plot")
                    .show_axes(false)
                    .show_grid(false)
                    .allow_drag(drag_enabled)
                    .view_aspect(1.0)
                    .data_aspect(1.0 / texture.aspect_ratio())
                    .show(ui, |pui| {
                        if let Some(pointer) = pui.pointer_coordinate() {
                            if new_horiz {
                                self.grid.horizontals.push(HorizSep { y: pointer.y });
                            }
                            if new_vert {
                                self.grid.verticals.push(VertSep { x: pointer.x });
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
            } else {
                ui.label("Must load an image first.");
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Table OCR",
        NativeOptions::default(),
        Box::new(|_cc| Box::new(TableGrid::default())),
    )
}
