use std::{
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};

use num_enum::FromPrimitive;
use rand::{Rng, TryRngCore, rand_core::UnwrapErr, rngs::OsRng, seq::SliceRandom};
use web_time::Instant;

use arc_swap::ArcSwap;
use bebop::Record;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Pos2, Rect, RichText, Sense, Vec2,
    emath::inverse_lerp,
};
use futures_util::{SinkExt, StreamExt};
use indexmap::IndexMap;
use keyframe::functions;
use web_sys::wasm_bindgen::JsCast;
use ws_stream_wasm::{WsMessage, WsMeta};

use crate::messages::redis_game::{GameMessage, KeyValue};

struct CellAnimation {
    animation_start: Instant,
}

impl CellAnimation {
    fn new() -> Self {
        Self {
            animation_start: Instant::now(),
        }
    }
}

#[derive(FromPrimitive, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Powerup {
    #[default]
    Rows,
    Columns,
    RowsColumns,
    Random,
    Autoclick,
}

const POWERUPS: [Powerup; 5] = [
    Powerup::Rows,
    Powerup::Columns,
    Powerup::RowsColumns,
    Powerup::Random,
    Powerup::Autoclick,
];

impl Powerup {
    fn hovered(&self, cursor_pos: (usize, usize), grid_cell: (usize, usize)) -> bool {
        match self {
            Self::Rows => cursor_pos.1 % 2 == grid_cell.1 % 2,
            Self::Columns => cursor_pos.0 % 2 == grid_cell.0 % 2,
            Self::RowsColumns => {
                cursor_pos.0 % 2 == grid_cell.0 % 2 || cursor_pos.1 % 2 == grid_cell.1 % 2
            }
            Self::Autoclick => cursor_pos == grid_cell,
            Self::Random => OsRng.unwrap_err().random_bool(1.0 / 3.0),
        }
    }
}

