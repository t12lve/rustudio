use crate::backend::{Action, Backend};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const GOLD: Color32 = Color32::from_rgb(242, 197, 79);
const PAPER: Color32 = Color32::from_rgb(27, 26, 23);
const PANEL: Color32 = Color32::from_rgb(37, 36, 32);
const CHASSIS: Color32 = Color32::from_rgb(17, 17, 15);
const MUTED: Color32 = Color32::from_rgb(177, 173, 160);
const RULE: Color32 = Color32::from_rgb(68, 66, 59);
const INK: Color32 = Color32::from_rgb(239, 237, 229);
const CHANNELS: [(&str, &str); 8] = [
    ("Mic", "Micro"),
    ("Chat", "Chat"),
    ("Music", "Musique"),
    ("Game", "Jeu"),
    ("Console", "Console"),
    ("LineIn", "Entrée ligne"),
    ("System", "Système"),
    ("Sample", "Samples"),
];
const OUTPUTS: [(&str, &str); 6] = [
    ("Headphones", "Casque"),
    ("BroadcastMix", "Stream"),
    ("ChatMic", "Chat vocal"),
    ("LineOut", "Sortie ligne"),
    ("StreamMix2", "Stream 2"),
    ("Sampler", "Sampler"),
];
const TABS: [&str; 8] = [
    "Mixeur",
    "Micro",
    "Configuration",
    "Routage",
    "Éclairage",
    "Effets",
    "Échantillonneur",
    "Système",
];
const FADERS: [&str; 4] = ["A", "B", "C", "D"];
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_string()
}
fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}
fn color(index: usize) -> Color32 {
    [
        Color32::from_rgb(141, 208, 156),
        Color32::from_rgb(153, 182, 241),
        Color32::from_rgb(245, 178, 110),
        Color32::from_rgb(212, 150, 205),
        MUTED,
    ][index.min(4)]
}
fn name(id: &str) -> String {
    CHANNELS
        .iter()
        .chain(
            [
                ("Headphones", "Casque"),
                ("LineOut", "Sortie ligne"),
                ("MicMonitor", "Retour micro"),
            ]
            .iter(),
        )
        .find(|(key, _)| *key == id)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| id.to_string())
}
fn button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add_sized(
        [ui.available_width().min(180.0), 44.0],
        egui::Button::new(RichText::new(label).color(if selected { GOLD } else { INK }))
            .fill(if selected {
                Color32::from_rgb(56, 47, 27)
            } else {
                PANEL
            })
            .stroke(if selected {
                Stroke::new(1.0_f32, GOLD)
            } else {
                Stroke::new(1.0_f32, RULE)
            }),
    )
}
fn tabs(ui: &mut egui::Ui, selected: &mut usize, labels: &[&str]) {
    ui.horizontal(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let response = ui.add_sized(
                [110.0, 42.0],
                egui::Button::new(RichText::new(*label).color(if *selected == index {
                    GOLD
                } else {
                    INK
                }))
                .fill(if *selected == index {
                    Color32::from_rgb(56, 47, 27)
                } else {
                    PAPER
                }),
            );
            if response.clicked() {
                *selected = index;
            }
        }
    });
    ui.add_space(10.0);
}
fn combo(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    current: &str,
    options: &[(String, String)],
) -> Option<String> {
    if !label.is_empty() {
        ui.label(label);
    }
    let mut next = current.to_string();
    egui::ComboBox::from_id_salt(id)
        .width(
            ui.available_width()
                .min(if id == "device" || id == "profile" {
                    160.0
                } else {
                    360.0
                }),
        )
        .selected_text(
            options
                .iter()
                .find(|(id, _)| id == current)
                .map(|(_, name)| name.as_str())
                .unwrap_or(current),
        )
        .show_ui(ui, |ui| {
            for (value, label) in options {
                ui.selectable_value(&mut next, value.clone(), label);
            }
        });
    (next != current).then_some(next)
}