pub struct TemplateApp {
    label: String,
    joined: Arc<AtomicBool>,
    leaderboard: bool,
    error: Arc<ArcSwap<String>>,
    people: Arc<ArcSwap<IndexMap<String, AtomicI64>>>,
    click_sender: flume::Sender<Vec<(String, i64)>>,
    animation_state: Vec<CellAnimation>,
    powerup_instant: Instant,
    autoclick_instant: Instant,
    show_powerup_window: bool,
    rng: UnwrapErr<OsRng>,
    x_down: bool,
    z_down: bool,
    powerup: Option<Powerup>,
    powerups: std::array::IntoIter<Powerup, 5>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: String::new(),
            joined: Arc::new(false.into()),
            leaderboard: false,
            error: Arc::new(ArcSwap::new(Arc::new(String::new()))),
            people: Arc::new(ArcSwap::new(Arc::new(IndexMap::default()))),
            click_sender: flume::unbounded().0,
            animation_state: Vec::new(),
            powerup_instant: Instant::now(),
            autoclick_instant: Instant::now(),
            show_powerup_window: true,
            rng: OsRng.unwrap_err(),
            x_down: false,
            z_down: false,
            powerup: None,
            powerups: {
                let mut powerups = POWERUPS;
                powerups.shuffle(&mut OsRng.unwrap_err());
                powerups.into_iter()
            },
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        Default::default()
    }

    fn next_powerup(&mut self) -> Powerup {
        if let Some(powerup) = self.powerups.next() {
            powerup
        } else {
            let mut powerups = POWERUPS;
            powerups.shuffle(&mut self.rng);
            self.powerups = powerups.into_iter();
            self.powerups.next().unwrap()
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repaintInvalid frame headering, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        // egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        //     // The top panel is often a good place for a menu bar:

        //     egui::MenuBar::new().ui(ui, |ui| {
        //         // NOTE: no File->Quit on web pages!
        //         let is_web = cfg!(target_arch = "wasm32");
        //         if !is_web {
        //             ui.menu_button("File", |ui| {
        //                 if ui.button("Quit").clicked() {
        //                     ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        //                 }
        //             });
        //             ui.add_space(16.0);
        //         }

        //         egui::widgets::global_theme_preference_buttons(ui);
        //     });
        // });

        let joined = self.joined.load(Ordering::Relaxed);
        if self.leaderboard {
            egui::CentralPanel::default()
                .frame(Frame::new().fill(Color32::BLACK))
                .show(ctx, |ui| {
                    let map = self.people.load();
                    let mut map = map.iter().collect::<Vec<_>>();
                    map.sort_by_key(|k| k.1.load(Ordering::Relaxed));
                    let max_score = map.last().map_or(0, |s| s.1.load(Ordering::Relaxed)) as f32;
                    let min_score =
                        map.get(0).map_or(i64::MIN, |s| s.1.load(Ordering::Relaxed)) as f32;
                    egui::Grid::new("leaderboard")
                        .num_columns(3)
                        .spacing([40.0, 4.0])
                        .show(ui, |ui| {
                            for (name, score) in map.into_iter().rev() {
                                ui.label(RichText::new(name).font(FontId::proportional(32.0)));
                                let score = score.load(Ordering::Relaxed) as f32;
                                ui.label(
                                    RichText::new(format!("{}", score))
                                        .font(FontId::proportional(32.0)),
                                );
                                ui.add(
                                    egui::ProgressBar::new(
                                        inverse_lerp(min_score..=max_score, score).unwrap_or(0.0),
                                    )
                                    .desired_height(32.0),
                                );
                                ui.end_row();
                            }
                        });
                });
        } else {
            egui::CentralPanel::default()
                .frame(Frame::new().fill(Color32::BLACK))
                .show(ctx, |ui| {
                    let map = self.people.load();
                    self.animation_state
                        .resize_with(map.len(), CellAnimation::new);

                    let grid_stride = if map.len() <= 2 {
                        map.len()
                    } else {
                        (map.len() as f64 + (map.len() % 2) as f64).sqrt().ceil() as usize
                    };

                    let max_rect = ui.max_rect();
                    let cell_size = Vec2::new(
                        max_rect.width() / grid_stride as f32,
                        max_rect.height() / grid_stride as f32,
                    );
                    let grid_cell_pointer_pos = ui.input(|i| i.pointer.latest_pos()).map(|pos| {
                        (
                            (pos.x / cell_size.x) as usize,
                            (pos.y / cell_size.y) as usize,
                        )
                    });
                    let mut map_iter = map.iter().zip(self.animation_state.iter_mut());
                    let x_currently_down = ui.input(|i| i.key_down(egui::Key::X));
                    let z_currently_down = ui.input(|i| i.key_down(egui::Key::Z));
                    let mut xz_pressed = false;
                    if !self.x_down && x_currently_down {
                        self.x_down = true;
                        xz_pressed = true;
                    }
                    if !self.z_down && z_currently_down {
                        self.z_down = true;
                        xz_pressed = true;
                    }
                    if !x_currently_down {
                        self.x_down = false;
                    }
                    if !z_currently_down {
                        self.z_down = false;
                    }
                    let mut clicks = Vec::new();
                    for y in 0..grid_stride {
                        for x in 0..grid_stride {
                            let grid_cell = ui.allocate_rect(
                                Rect::from_min_size(
                                    Pos2 {
                                        x: cell_size.x.mul_add(x as f32, max_rect.min.x),
                                        y: cell_size.y.mul_add(y as f32, max_rect.min.y),
                                    },
                                    cell_size,
                                ),
                                Sense::click(),
                            );
                            let hovered = if let (Some(powerup), Some(grid_cell_pointer_pos)) =
                                (self.powerup, grid_cell_pointer_pos)
                            {
                                powerup.hovered(grid_cell_pointer_pos, (x, y))
                            } else {
                                grid_cell.hovered()
                            };
                            ui.painter().rect(
                                grid_cell.rect,
                                CornerRadius::ZERO,
                                if hovered {
                                    ui.style().visuals.window_stroke.color
                                } else {
                                    ui.style().visuals.window_fill
                                },
                                ui.style().visuals.window_stroke,
                                egui::StrokeKind::Middle,
                            );
                            if let Some(((name, score), animation_state)) = map_iter.next() {
                                if hovered
                                    && (xz_pressed
                                        || ui.input(|i| i.pointer.any_click())
                                        || (self.powerup == Some(Powerup::Autoclick)
                                            && self.autoclick_instant.elapsed().as_millis() > 50))
                                {
                                    self.autoclick_instant = Instant::now();
                                    animation_state.animation_start = Instant::now();
                                    if name == &self.label
                                        || (ui.input(|i| i.modifiers.ctrl)
                                            && grid_cell_pointer_pos == Some((x, y)))
                                    {
                                        clicks.push((name.clone(), 10_000));
                                        score.fetch_add(10_000, Ordering::Relaxed);
                                    } else {
                                        clicks.push((name.clone(), -10_000i64));
                                        score.fetch_sub(10_000, Ordering::Relaxed);
                                    }
                                }
                                ui.painter().text(
                                    Pos2::new(
                                        grid_cell.rect.min.x + grid_cell.rect.width() / 2.0,
                                        grid_cell.rect.min.y,
                                    ),
                                    Align2::CENTER_TOP,
                                    name,
                                    FontId::proportional(keyframe::ease_with_scaled_time(
                                        functions::EaseInCubic,
                                        72.0,
                                        54.0,
                                        animation_state.animation_start.elapsed().as_secs_f64(),
                                        0.1,
                                    )),
                                    ui.style().visuals.text_color(),
                                );
                                ui.painter().text(
                                    Pos2::new(
                                        grid_cell.rect.min.x + grid_cell.rect.width() / 2.0,
                                        grid_cell.rect.max.y,
                                    ),
                                    Align2::CENTER_BOTTOM,
                                    format!("{}", score.load(Ordering::Relaxed)),
                                    FontId::proportional(keyframe::ease_with_scaled_time(
                                        functions::EaseInCubic,
                                        32.0,
                                        24.0,
                                        animation_state.animation_start.elapsed().as_secs_f64(),
                                        0.1,
                                    )),
                                    ui.style().visuals.text_color(),
                                );
                            }
                        }
                    }
                    if !clicks.is_empty() {
                        self.click_sender.send(clicks).unwrap();
                    }
                    if joined {
                        let now = Instant::now();
                        ui.painter().rect_filled(
                            Rect::from_min_max(
                                Pos2::new(max_rect.min.x, max_rect.max.y - 12.0),
                                Pos2::new(
                                    keyframe::ease_with_scaled_time(
                                        functions::Linear,
                                        max_rect.min.x,
                                        max_rect.max.x,
                                        if self.powerup_instant > now {
                                            self.powerup_instant.duration_since(now).as_secs_f64()
                                        } else {
                                            self.powerup = None;
                                            self.powerup_instant.elapsed().as_secs_f64()
                                        },
                                        10.0,
                                    ),
                                    max_rect.max.y,
                                ),
                            ),
                            CornerRadius::ZERO,
                            Color32::WHITE,
                        );
                    }
                });
        }

        if ctx.input(|i| i.modifiers.shift) && self.powerup_instant.elapsed().as_secs() > 9 {
            self.powerup_instant = Instant::now() + Duration::from_secs(8);
            self.powerup = Some(self.next_powerup());
        }

        if !joined {
            egui::Window::new("Enter your name")
                .auto_sized()
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let label_response = ui.text_edit_singleline(&mut self.label);
                    if ui.button("Join").clicked()
                        || (label_response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        let joined = Arc::clone(&self.joined);
                        let error = Arc::clone(&self.error);
                        let people = Arc::clone(&self.people);
                        let label = self.label.clone();
                        let (tx, rx) = flume::unbounded();
                        self.click_sender = tx;
                        self.leaderboard = self.label == "Bradshaw";
                        self.show_powerup_window = !self.leaderboard;
                        wasm_bindgen_futures::spawn_local(websocket(
                            joined, error, people, label, rx,
                        ));
                    }
                    ui.label(self.error.load().deref().deref())
                });
        } else if self.show_powerup_window {
            egui::Window::new("Tutorial")
                .auto_sized()
                .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading("- Click your name to gain points");
                    ui.heading("- Click other people's names to make them lose points");
                    ui.heading("- Hold Ctrl and click your friends to help them");
                    ui.heading("- Press shift to use powerups when the bar is full");
                });
            if ctx.input(|i| i.modifiers.shift) {
                self.show_powerup_window = false;
            }
        }
        ctx.request_repaint();
    }
}