pub struct Studio {
    backend: Backend,
    serial: String,
    tab: usize,
    subtab: usize,
    fader: usize,
    band: usize,
    bank: String,
    pad: String,
    effect: usize,
    light_group: usize,
    light_index: usize,
    mix_b: bool,
    library_page: usize,
    sample_index: usize,
    rename_effect: bool,
    layout_overflow: bool,
    notice: String,
    boot_error: String,
    logo: egui::TextureHandle,
    data: std::path::PathBuf,
    draft: HashMap<String, f64>,
    copy_name: String,
    copy_open: bool,
    load_profile: Option<(bool, String)>,
    smoke: bool,
    smoke_step: usize,
    capture_pending: bool,
    last_step: Instant,
    started: Instant,
    report: Vec<Value>,
}
impl Studio {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        data: std::path::PathBuf,
        boot_error: String,
    ) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "RobotoFlex".into(),
            egui::FontData::from_static(include_bytes!("../assets/RobotoFlex.ttf")).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "RobotoFlex".into());
        cc.egui_ctx.set_fonts(fonts);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PAPER;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(INK);
        style.visuals.selection.bg_fill = Color32::from_rgb(68, 55, 27);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, GOLD);
        style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
        style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
        style.visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.65 };
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::ZERO;
            widget.bg_stroke = Stroke::new(1.0_f32, RULE);
        }
        style.spacing.item_spacing = Vec2::new(12.0, 10.0);
        style.spacing.button_padding = Vec2::new(12.0, 10.0);
        style.spacing.interact_size = Vec2::new(44.0, 36.0);
        style.spacing.slider_width = 360.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(24.0));
        cc.egui_ctx.set_style(style);
        let img = image::load_from_memory(include_bytes!("../assets/logo.png"))
            .unwrap()
            .into_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let logo = cc.egui_ctx.load_texture(
            "GoXLR EX",
            egui::ColorImage::from_rgba_unmultiplied(size, &img),
            Default::default(),
        );
        Self {
            backend: Backend::start(cc.egui_ctx.clone()),
            serial: String::new(),
            tab: 0,
            subtab: 0,
            fader: 0,
            band: 0,
            bank: "A".into(),
            pad: "TopLeft".into(),
            effect: 0,
            light_group: 0,
            light_index: 0,
            mix_b: false,
            library_page: 0,
            sample_index: 0,
            rename_effect: false,
            layout_overflow: false,
            notice: String::new(),
            boot_error,
            logo,
            data,
            draft: HashMap::new(),
            copy_name: String::new(),
            copy_open: false,
            load_profile: None,
            smoke: std::env::args().any(|a| a == "--smoke-test"),
            smoke_step: 0,
            capture_pending: false,
            last_step: Instant::now(),
            started: Instant::now(),
            report: vec![],
        }
    }
    fn command(&mut self, name: &str, args: Value) {
        if self.smoke {
            return;
        }
        if name == "__monitor" {
            self.command(
                "SetVolume",
                json!(["MicMonitor", (number(&args) * 255.0 / 100.0).round() as u8]),
            );
            return;
        }
        let mut object = serde_json::Map::new();
        object.insert(name.into(), args);
        let value = Value::Object(object);
        if let Err(error) = crate::backend::device_request(self.serial.clone(), value.clone()) {
            self.notice = error;
            return;
        }
        let _ = self
            .backend
            .tx
            .send(Action::Device(self.serial.clone(), value));
    }
    fn daemon(&mut self, name: &str, args: Value) {
        if self.smoke {
            return;
        }
        let value = if name == "StopDaemon" {
            json!("StopDaemon")
        } else {
            let mut object = serde_json::Map::new();
            object.insert(name.into(), args);
            Value::Object(object)
        };
        let _ = self.backend.tx.send(Action::Daemon(value));
    }
    fn parameter(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        value: f64,
        min: f64,
        max: f64,
        unit: &str,
        command: &str,
        prefix: &[Value],
    ) {
        ui.push_id((label, command), |ui| {
            ui.horizontal(|ui| {
                ui.label(label);
            });
            let mut next = value;
            let changed = ui
                .horizontal(|ui| {
                    ui.spacing_mut().slider_width =
                        (ui.available_width() - 110.0).clamp(80.0, 650.0);
                    let slider = ui.add(
                        egui::Slider::new(&mut next, min..=max)
                            .show_value(false)
                            .step_by(1.0),
                    );
                    let numeric = ui.add(
                        egui::DragValue::new(&mut next)
                            .range(min..=max)
                            .speed(1.0)
                            .suffix(format!(" {unit}")),
                    );
                    slider.changed() || numeric.changed()
                })
                .inner;
            if changed {
                let val = json!(next.round() as i64);
                let args = if prefix.is_empty() {
                    val
                } else {
                    let mut args = prefix.to_vec();
                    args.push(val);
                    json!(args)
                };
                self.command(command, args);
            }
        });
    }
    fn toggle(&mut self, ui: &mut egui::Ui, label: &str, current: bool, command: &str) {
        let mut next = current;
        if ui.checkbox(&mut next, label).changed() {
            self.command(command, json!(next));
        }
    }
    fn microphone(&mut self, ui: &mut egui::Ui, m: &Value) {
        tabs(
            ui,
            &mut self.subtab,
            &["Entrée", "Gate", "Égaliseur", "Compresseur", "Retour"],
        );
        let mic = &m["mic_status"];
        match self.subtab {
            0 => {
                ui.heading("Entrée micro");
                ui.add_space(12.0);
                let current = text(&mic["mic_type"]);
                let options = vec![
                    ("Dynamic".into(), "Dynamique".into()),
                    ("Condenser".into(), "Condensateur · 48 V".into()),
                    ("Jack".into(), "Jack 3,5 mm".into()),
                ];
                if let Some(next) = combo(ui, "mic-type", "Type de microphone", &current, &options)
                {
                    self.command("SetMicrophoneType", json!(next));
                }
                ui.add_space(16.0);
                self.parameter(
                    ui,
                    "Gain d’entrée",
                    number(&mic["mic_gains"][&current]),
                    0.0,
                    72.0,
                    "dB",
                    "SetMicrophoneGain",
                    &[json!(current)],
                );
                ui.label(RichText::new("i").color(GOLD)).on_hover_text("Le gain amplifie le signal à l’entrée. Trop bas : voix faible. Trop haut : saturation, même en baissant ensuite le volume du mix.");
            }
            1 => {
                ui.heading("Gate");
                self.toggle(
                    ui,
                    "Actif",
                    mic["noise_gate"]["enabled"].as_bool().unwrap_or(false),
                    "SetGateActive",
                );
                ui.label(RichText::new("i").color(GOLD)).on_hover_text("Une porte pour les sons faibles : elle atténue le bruit entre deux phrases, puis laisse passer votre voix au-dessus du seuil.");
                ui.add_enabled_ui(
                    mic["noise_gate"]["enabled"].as_bool().unwrap_or(false),
                    |ui| {
                        self.parameter(
                            ui,
                            "Seuil d’ouverture",
                            number(&mic["noise_gate"]["threshold"]),
                            -59.0,
                            0.0,
                            "dB",
                            "SetGateThreshold",
                            &[],
                        );
                        self.parameter(
                            ui,
                            "Atténuation",
                            number(&mic["noise_gate"]["attenuation"]),
                            0.0,
                            100.0,
                            "%",
                            "SetGateAttenuation",
                            &[],
                        );
                        self.index_choice(
                            ui,
                            "gate-attack",
                            "Attaque",
                            &mic["noise_gate"]["attack"],
                            &gate_times(),
                            "SetGateAttack",
                        );
                        self.index_choice(
                            ui,
                            "gate-release",
                            "Relâchement",
                            &mic["noise_gate"]["release"],
                            &gate_times(),
                            "SetGateRelease",
                        );
                    },
                );
            }
            2 => self.equaliser(ui, m),
            3 => {
                ui.heading("Compresseur");
                ui.label(RichText::new("i").color(GOLD)).on_hover_text("Un limiteur d’écarts : quand vous parlez fort, le compresseur réduit ce qui dépasse le seuil. Il aide à garder une voix régulière sans écraser les phrases douces.");
                self.parameter(
                    ui,
                    "Seuil de compression",
                    number(&mic["compressor"]["threshold"]),
                    -40.0,
                    0.0,
                    "dB",
                    "SetCompressorThreshold",
                    &[],
                );
                let ratios = [
                    "1:1", "1,1:1", "1,2:1", "1,4:1", "1,6:1", "1,8:1", "2:1", "2,5:1", "3,2:1",
                    "4:1", "5,6:1", "8:1", "16:1", "32:1", "64:1",
                ]
                .map(String::from);
                self.index_choice(
                    ui,
                    "ratio",
                    "Ratio",
                    &mic["compressor"]["ratio"],
                    &ratios,
                    "SetCompressorRatio",
                );
                self.index_choice(
                    ui,
                    "comp-attack",
                    "Attaque",
                    &mic["compressor"]["attack"],
                    &[
                        0.001, 2., 3., 4., 5., 6., 7., 8., 9., 10., 12., 14., 16., 18., 20., 23.,
                        26., 30., 35., 40.,
                    ]
                    .map(|v| format!("{v} ms")),
                    "SetCompressorAttack",
                );
                self.index_choice(
                    ui,
                    "comp-release",
                    "Relâchement",
                    &mic["compressor"]["release"],
                    &[
                        15, 15, 25, 35, 45, 55, 65, 75, 85, 100, 115, 140, 170, 230, 340, 680,
                        1000, 1500, 2000, 3000,
                    ]
                    .map(|v| format!("{v} ms")),
                    "SetCompressorReleaseTime",
                );
                self.parameter(
                    ui,
                    "Gain de compensation",
                    number(&mic["compressor"]["makeup_gain"]),
                    0.0,
                    24.0,
                    "dB",
                    "SetCompressorMakeupGain",
                    &[],
                );
            }
            _ => {
                ui.heading("Retour et finitions");
                self.parameter(
                    ui,
                    "Retour micro dans le casque",
                    (number(&m["levels"]["volumes"]["MicMonitor"]) / 255.0 * 100.0).round(),
                    0.0,
                    100.0,
                    "%",
                    "__monitor",
                    &[],
                );
                self.parameter(
                    ui,
                    "Bip de censure",
                    number(&m["levels"]["bleep"]),
                    -36.0,
                    0.0,
                    "dB",
                    "SetSwearButtonVolume",
                    &[],
                );
                if text(&m["hardware"]["device_type"]) != "Mini" {
                    self.parameter(
                        ui,
                        "De-esser",
                        number(&m["levels"]["deess"]),
                        0.0,
                        100.0,
                        "%",
                        "SetDeeser",
                        &[],
                    );
                }
            }
        }
    }
    fn index_choice(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        label: &str,
        current: &Value,
        labels: &[String],
        command: &str,
    ) {
        let choices: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| (i.to_string(), label.clone()))
            .collect();
        if let Some(next) = combo(ui, id, label, &number(current).to_string(), &choices) {
            if let Ok(value) = next.parse::<usize>() {
                self.command(command, json!(value));
            }
        }
    }
    fn equaliser(&mut self, ui: &mut egui::Ui, m: &Value) {
        let mini = text(&m["hardware"]["device_type"]) == "Mini";
        let key = if mini { "equaliser_mini" } else { "equaliser" };
        let eq = &m["mic_status"][key];
        let bands = if mini {
            vec![90, 250, 500, 1000, 3000, 8000]
        } else {
            vec![31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
        };
        self.band = self.band.min(bands.len() - 1);
        ui.horizontal(|ui| {
            ui.heading("Égaliseur");
            if button(ui, "À plat", false).clicked() {
                for hz in &bands {
                    self.command(
                        if mini { "SetEqMiniGain" } else { "SetEqGain" },
                        json!([eq_id(*hz), 0]),
                    );
                }
            }
        });
        let presets = [
            ("", "Choisir un preset"),
            ("neutral", "Neutre"),
            ("warm", "Chaleur"),
            ("presence", "Présence"),
            ("clarity", "Clarté"),
        ]
        .map(|(id, name)| (id.into(), name.into()));
        if let Some(preset) = combo(ui, "eq-preset", "Preset d’égaliseur", "", &presets) {
            let gains = eq_preset(&preset, mini);
            for (index, hz) in bands.iter().enumerate() {
                self.command(
                    if mini { "SetEqMiniGain" } else { "SetEqGain" },
                    json!([eq_id(*hz), gains[index]]),
                );
            }
        }
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 140.0),
            Sense::click_and_drag(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, CHASSIS);
        let plot = rect.shrink2(Vec2::new(28.0, 20.0));
        for gain in [-9, 0, 9] {
            let y = plot.center().y - gain as f32 / 18.0 * plot.height();
            p.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(1.0_f32, RULE),
            );
            p.text(
                Pos2::new(rect.left() + 3.0, y),
                Align2::LEFT_CENTER,
                gain.to_string(),
                FontId::proportional(11.0),
                MUTED,
            );
        }
        let points: Vec<Pos2> = bands
            .iter()
            .map(|hz| {
                let id = eq_id(*hz);
                let freq = number(&eq["frequency"][&id]).max(30.0);
                Pos2::new(
                    plot.left()
                        + ((freq / 30.0).ln() / (18000.0f64 / 30.0).ln()) as f32 * plot.width(),
                    plot.center().y - number(&eq["gain"][&id]) as f32 / 18.0 * plot.height(),
                )
            })
            .collect();
        p.add(egui::Shape::line(
            points.clone(),
            Stroke::new(2.0_f32, GOLD),
        ));
        for (index, point) in points.iter().enumerate() {
            p.circle_filled(*point, if index == self.band { 5.0 } else { 3.0 }, GOLD);
        }
        if let Some(pos) = response.interact_pointer_pos() {
            if response.clicked() {
                self.band = points
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.distance(pos).total_cmp(&b.distance(pos)))
                    .unwrap()
                    .0;
            }
            if response.dragged() {
                let gain = ((plot.center().y - pos.y) / plot.height() * 18.0)
                    .round()
                    .clamp(-9.0, 9.0);
                self.command(
                    if mini { "SetEqMiniGain" } else { "SetEqGain" },
                    json!([eq_id(bands[self.band]), gain as i8]),
                );
            }
        }
        ui.horizontal(|ui| {
            for (index, hz) in bands.iter().enumerate() {
                if ui
                    .selectable_label(
                        self.band == index,
                        format!(
                            "{}\n{:+} dB",
                            hz_label(*hz),
                            number(&eq["gain"][eq_id(*hz)])
                        ),
                    )
                    .clicked()
                {
                    self.band = index;
                }
            }
        });
        let id = eq_id(bands[self.band]);
        let lower = if self.band == 0 {
            30.0
        } else {
            number(&eq["frequency"][eq_id(bands[self.band - 1])]) + 0.5
        };
        let upper = if self.band == bands.len() - 1 {
            20000.0
        } else {
            number(&eq["frequency"][eq_id(bands[self.band + 1])]) - 0.5
        };
        ui.columns(2, |columns| {
            self.parameter(
                &mut columns[0],
                &format!("Gain {}", hz_label(bands[self.band])),
                number(&eq["gain"][&id]),
                -9.0,
                9.0,
                "dB",
                if mini { "SetEqMiniGain" } else { "SetEqGain" },
                &[json!(id)],
            );
            self.parameter(
                &mut columns[1],
                "Fréquence centrale",
                number(&eq["frequency"][eq_id(bands[self.band])]),
                lower.ceil(),
                upper.floor(),
                "Hz",
                if mini { "SetEqMiniFreq" } else { "SetEqFreq" },
                &[json!(eq_id(bands[self.band]))],
            );
        });
        ui.label(
            RichText::new("Courbe indicative, sans analyse audio.")
                .color(MUTED)
                .size(12.0),
        );
    }
}
fn eq_id(hz: i32) -> String {
    if hz >= 1000 {
        format!("Equalizer{}KHz", hz / 1000)
    } else {
        format!("Equalizer{hz}Hz")
    }
}
fn hz_label(hz: i32) -> String {
    if hz >= 1000 {
        format!("{} kHz", hz / 1000)
    } else {
        format!("{hz} Hz")
    }
}
fn gate_times() -> Vec<String> {
    (1..=20)
        .map(|v| v * 10)
        .chain((250..=1000).step_by(50))
        .chain((1100..=2000).step_by(100))
        .map(|v| format!("{v} ms"))
        .collect()
}
fn eq_preset(id: &str, mini: bool) -> Vec<i8> {
    match (id, mini) {
        ("warm", true) => vec![0, 2, 1, 0, -1, -2],
        ("presence", true) => vec![-2, 0, 0, 1, 3, 1],
        ("clarity", true) => vec![-3, -1, 0, 1, 2, 2],
        ("warm", false) => vec![-3, -1, 2, 2, 1, 0, 0, -1, -1, -2],
        ("presence", false) => vec![-3, -2, -1, 0, 0, 1, 3, 3, 1, 0],
        ("clarity", false) => vec![-4, -3, -1, -1, 0, 1, 2, 2, 2, 1],
        _ => vec![0; if mini { 6 } else { 10 }],
    }
}

impl Studio {
    fn device(&mut self, ui: &mut egui::Ui, m: &Value) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().min(440.0), 250.0),
            Sense::click(),
        );
        let p = ui.painter_at(rect);
        let body = rect.shrink2(Vec2::new(26.0, 18.0));
        let front = body.translate(Vec2::new(0.0, 10.0));
        p.rect_filled(front, 0.0, Color32::BLACK);
        p.rect_filled(body, 0.0, Color32::from_rgb(47, 47, 43));
        p.rect_stroke(
            body,
            0.0,
            Stroke::new(2.0_f32, RULE),
            egui::StrokeKind::Inside,
        );
        let width = (body.width() - 32.0) / 4.0;
        for (index, id) in FADERS.iter().enumerate() {
            let x = body.left() + 16.0 + width * (index as f32 + 0.5);
            let fader = &m["fader_status"][*id];
            let source = text(&fader["channel"]);
            let level = number(&m["levels"]["volumes"][&source]) / 255.0;
            let top = body.top() + 48.0;
            let bottom = body.bottom() - 44.0;
            p.text(
                Pos2::new(x, body.top() + 25.0),
                Align2::CENTER_CENTER,
                name(&source),
                FontId::proportional(10.0),
                color(index),
            );
            p.line_segment(
                [Pos2::new(x, top), Pos2::new(x, bottom)],
                Stroke::new(5.0_f32, CHASSIS),
            );
            p.line_segment(
                [Pos2::new(x + 16.0, top), Pos2::new(x + 16.0, bottom)],
                Stroke::new(3.0_f32, color(index)),
            );
            let y = bottom - level as f32 * (bottom - top);
            p.rect_filled(
                Rect::from_center_size(Pos2::new(x, y), Vec2::new(30.0, 14.0)),
                0.0,
                Color32::from_gray(178),
            );
            p.line_segment(
                [Pos2::new(x - 12.0, y), Pos2::new(x + 12.0, y)],
                Stroke::new(2.0_f32, CHASSIS),
            );
            p.rect_filled(
                Rect::from_center_size(Pos2::new(x, body.bottom() - 24.0), Vec2::new(34.0, 12.0)),
                0.0,
                color(index),
            );
            if self.fader == index {
                p.rect_stroke(
                    Rect::from_min_max(
                        Pos2::new(x - width / 2.0 + 2.0, body.top() + 8.0),
                        Pos2::new(x + width / 2.0 - 2.0, body.bottom() - 10.0),
                    ),
                    0.0,
                    Stroke::new(1.0_f32, GOLD),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.fader = (((pos.x - body.left() - 16.0) / width).floor() as usize).min(3);
            }
        }
    }
    fn mixer(&mut self, ui: &mut egui::Ui, m: &Value) {
        tabs(ui, &mut self.subtab, &["Entrées", "Sorties"]);
        if self.subtab == 1 {
            for (id, label) in [
                ("Headphones", "Casque"),
                ("MicMonitor", "Retour micro"),
                ("LineOut", "Sortie ligne"),
            ] {
                let mut volume = (number(&m["levels"]["volumes"][id]) / 255.0 * 100.0).round();
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui
                        .add(
                            egui::Slider::new(&mut volume, 0.0..=100.0)
                                .step_by(1.0)
                                .suffix(" %"),
                        )
                        .changed()
                    {
                        self.command(
                            "SetVolume",
                            json!([id, (volume * 255.0 / 100.0).round() as u8]),
                        );
                    }
                });
            }
            let outputs: Vec<_> = OUTPUTS
                .iter()
                .filter(|(id, _)| *id != "Sampler" || text(&m["hardware"]["device_type"]) != "Mini")
                .map(|(id, label)| (id.to_string(), label.to_string()))
                .collect();
            if let Some(next) = combo(
                ui,
                "monitor",
                "Écoute au casque",
                &text(&m["levels"]["output_monitor"]),
                &outputs,
            ) {
                self.command("SetMonitorMix", json!(next));
            }
            let supported = m["levels"]["submix_supported"].as_bool().unwrap_or(false);
            ui.add_enabled_ui(supported, |ui| {
                self.toggle(
                    ui,
                    "Double mix",
                    !m["levels"]["submix"].is_null(),
                    "SetSubMixEnabled",
                );
            });
            if let Some(outputs) = m["levels"]["submix"]["outputs"].as_object() {
                for (id, value) in outputs {
                    if let Some(next) = combo(
                        ui,
                        id,
                        &format!(
                            "Mix de {}",
                            OUTPUTS
                                .iter()
                                .find(|(key, _)| *key == id)
                                .map(|(_, label)| *label)
                                .unwrap_or(id)
                        ),
                        &text(value),
                        &[("A".into(), "Mix A".into()), ("B".into(), "Mix B".into())],
                    ) {
                        self.command("SetSubMixOutputMix", json!([id, next]));
                    }
                }
            }
            return;
        }
        let dual = !m["levels"]["submix"].is_null();
        if !dual {
            self.mix_b = false;
        }
        if dual {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mix_b, false, "Mix A");
                ui.selectable_value(&mut self.mix_b, true, "Mix B");
            });
        }
        let volume_command = if self.mix_b {
            "SetSubMixVolume"
        } else {
            "SetVolume"
        };
        let mini = text(&m["hardware"]["device_type"]) == "Mini";
        let count = if mini { 7 } else { 8 };
        let show_device = ui.available_width() > 1100.0;
        let channel_width =
            ((ui.available_width() - if show_device { 300.0 } else { 0.0 }) / count as f32 - 12.0)
                .max(64.0);
        ui.horizontal_top(|ui| {
            for (index, (id, label)) in CHANNELS.iter().take(count).enumerate() {
                ui.allocate_ui_with_layout(
                    Vec2::new(channel_width, 420.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let assigned = FADERS
                            .iter()
                            .find(|f| text(&m["fader_status"][**f]["channel"]) == *id)
                            .copied();
                        ui.label(
                            RichText::new(assigned.unwrap_or(" "))
                                .color(color(index))
                                .strong(),
                        );
                        ui.label(RichText::new(*label).strong());
                        ui.add_space(8.0);
                        let raw = if self.mix_b {
                            number(&m["levels"]["submix"]["inputs"][*id]["volume"])
                        } else {
                            number(&m["levels"]["volumes"][*id])
                        };
                        let current = (raw / 255.0 * 100.0).round();
                        let mut value = *self.draft.get(*id).unwrap_or(&current);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(64.0, 205.0), Sense::click_and_drag());
                        let p = ui.painter_at(rect);
                        let top = rect.top() + 16.0;
                        let bottom = rect.bottom() - 16.0;
                        for tick in 0..=10 {
                            let y = top + (bottom - top) * tick as f32 / 10.0;
                            p.line_segment(
                                [
                                    Pos2::new(rect.left() + 4.0, y),
                                    Pos2::new(rect.right() - 4.0, y),
                                ],
                                Stroke::new(1.0_f32, RULE),
                            );
                        }
                        p.rect_filled(
                            Rect::from_center_size(rect.center(), Vec2::new(7.0, bottom - top)),
                            0.0,
                            CHASSIS,
                        );
                        let y = bottom - (bottom - top) * value as f32 / 100.0;
                        p.line_segment(
                            [
                                Pos2::new(rect.center().x + 17.0, bottom),
                                Pos2::new(rect.center().x + 17.0, y),
                            ],
                            Stroke::new(3.0_f32, color(index)),
                        );
                        let cap = Rect::from_center_size(
                            Pos2::new(rect.center().x, y),
                            Vec2::new(44.0, 24.0),
                        );
                        p.rect_filled(cap.translate(Vec2::new(0.0, 3.0)), 0.0, CHASSIS);
                        p.rect_filled(cap, 0.0, Color32::from_gray(165));
                        for offset in [-5.0, 0.0, 5.0] {
                            p.line_segment(
                                [
                                    Pos2::new(cap.left() + 4.0, y + offset),
                                    Pos2::new(cap.right() - 4.0, y + offset),
                                ],
                                Stroke::new(1.0_f32, Color32::from_gray(70)),
                            );
                        }
                        if response.dragged() || response.clicked() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                value = ((bottom - pos.y) / (bottom - top) * 100.0)
                                    .round()
                                    .clamp(0.0, 100.0)
                                    as f64;
                                self.draft.insert(id.to_string(), value);
                            }
                        }
                        if response.drag_stopped() || response.clicked() {
                            self.command(
                                volume_command,
                                json!([id, (value * 255.0 / 100.0).round() as u8]),
                            );
                            self.draft.remove(*id);
                        }
                        if response.has_focus() {
                            let step = ui.input(|i| {
                                if i.key_pressed(egui::Key::ArrowUp)
                                    || i.key_pressed(egui::Key::ArrowRight)
                                {
                                    1.0
                                } else if i.key_pressed(egui::Key::ArrowDown)
                                    || i.key_pressed(egui::Key::ArrowLeft)
                                {
                                    -1.0
                                } else {
                                    0.0
                                }
                            });
                            if step != 0.0 {
                                self.command(
                                    volume_command,
                                    json!([
                                        id,
                                        ((current + step).clamp(0.0, 100.0) * 255.0 / 100.0).round()
                                            as u8
                                    ]),
                                );
                            }
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::slider(true, current, format!("Volume {label}"))
                        });
                        if ui
                            .add(
                                egui::DragValue::new(&mut value)
                                    .range(0.0..=100.0)
                                    .suffix(" %"),
                            )
                            .changed()
                        {
                            self.command(
                                volume_command,
                                json!([id, (value * 255.0 / 100.0).round() as u8]),
                            );
                        }
                        if let Some(fader) = assigned {
                            let mute = &m["fader_status"][fader];
                            let muted = text(&mute["mute_state"]) != "Unmuted";
                            if button(ui, if muted { "Rétablir" } else { "Couper" }, muted)
                                .clicked()
                            {
                                let state = if muted {
                                    "Unmuted"
                                } else if text(&mute["mute_type"]) == "All" {
                                    "MutedToAll"
                                } else {
                                    "MutedToX"
                                };
                                self.command("SetFaderMuteState", json!([fader, state]));
                            }
                            ui.label(
                                RichText::new(mute_name(&text(&mute["mute_type"])))
                                    .color(MUTED)
                                    .size(12.0),
                            );
                        } else {
                            ui.label(RichText::new("Sans fader").color(MUTED).size(12.0));
                        }
                        if dual {
                            let linked = m["levels"]["submix"]["inputs"][*id]["linked"]
                                .as_bool()
                                .unwrap_or(false);
                            if button(ui, if linked { "Liés" } else { "Lier A/B" }, linked)
                                .clicked()
                            {
                                self.command("SetSubMixLinked", json!([id, !linked]));
                            }
                        }
                    },
                );
            }
            if show_device {
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(250.0, 400.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.heading(if mini { "GoXLR Mini" } else { "GoXLR Original" });
                        self.device(ui, m);
                        if button(ui, "Configurer les faders", false).clicked() {
                            self.tab = 2;
                            self.subtab = 0;
                        }
                    },
                );
            }
        });
    }
    fn configuration(&mut self, ui: &mut egui::Ui, m: &Value) {
        tabs(ui, &mut self.subtab, &["Faders", "Bouton micro"]);
        ui.columns(2,|columns|{
            self.device(&mut columns[0],m);columns[0].horizontal(|ui|{for(index,id)in FADERS.iter().enumerate(){if ui.selectable_label(self.fader==index,format!("{id} · {}",name(&text(&m["fader_status"][*id]["channel"])))).clicked(){self.fader=index;}}});
            let ui=&mut columns[1];
            if self.subtab==0{let id=FADERS[self.fader];let f=&m["fader_status"][id];ui.heading(format!("Fader {id}"));let mut sources:Vec<_>=CHANNELS.iter().filter(|(id,_)|*id!="Sample"||text(&m["hardware"]["device_type"])!="Mini").map(|(id,name)|(id.to_string(),name.to_string())).collect();sources.extend([("Headphones".into(),"Casque".into()),("LineOut".into(),"Sortie ligne".into())]);if let Some(next)=combo(ui,"fader-source","Source contrôlée",&text(&f["channel"]),&sources){self.command("SetFader",json!([id,next]));}
                let options=mute_options(&text(&f["channel"]));if let Some(next)=combo(ui,"mute-target","Coupure courte",&text(&f["mute_type"]),&options){self.command("SetFaderMuteFunction",json!([id,next]));}
                ui.horizontal(|ui|{if button(ui,"Appui court",false).clicked(){let next=if text(&f["mute_state"])!="Unmuted"{"Unmuted"}else if text(&f["mute_type"])=="All"{"MutedToAll"}else{"MutedToX"};self.command("SetFaderMuteState",json!([id,next]));}if button(ui,"Appui long",false).clicked(){self.command("SetFaderMuteState",json!([id,"MutedToAll"]));}});
                ui.separator();ui.label("Audio Windows");let channel=text(&f["channel"]);ui.label(if channel=="Mic"{"Choisissez l’entrée GoXLR dans votre application pour le microphone.".into()}else if ["Console","LineIn"].contains(&channel.as_str()){"Cette source correspond à une prise physique du GoXLR.".into()}else{format!("Dans le mélangeur Windows, choisissez la sortie GoXLR « {} » pour votre application.",name(&channel))});if button(ui,"Mélangeur Windows",false).clicked(){open_windows("ms-settings:apps-volume");}
            }else{let cough=&m["cough_button"];ui.heading("Bouton micro");if let Some(next)=combo(ui,"cough-mode","Comportement",if cough["is_toggle"].as_bool().unwrap_or(true){"toggle"}else{"hold"},&[("toggle".into(),"Appuyer pour couper / rétablir".into()),("hold".into(),"Maintenir pour couper".into())]){self.command("SetCoughIsHold",json!(next=="hold"));}if let Some(next)=combo(ui,"cough-target","Couper vers",&text(&cough["mute_type"]),&mute_options("Mic")){self.command("SetCoughMuteFunction",json!(next));}if button(ui,if text(&cough["state"])=="Unmuted"{"Couper le micro"}else{"Rétablir le micro"},text(&cough["state"])!="Unmuted").clicked(){self.command("SetCoughMuteState",json!(if text(&cough["state"])=="Unmuted"{if text(&cough["mute_type"])=="All"{"MutedToAll"}else{"MutedToX"}}else{"Unmuted"}));}ui.label("Le réglage Maintenir s’applique au bouton physique.");}
        });
    }
    fn routing(&mut self, ui: &mut egui::Ui, m: &Value) {
        let mini = text(&m["hardware"]["device_type"]) == "Mini";
        let count = if mini { 7 } else { 8 };
        let outputs: Vec<_> = OUTPUTS
            .iter()
            .filter(|(id, _)| *id != "Sampler" || !mini)
            .collect();
        egui::Grid::new("route-matrix").min_col_width((ui.available_width()/(outputs.len()+1)as f32-14.0).max(90.0)).spacing([8.0,4.0]).show(ui,|ui|{
            ui.label("Sources");for(_,label)in &outputs{ui.label(RichText::new(*label).strong());}ui.end_row();
            for(index,(channel,label))in CHANNELS.iter().take(count).enumerate(){let input=if *channel=="Mic"{"Microphone"}else{channel};ui.label(RichText::new(*label).color(color(index)));for(output,_)in &outputs{let connected=m["router"][input][*output].as_bool().unwrap_or(false);let allowed=!(input=="Chat"&&*output=="ChatMic");let response=ui.add_enabled(allowed,egui::Button::new(if !allowed{"×"}else if connected{""}else{"·"}).fill(if connected{Color32::from_rgb(68,55,27)}else{PANEL}).min_size(Vec2::new(46.0,40.0)));if connected{let center=response.rect.center();ui.painter().line_segment([center+Vec2::new(-6.0,0.0),center+Vec2::new(-1.0,5.0)],Stroke::new(2.0_f32,GOLD));ui.painter().line_segment([center+Vec2::new(-1.0,5.0),center+Vec2::new(8.0,-5.0)],Stroke::new(2.0_f32,GOLD));}if response.clicked(){self.command("SetRouter",json!([input,output,!connected]));}if !allowed{response.on_hover_text("Le chat ne peut pas revenir vers sa propre entrée : cela renverrait vos interlocuteurs vers eux-mêmes.");}}ui.end_row();}
        });
        ui.add_space(14.0);
        let choices: Vec<_> = outputs
            .iter()
            .map(|(id, label)| (id.to_string(), label.to_string()))
            .collect();
        if let Some(next) = combo(
            ui,
            "routing-monitor",
            "Écoute au casque",
            &text(&m["levels"]["output_monitor"]),
            &choices,
        ) {
            self.command("SetMonitorMix", json!(next));
        }
        ui.label(RichText::new("Une coche est une liaison configurée ; une coupure de fader peut temporairement la bloquer.").color(MUTED));
    }
    fn lighting(&mut self, ui: &mut egui::Ui, m: &Value) {
        tabs(
            ui,
            &mut self.light_group,
            &["Faders", "Boutons", "Général", "Encodeurs", "Banques"],
        );
        let keys = ["faders", "buttons", "simple", "encoders", "sampler"];
        let group = keys[self.light_group];
        let entries = m["lighting"][group].as_object();
        let Some(entries) = entries else {
            ui.label("Zone indisponible sur cet appareil.");
            return;
        };
        if entries.is_empty() {
            ui.label("Zone indisponible sur cet appareil.");
            return;
        }
        let mut names: Vec<_> = entries.keys().cloned().collect();
        names.sort();
        self.light_index = self.light_index.min(names.len() - 1);
        let id = names[self.light_index].clone();
        let options: Vec<_> = names.iter().map(|id| (id.clone(), zone_name(id))).collect();
        if let Some(next) = combo(ui, "lighting-zone", "Zone", &id, &options) {
            self.light_index = names.iter().position(|id| id == &next).unwrap_or(0);
            return;
        }
        let light = &entries[&id];
        let colours = if light.get("colours").is_some() {
            &light["colours"]
        } else {
            light
        };
        ui.columns(2, |columns| {
            self.device(&mut columns[0], m);
            let ui = &mut columns[1];
            let mut values = vec![];
            let mut changed = false;
            for (index, key) in ["colour_one", "colour_two", "colour_three"]
                .iter()
                .enumerate()
            {
                if let Some(hex) = colours[*key].as_str() {
                    let mut rgb = parse_color(hex);
                    ui.horizontal(|ui| {
                        ui.label(format!("Couleur {}", index + 1));
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            changed = true;
                        }
                        ui.label(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                    });
                    values.push(format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                }
            }
            if changed {
                match group {
                    "faders" => self.command("SetFaderColours", json!([id, values[0], values[1]])),
                    "buttons" => {
                        self.command("SetButtonColours", json!([id, values[0], values.get(1)]))
                    }
                    "simple" => self.command("SetSimpleColour", json!([id, values[0]])),
                    "encoders" => self.command(
                        "SetEncoderColour",
                        json!([id, values[0], values[1], values[2]]),
                    ),
                    "sampler" => self.command(
                        "SetSampleColour",
                        json!([id, values[0], values[1], values[2]]),
                    ),
                    _ => {}
                }
            }
            if group == "faders" {
                let options = [
                    ("TwoColour", "Deux couleurs"),
                    ("Gradient", "Dégradé"),
                    ("Meter", "Niveau"),
                    ("GradientMeter", "Dégradé et niveau"),
                ]
                .map(|(id, label)| (id.into(), label.into()));
                if let Some(next) = combo(
                    ui,
                    "light-style",
                    "Affichage",
                    &text(&light["style"]),
                    &options,
                ) {
                    self.command("SetFaderDisplayStyle", json!([id, next]));
                }
            }
            if group == "buttons" || group == "sampler" {
                let options = [
                    ("Dimmed", "Atténué"),
                    ("Colour2", "Deuxième couleur"),
                    ("DimmedColour2", "Deuxième couleur atténuée"),
                ]
                .map(|(id, label)| (id.into(), label.into()));
                if let Some(next) = combo(
                    ui,
                    "off-style",
                    "État éteint",
                    &text(&light["off_style"]),
                    &options,
                ) {
                    self.command(
                        if group == "buttons" {
                            "SetButtonOffStyle"
                        } else {
                            "SetSampleOffStyle"
                        },
                        json!([id, next]),
                    );
                }
            }
        });
    }
}
fn mute_name(id: &str) -> &str {
    match id {
        "All" => "Tout",
        "ToStream" => "Stream",
        "ToVoiceChat" => "Chat vocal",
        "ToPhones" => "Casque",
        "ToLineOut" => "Sortie ligne",
        "ToStream2" => "Stream 2",
        "ToStreams" => "Streams",
        _ => id,
    }
}
fn mute_options(channel: &str) -> Vec<(String, String)> {
    [
        "All",
        "ToStream",
        "ToVoiceChat",
        "ToPhones",
        "ToLineOut",
        "ToStream2",
        "ToStreams",
    ]
    .iter()
    .filter(|target| {
        (!["Headphones", "LineOut"].contains(&channel) || **target == "All")
            && !(channel == "Chat" && **target == "ToVoiceChat")
    })
    .map(|id| (id.to_string(), mute_name(id).to_string()))
    .collect()
}
fn parse_color(value: &str) -> [u8; 3] {
    let hex = value.trim_start_matches('#');
    if hex.len() != 6 {
        return [0, 0, 0];
    }
    std::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0))
}
fn zone_name(value: &str) -> String {
    match value {
        "Fader1Mute" => "Coupure A",
        "Fader2Mute" => "Coupure B",
        "Fader3Mute" => "Coupure C",
        "Fader4Mute" => "Coupure D",
        "Cough" => "Bouton micro",
        "Bleep" => "Bip",
        "Global" => "Couleur générale",
        "Accent" => "Logo",
        _ => value,
    }
    .to_string()
}
fn open_windows(target: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe")
            .arg(target)
            .creation_flags(0x08000000)
            .spawn();
    }
    #[cfg(not(windows))]
    let _ = target;
}