async fn websocket(
    joined: Arc<AtomicBool>,
    error: Arc<ArcSwap<String>>,
    people: Arc<ArcSwap<IndexMap<String, AtomicI64>>>,
    label: String,
    rx: flume::Receiver<Vec<(String, i64)>>,
) {
    let (_connection_meta, connection) = match WsMeta::connect(
        format!(
            "ws://{}/ws",
            web_sys::window()
                .unwrap()
                .window()
                .location()
                .host()
                .unwrap()
        ),
        None,
    )
    .await
    {
        Ok(conn) => conn,
        Err(e) => {
            error.store(Arc::new(format!("{}", e)));
            return;
        }
    };
    let mut connection = connection.fuse();
    joined.store(true, Ordering::Relaxed);
    connection.send(WsMessage::Text(label)).await.unwrap();
    loop {
        futures_util::select_biased! {
            click = rx.recv_async() => if let Ok(clicks) = click {
                let mut clicks_buf = Vec::new();
                for (name, amount) in &clicks {
                    clicks_buf.push(KeyValue {
                        key: name,
                        value: *amount
                    });
                }
                let mut buf = Vec::new();
                GameMessage {
                    updates: None,
                    clicks: Some(clicks_buf),
                    clear: None
                }
                .serialize(&mut buf)
                .unwrap();
                connection.send(WsMessage::Binary(buf)).await.unwrap();
            },
            message = connection.next() => if let Some(message) = message {
                match message {
                    WsMessage::Binary(message) => {
                        if let Ok(message) = GameMessage::deserialize(&message) {
                            if message.clear == Some(true) {
                                let mut map = IndexMap::default();
                                if let Some(mut updates) = message.updates {
                                    updates.sort_by_key(|kv| kv.key);
                                    for key_value in updates {
                                        map.insert(key_value.key.to_owned(), key_value.value.into());
                                    }
                                }
                                web_sys::console::log_1(&format!("{:?}", map).into());
                                people.store(Arc::new(map));
                            } else {
                                let map = people.load();
                                if let Some(updates) = message.updates {
                                    for key_value in updates {
                                        if let Some(person) = map.get(key_value.key) {
                                            person.store(key_value.value, Ordering::Relaxed);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    WsMessage::Text(error) => {
                        let document = web_sys::window()
                            .expect("No window")
                            .document()
                            .expect("No document");

                        let div = document
                            .get_element_by_id("page")
                            .expect("Failed to find page div")
                            .dyn_into::<web_sys::HtmlDivElement>()
                            .expect("page was not a HtmlDivElement");

                        div.set_inner_html(&error);
                        return;
                    }
                }
            }
        }
    }
}