impl Studio {
    fn effects(&mut self, ui: &mut egui::Ui, m: &Value, status: &Value) {
        let fx = &m["effects"];
        if fx.is_null() {
            ui.label("Les effets ne sont pas disponibles sur cet appareil.");
            return;
        }
        ui.horizontal(|ui| {
            for index in 1..=6 {
                let id = format!("Preset{index}");
                if ui
                    .selectable_label(
                        text(&fx["active_preset"]) == id,
                        text(&fx["preset_names"][&id]),
                    )
                    .clicked()
                {
                    self.command("SetActiveEffectPreset", json!(id));
                }
            }
        });
        self.toggle(
            ui,
            "Effets actifs",
            fx["is_enabled"].as_bool().unwrap_or(false),
            "SetFXEnabled",
        );
        let schema: Value = serde_json::from_str(include_str!("../assets/effects.json")).unwrap();
        let effects = schema.as_array().unwrap();
        self.effect = self.effect.min(effects.len() - 1);
        ui.horizontal(|ui| {
            for (index, effect) in effects.iter().enumerate() {
                if ui
                    .selectable_label(self.effect == index, text(&effect["name"]))
                    .clicked()
                {
                    self.effect = index;
                    self.subtab = 0;
                }
            }
        });
        let effect = &effects[self.effect];
        let id = text(&effect["id"]);
        let current = &fx["current"][&id];
        ui.add_space(8.0);
        if let Some(toggle) = effect["toggle"].as_str() {
            self.toggle(
                ui,
                "Actif",
                current["is_enabled"].as_bool().unwrap_or(false),
                toggle,
            );
        }
        let styles: Vec<_> = effect["styles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (text(&v[0]), text(&v[1])))
            .collect();
        if let Some(next) = combo(
            ui,
            "effect-style",
            "Style",
            &text(&current["style"]),
            &styles,
        ) {
            self.command(effect["styleCommand"].as_str().unwrap(), json!(next));
        }
        let pages: Vec<Vec<&Value>> = effect["params"]
            .as_array()
            .unwrap()
            .chunks(4)
            .map(|p| p.iter().collect())
            .collect();
        if pages.len() > 1 {
            let labels: Vec<String> = (0..pages.len())
                .map(|i| {
                    if i == 0 {
                        "Essentiel".into()
                    } else {
                        format!("Détails {i}")
                    }
                })
                .collect();
            let labels: Vec<_> = labels.iter().map(|s| s.as_str()).collect();
            tabs(ui, &mut self.subtab, &labels);
        }
        self.subtab = self.subtab.min(pages.len() - 1);
        for param in &pages[self.subtab] {
            let field = text(&param["field"]);
            let mut min = number(&param["min"]);
            let mut max = number(&param["max"]);
            if id == "pitch" && field == "amount" && text(&current["style"]) == "Wide" {
                min = -24.;
                max = 24.;
            }
            if id == "gender" && field == "amount" {
                max = match text(&current["style"]).as_str() {
                    "Wide" => 50.,
                    "Medium" => 25.,
                    _ => 12.,
                };
                min = -max;
            }
            let enabled = !(id == "echo"
                && ((field == "tempo" && text(&current["style"]) == "ClassicSlap")
                    || ((field == "delay_left" || field == "delay_right")
                        && text(&current["style"]) != "ClassicSlap")));
            ui.add_enabled_ui(enabled, |ui| {
                let prefix = param["prefix"].as_array().cloned().unwrap_or_default();
                self.parameter(
                    ui,
                    &text(&param["label"]),
                    number(&current[&field]),
                    min,
                    max,
                    &text(&param["unit"]),
                    &text(&param["command"]),
                    &prefix,
                )
            });
        }
        if id == "hard_tune" {
            let sources = [
                ("All", "Toutes les sources"),
                ("Music", "Musique"),
                ("Game", "Jeu"),
                ("LineIn", "Entrée ligne"),
                ("System", "Système"),
            ]
            .map(|(id, label)| (id.into(), label.into()));
            if let Some(next) = combo(
                ui,
                "hardtune-source",
                "Référence musicale",
                &text(&current["source"]),
                &sources,
            ) {
                self.command("SetHardTuneSource", json!(next));
            }
        }
        ui.horizontal(|ui| {
            if button(ui, "Renommer la mémoire", false).clicked() {
                self.copy_name = text(&fx["preset_names"][text(&fx["active_preset"])]);
                self.rename_effect = true;
                self.copy_open = true;
            }
            if button(ui, "Enregistrer le preset", false).clicked() {
                self.command("SaveActivePreset", json!([]));
            }
            let names: Vec<_> = status["files"]["presets"]
                .as_array()
                .map(|v| v.iter().map(|v| (text(v), text(v))).collect())
                .unwrap_or_default();
            let mut choices = vec![("".into(), "Charger un preset".into())];
            choices.extend(names);
            if let Some(next) = combo(ui, "preset-library", "", "", &choices) {
                self.load_profile = Some((false, format!("@effect:{next}")));
            }
        });
    }
    fn sampler(&mut self, ui: &mut egui::Ui, m: &Value, status: &Value) {
        let sampler = &m["sampler"];
        if sampler.is_null() {
            ui.label("L’échantillonneur n’est pas disponible sur cet appareil.");
            return;
        }
        tabs(ui, &mut self.subtab, &["Pads", "Bibliothèque", "Découpe"]);
        let bank = text(&sampler["active_bank"]);
        self.bank = bank.clone();
        ui.horizontal(|ui| {
            for id in ["A", "B", "C"] {
                if ui
                    .selectable_label(bank == id, format!("Banque {id}"))
                    .clicked()
                {
                    self.command("SetActiveSamplerBank", json!(id));
                }
            }
            if button(ui, "Arrêter tout", false).clicked() {
                if let Some(banks) = sampler["banks"].as_object() {
                    for (b, pads) in banks {
                        if let Some(pads) = pads.as_object() {
                            for (pad, data) in pads {
                                if data["is_playing"].as_bool().unwrap_or(false) {
                                    self.command("StopSamplePlayback", json!([b, pad]));
                                }
                            }
                        }
                    }
                }
            }
        });
        let pad = &sampler["banks"][&bank][&self.pad];
        match self.subtab {
            0 => {
                for (row, pads) in [["TopLeft", "TopRight"], ["BottomLeft", "BottomRight"]]
                    .iter()
                    .enumerate()
                {
                    ui.horizontal(|ui| {
                        for (index, id) in pads.iter().enumerate() {
                            let current = &sampler["banks"][&bank][*id];
                            ui.group(|ui| {
                                ui.set_min_width(270.0);
                                let samples =
                                    current["samples"].as_array().map(|v| v.len()).unwrap_or(0);
                                if button(
                                    ui,
                                    &format!("{}{} · {} sons", bank, row * 2 + index + 1, samples),
                                    self.pad == *id,
                                )
                                .clicked()
                                {
                                    self.pad = id.to_string();
                                }
                                let playing = current["is_playing"].as_bool().unwrap_or(false);
                                ui.add_enabled_ui(samples > 0, |ui| {
                                    if button(ui, if playing { "Arrêter" } else { "Lire" }, playing)
                                        .clicked()
                                    {
                                        self.command(
                                            if playing {
                                                "StopSamplePlayback"
                                            } else {
                                                "PlayNextSample"
                                            },
                                            json!([bank, id]),
                                        );
                                    }
                                });
                            });
                        }
                    });
                }
                let choices = [
                    ("PlayNext", "Lire le suivant"),
                    ("PlayStop", "Lire / arrêter"),
                    ("PlayFade", "Lire / fondre"),
                    ("StopOnRelease", "Maintenir sur le pad physique"),
                    ("FadeOnRelease", "Maintenir / fondre"),
                    ("Loop", "Boucler"),
                ]
                .map(|(id, label)| (id.into(), label.into()));
                if let Some(next) = combo(
                    ui,
                    "sampler-mode",
                    "Comportement du pad sélectionné",
                    &text(&pad["function"]),
                    &choices,
                ) {
                    self.command("SetSamplerFunction", json!([bank, self.pad, next]));
                }
            }
            1 => {
                ui.label(format!("Affecter au pad {} · {}", bank, self.pad));
                let samples: Vec<_> = status["files"]["samples"]
                    .as_object()
                    .map(|files| {
                        files
                            .iter()
                            .map(|(id, value)| (id.clone(), text(&value["name"])))
                            .collect()
                    })
                    .unwrap_or_default();
                if samples.is_empty() {
                    ui.label("Aucun sample dans le dossier du moteur.");
                }
                let page_count = samples.len().max(1).div_ceil(6);
                self.library_page = self.library_page.min(page_count - 1);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.library_page > 0, egui::Button::new("Précédent"))
                        .clicked()
                    {
                        self.library_page -= 1;
                    }
                    ui.label(format!("Page {} / {}", self.library_page + 1, page_count));
                    if ui
                        .add_enabled(
                            self.library_page + 1 < page_count,
                            egui::Button::new("Suivant"),
                        )
                        .clicked()
                    {
                        self.library_page += 1;
                    }
                });
                for (id, label) in samples.iter().skip(self.library_page * 6).take(6) {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        if button(ui, "Affecter", false).clicked() {
                            self.command("AddSample", json!([bank, self.pad, id]));
                        }
                    });
                }
                if button(ui, "Ouvrir le dossier Samples", false).clicked() {
                    self.daemon("OpenPath", json!("Samples"));
                }
            }
            _ => {
                let samples = pad["samples"].as_array().cloned().unwrap_or_default();
                if samples.is_empty() {
                    ui.label("Ce pad est vide.");
                }
                if !samples.is_empty() {
                    self.sample_index = self.sample_index.min(samples.len() - 1);
                    let choices: Vec<_> = samples
                        .iter()
                        .enumerate()
                        .map(|(i, sample)| {
                            (
                                i.to_string(),
                                format!("{} · {}", i + 1, text(&sample["name"])),
                            )
                        })
                        .collect();
                    if let Some(next) = combo(
                        ui,
                        "sample-index",
                        "Son affecté",
                        &self.sample_index.to_string(),
                        &choices,
                    ) {
                        self.sample_index = next.parse().unwrap_or(0);
                    }
                    let index = self.sample_index;
                    let sample = &samples[index];
                    ui.label(RichText::new(text(&sample["name"])).strong());
                    self.parameter(
                        ui,
                        "Début",
                        number(&sample["start_pct"]),
                        0.,
                        number(&sample["stop_pct"]) - 1.,
                        "%",
                        "SetSampleStartPercent",
                        &[json!(bank), json!(self.pad), json!(index)],
                    );
                    self.parameter(
                        ui,
                        "Fin",
                        number(&sample["stop_pct"]),
                        number(&sample["start_pct"]) + 1.,
                        100.,
                        "%",
                        "SetSampleStopPercent",
                        &[json!(bank), json!(self.pad), json!(index)],
                    );
                    ui.horizontal(|ui| {
                        if button(ui, "Lire le segment", false).clicked() {
                            self.command("PlaySampleByIndex", json!([bank, self.pad, index]));
                        }
                        if button(ui, "Retirer", false).clicked() {
                            self.command("RemoveSampleByIndex", json!([bank, self.pad, index]));
                        }
                    });
                }
            }
        }
    }
    fn system(&mut self, ui: &mut egui::Ui, m: &Value, status: &Value) {
        tabs(
            ui,
            &mut self.subtab,
            &["Appareil", "Application", "Dossiers", "Informations"],
        );
        match self.subtab {
            0 => {
                self.toggle(
                    ui,
                    "Verrouiller les faders physiques",
                    m["settings"]["lock_faders"].as_bool().unwrap_or(false),
                    "SetLockFaders",
                );
                self.toggle(
                    ui,
                    "Couper aussi le retour micro lors de la coupure Chat",
                    m["settings"]["vc_mute_also_mute_cm"]
                        .as_bool()
                        .unwrap_or(false),
                    "SetVCMuteAlsoMuteCM",
                );
                self.parameter(
                    ui,
                    "Durée d’un appui long",
                    number(&m["settings"]["mute_hold_duration"]),
                    100.,
                    5000.,
                    "ms",
                    "SetMuteHoldDuration",
                    &[],
                );
                if text(&m["hardware"]["device_type"]) != "Mini" {
                    self.toggle(
                        ui,
                        "Écouter la voix avec ses effets",
                        m["settings"]["enable_monitor_with_fx"]
                            .as_bool()
                            .unwrap_or(false),
                        "SetMonitorWithFx",
                    );
                    self.parameter(
                        ui,
                        "Fondu des samples",
                        number(&m["settings"]["fade_duration"]),
                        0.,
                        5000.,
                        "ms",
                        "SetSamplerFadeDuration",
                        &[],
                    );
                }
            }
            1 => {
                ui.heading("Interface native Rust");
                ui.label("Fenêtre et contrôles dessinés par egui. Aucun navigateur ni WebView.");
                let options = ["Off", "Error", "Warn", "Info", "Debug", "Trace"]
                    .map(|id| (id.to_string(), id.to_string()));
                if let Some(next) = combo(
                    ui,
                    "log-level",
                    "Niveau des journaux",
                    &text(&status["config"]["log_level"]),
                    &options,
                ) {
                    self.daemon("SetLogLevel", json!(next));
                }
            }
            2 => {
                for (id, label) in [
                    ("Profiles", "Profils de mixage"),
                    ("MicProfiles", "Profils micro"),
                    ("Samples", "Samples"),
                    ("Presets", "Presets d’effets"),
                    ("Logs", "Journaux"),
                ] {
                    if button(ui, label, false).clicked() {
                        self.daemon("OpenPath", json!(id));
                    }
                }
            }
            _ => {
                egui::Grid::new("system-info")
                    .spacing([30.0, 18.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                "Appareil",
                                format!(
                                    "GoXLR {}",
                                    if text(&m["hardware"]["device_type"]) == "Mini" {
                                        "Mini"
                                    } else {
                                        "Original"
                                    }
                                ),
                            ),
                            ("Numéro de série", self.serial.clone()),
                            ("Firmware", version(&m["hardware"]["versions"]["firmware"])),
                            ("Moteur", text(&status["config"]["daemon_version"])),
                            (
                                "Pilote",
                                format!(
                                    "{} · {}",
                                    text(&status["config"]["driver_interface"]["interface"]),
                                    version(&status["config"]["driver_interface"]["version"])
                                ),
                            ),
                            ("Interface", "Rust · egui 0.31.1".into()),
                        ] {
                            ui.label(RichText::new(label).color(MUTED));
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
                ui.label("La mise à jour du firmware n’est pas disponible dans cette version.");
            }
        }
    }
    fn render(&mut self, ctx: &egui::Context) {
        let state = self.backend.snapshot();
        let status = state
            .status
            .unwrap_or_else(|| json!({"mixers":{},"files":{},"config":{}}));
        let devices = status["mixers"].as_object().cloned().unwrap_or_default();
        if !devices.contains_key(&self.serial) {
            self.serial = devices.keys().next().cloned().unwrap_or_default();
        }
        let m = devices.get(&self.serial).cloned().unwrap_or(Value::Null);
        let mini = text(&m["hardware"]["device_type"]) == "Mini";
        if mini && (self.tab == 5 || self.tab == 6) {
            self.tab = 0;
            self.subtab = 0;
        }
        egui::SidePanel::left("navigation")
            .exact_width(190.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(CHASSIS).inner_margin(16.0))
            .show(ctx, |ui| {
                ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::new(136.0, 52.0)));
                ui.add_space(28.0);
                ui.label(
                    RichText::new(if mini {
                        "Console / MINI"
                    } else if m.is_null() {
                        "Console"
                    } else {
                        "Console / ORIGINAL"
                    })
                    .color(MUTED)
                    .size(12.0),
                );
                ui.add_space(12.0);
                for (index, label) in TABS.iter().enumerate() {
                    if mini && (index == 5 || index == 6) {
                        continue;
                    }
                    if button(ui, label, self.tab == index).clicked() {
                        self.tab = index;
                        self.subtab = 0;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(RichText::new("Rust natif").color(MUTED).size(12.0));
                });
            });
        egui::TopBottomPanel::top("toolbar")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let options: Vec<_> = devices
                        .iter()
                        .map(|(id, m)| {
                            (
                                id.clone(),
                                format!(
                                    "GoXLR {}",
                                    if text(&m["hardware"]["device_type"]) == "Mini" {
                                        "Mini"
                                    } else {
                                        "Original"
                                    }
                                ),
                            )
                        })
                        .collect();
                    if let Some(next) = combo(ui, "device", "", &self.serial, &options) {
                        self.serial = next;
                    }
                    ui.label(
                        RichText::new(if state.connected {
                            self.serial.as_str()
                        } else {
                            "Moteur déconnecté"
                        })
                        .size(12.0)
                        .color(if state.connected {
                            MUTED
                        } else {
                            Color32::LIGHT_RED
                        }),
                    );
                    if !m.is_null() {
                        let microphone = self.tab == 1;
                        let key = if microphone {
                            "mic_profile_name"
                        } else {
                            "profile_name"
                        };
                        let active = text(&m[key]);
                        let mut choices = vec![(active.clone(), active.clone())];
                        if let Some(names) = status["files"][if microphone {
                            "mic_profiles"
                        } else {
                            "profiles"
                        }]
                        .as_array()
                        {
                            for name in names {
                                let name = text(name);
                                if name != active {
                                    choices.push((name.clone(), name));
                                }
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_sized([42., 38.], egui::Button::new("+"))
                                .on_hover_text("Enregistrer sous un nouveau nom")
                                .clicked()
                            {
                                self.copy_name.clear();
                                self.rename_effect = false;
                                self.copy_open = true;
                            }
                            if button(ui, "Enregistrer", true).clicked() {
                                self.command(
                                    if microphone {
                                        "SaveMicProfile"
                                    } else {
                                        "SaveProfile"
                                    },
                                    json!([]),
                                );
                            }
                            if let Some(next) = combo(ui, "profile", "", &active, &choices) {
                                self.load_profile = Some((microphone, next));
                            }
                            ui.label(if microphone { "Profil micro" } else { "Profil" });
                        });
                    }
                });
            });
        egui::TopBottomPanel::bottom("status").exact_height(34.0).frame(egui::Frame::new().fill(CHASSIS).inner_margin(8.0)).show(ctx,|ui|{
            let error=if !self.boot_error.is_empty(){self.boot_error.clone()}else if !state.error.is_empty(){state.error.clone()}else{self.notice.clone()};ui.horizontal(|ui|{if !error.is_empty(){ui.add(egui::Label::new(RichText::new(if error.chars().count()>140{"Erreur de connexion ou de commande. Consultez les journaux dans Système."}else{error.as_str()}).color(Color32::LIGHT_RED)).truncate()).on_hover_text(&error);if ui.small_button("Effacer").clicked(){self.notice.clear();self.boot_error.clear();self.backend.state.lock().unwrap().error.clear();}}else{ui.label(RichText::new(&state.confirmation).color(MUTED).size(12.0));}});
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAPER).inner_margin(22.0))
            .show(ctx, |ui| {
                ui.heading(TABS[self.tab]);
                ui.add_space(14.0);
                if m.is_null() {
                    ui.add_space(40.0);
                    ui.heading(if state.connected {
                        "Aucun GoXLR détecté"
                    } else {
                        "Connexion au moteur GoXLR"
                    });
                    ui.label("Branchez le GoXLR et vérifiez le pilote TC-Helicon.");
                    return;
                }
                ui.add_enabled_ui(state.connected, |ui| match self.tab {
                    0 => self.mixer(ui, &m),
                    1 => self.microphone(ui, &m),
                    2 => self.configuration(ui, &m),
                    3 => self.routing(ui, &m),
                    4 => self.lighting(ui, &m),
                    5 => self.effects(ui, &m, &status),
                    6 => self.sampler(ui, &m, &status),
                    _ => self.system(ui, &m, &status),
                });
                let bounds = ui.min_rect();
                let clip = ui.clip_rect();
                self.layout_overflow =
                    bounds.right() > clip.right() + 1.0 || bounds.bottom() > clip.bottom() + 1.0;
            });
        if self.copy_open {
            egui::Window::new("Enregistrer sous")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Nom du profil");
                    ui.text_edit_singleline(&mut self.copy_name);
                    ui.horizontal(|ui| {
                        if ui.button("Annuler").clicked() {
                            self.copy_open = false;
                        }
                        if ui
                            .add_enabled(
                                !self.copy_name.trim().is_empty() && self.copy_name.len() <= 64,
                                egui::Button::new("Enregistrer"),
                            )
                            .clicked()
                        {
                            self.command(
                                if self.rename_effect {
                                    "RenameActivePreset"
                                } else if self.tab == 1 {
                                    "SaveMicProfileAs"
                                } else {
                                    "SaveProfileAs"
                                },
                                json!(self.copy_name.trim()),
                            );
                            self.copy_open = false;
                        }
                    });
                });
        }
        if let Some((microphone, name)) = self.load_profile.clone() {
            egui::Window::new("Charger ce profil ?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Remplacer les réglages actuels par « {} » ?",
                        name.trim_start_matches("@effect:")
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Conserver").clicked() {
                            self.load_profile = None;
                        }
                        if ui.button("Charger").clicked() {
                            if name.starts_with("@effect:") {
                                self.command(
                                    "LoadEffectPreset",
                                    json!(name.trim_start_matches("@effect:")),
                                );
                            } else {
                                self.command(
                                    if microphone {
                                        "LoadMicProfile"
                                    } else {
                                        "LoadProfile"
                                    },
                                    json!([name, true]),
                                );
                            }
                            self.load_profile = None;
                        }
                    });
                });
        }
    }
}
fn version(value: &Value) -> String {
    value
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|v| !v.is_null())
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_else(|| "—".into())
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.layout_overflow = false;
        self.render(ctx);
        if self.smoke {
            let state = self.backend.snapshot();
            if self.started.elapsed() > Duration::from_secs(35) {
                let _ = std::fs::create_dir_all(&self.data);
                let _ = std::fs::write(
                    self.data.join("rust-smoke-error.txt"),
                    "Le matériel n’a pas pu être vérifié.",
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            let events = ctx.input(|input| input.events.clone());
            for event in events {
                if let egui::Event::Screenshot { image, .. } = event {
                    let out = self.data.join("verification");
                    let _ = std::fs::create_dir_all(&out);
                    let bytes: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    let path = out.join(format!("rust-{:02}.png", self.smoke_step));
                    let _ = image::save_buffer(
                        path,
                        &bytes,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    );
                    self.report.push(json!({"tab":TABS[self.tab],"subtab":self.subtab,"size":[image.width(),image.height()],"fits":!self.layout_overflow}));
                    self.smoke_step += 1;
                    self.capture_pending = false;
                    self.last_step = Instant::now();
                    let steps = [
                        (0, 0),
                        (0, 1),
                        (1, 0),
                        (1, 1),
                        (1, 2),
                        (1, 3),
                        (1, 4),
                        (2, 0),
                        (2, 1),
                        (3, 0),
                        (4, 0),
                        (7, 0),
                        (7, 3),
                    ];
                    if self.smoke_step >= steps.len() {
                        let report = json!({"native":"Rust egui","webview":false,"device":self.serial,"audio_commands_sent":0,"screens":self.report});
                        let _ = std::fs::write(
                            out.join("rust-report.json"),
                            serde_json::to_vec_pretty(&report).unwrap(),
                        );
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        (self.tab, self.subtab) = steps[self.smoke_step];
                    }
                }
            }
            if state.connected
                && !self.serial.is_empty()
                && !self.capture_pending
                && self.last_step.elapsed() > Duration::from_millis(250)
            {
                self.capture_pending = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
