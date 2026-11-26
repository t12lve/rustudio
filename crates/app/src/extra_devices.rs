//! Pages des modules communautaires ajoutés par la fusion goxlr-ui : souris Logitech G (LIGHTSPEED),
//! clavier ROCCAT Vulcan 120 AIMO, manettes XInput et cartes réseau. L'état vient du moteur
//! (Event::Extra) ; les commandes passent par post_module. Aucune valeur inventée.
use crate::{
    backend::Device,
    desktop::Page,
    mixer::Mixer,
    ui::{self, Icon},
};
use gpui::{Context, Div, Window, div, prelude::*, px};
use serde_json::{Value, json};

/// Page -> espace API du module.
pub fn extra_module(page: Page) -> Option<&'static str> {
    Some(match page {
        Page::LogiMouse => "/api/modules/logitech-g",
        Page::RoccatKbd => "/api/modules/roccat-vulcan-120-aimo",
        Page::Gamepad => "/api/modules/gamepad-xinput",
        Page::Netcards => "/api/modules/netcard",
        _ => return None,
    })
}

fn module_icon(page: Page) -> Icon {
    match page {
        Page::LogiMouse => Icon::Mouse,
        Page::RoccatKbd => Icon::Keyboard,
        Page::Gamepad => Icon::Game,
        Page::Netcards => Icon::External,
        _ => Icon::Grid,
    }
}

#[derive(Default)]
pub struct ExtraPage {
    pub module: String,
    /// Réponse complète de GET /api/modules/{id}/state.
    pub view: Option<Value>,
    pub notice: Option<(bool, String)>,
}

impl ExtraPage {
    pub fn receive(&mut self, module: &str, view: Option<Value>) {
        self.module = module.to_owned();
        self.view = view;
    }
    fn device(&self) -> Option<Device> {
        self.view.as_ref()?.get("device").cloned().and_then(|v| serde_json::from_value(v).ok())
    }
    fn state(&self) -> Option<&Value> {
        self.view.as_ref()?.get("state")
    }
}

impl Mixer {
    /// Envoie une commande à un module et avertit quand le moteur a répondu.
    pub(crate) fn extra_command(&mut self, module: &'static str, action: &str, params: Value, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let module = module.to_owned();
        let action = action.to_owned();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.post_module(&module, &action, params) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(v) => {
                        let short: String = v.to_string().chars().take(140).collect();
                        this.extra.notice = Some((true, format!("Confirmé par le moteur : {short}")));
                    }
                    Err(e) => this.extra.notice = Some((false, e)),
                }
                this.client.kick_extra();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn extra_view(&self, page: Page, _window: &Window, cx: &mut Context<Self>) -> Div {
        let Some(module) = extra_module(page) else { return div() };
        let icon = module_icon(page);
        let mut content = div().flex().flex_col().gap(px(14.)).w_full();
        if self.extra.module == module {
            if let Some((ok, text)) = &self.extra.notice {
                content = content.child(ui::notice(*ok, text.clone()));
            }
        }
        let view = self.extra.view.as_ref().filter(|_| self.extra.module == module);
        let device = view.and_then(|v| v.get("device").cloned()).and_then(|v| serde_json::from_value::<Device>(v).ok());
        let status = device.as_ref().map(|d| d.status.as_str()).unwrap_or("");
        if status != "ready" && view.is_some() {
            let (title, text) = match status {
                "disconnected" => ("Appareil introuvable", "Branche-le ou active-le ; Rustudio réessaie tout seul."),
                "error" | "blocked" => ("Lecture impossible", "Rustudio réessaie tout seul."),
                _ => ("En attente…", "L’état apparaît après la première lecture."),
            };
            content = content.child(
                ui::card().items_center().py(px(28.)).child(ui::chip(icon, ui::ACCENT, 52.)).child(ui::title(title, 17.)).child(ui::hint(text)),
            );
        }
        let refresh = ui::choice("extra-refresh", Some(Icon::Refresh), "Actualiser", false).on_click(cx.listener(|this, _, _, cx| {
            this.client.kick_extra();
            cx.notify();
        }));
        content = content.child(div().flex().justify_end().child(refresh));
        if let Some(state) = view.and_then(|v| v.get("state")) {
            content = content.child(self.extra_body(page, module, state, cx));
        }
        content
    }

    fn extra_body(&self, page: Page, module: &'static str, state: &Value, cx: &mut Context<Self>) -> Div {
        match page {
            Page::LogiMouse => self.logi_body(module, state, cx),
            Page::RoccatKbd => self.roccat_body(module, state, cx),
            Page::Gamepad => self.gamepad_body(module, state, cx),
            Page::Netcards => self.netcard_body(module, state, cx),
            _ => div(),
        }
    }

    fn facts_card(title: &'static str, glyph: Icon, state: &Value) -> Div {
        let mut card = ui::card().flex().flex_col().gap(px(8.)).child(ui::card_title(Some((glyph, ui::ACCENT)), title, None));
        if let Some(obj) = state.as_object() {
            let mut first = true;
            for (k, v) in obj {
                let val = match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "—".into(),
                    other => other.to_string(),
                };
                card = card.child(ui::list_row(None, k.clone(), Some(val.into()), ui::tag("lu"), first));
                first = false;
            }
        }
        card
    }

    fn logi_body(&self, module: &'static str, state: &Value, cx: &mut Context<Self>) -> Div {
        let dpi = state.get("dpi").and_then(|v| v.as_u64()).unwrap_or(0);
        let polling = state.get("polling_hz").and_then(|v| v.as_u64()).unwrap_or(0);
        let dpi_list: Vec<u64> = state.get("dpi_list").and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|v| v.as_u64()).collect();
        let battery = state.get("battery").and_then(|v| v.as_u64());
        let charging = state.get("charging").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut facts = serde_json::Map::new();
        if let Some(n) = state.get("name") { facts.insert("name".into(), n.clone()); }
        if let Some(f) = state.get("firmware") { facts.insert("firmware".into(), f.clone()); }
        facts.insert("battery".into(), json!(battery.map(|b| format!("{b} %")).unwrap_or_else(|| if charging { "en charge".into() } else { "inconnue".into() })));
        facts.insert("dpi_max".into(), state.get("dpi_max").cloned().unwrap_or(Value::Null));
        let mut body = div().flex().flex_col().gap(px(14.));
        body = body.child(Self::facts_card("Souris", Icon::Mouse, &Value::Object(facts)));
        let mut dpi_card = ui::card().flex().flex_col().gap(px(10.)).child(ui::card_title(Some((Icon::Mouse, ui::ACCENT)), "Sensibilité (DPI)", Some(format!("actuel : {dpi}").into())));
        let mut row = div().flex().flex_wrap().gap(px(8.));
        for d in dpi_list {
            let selected = d == dpi;
            row = row.child(ui::choice(eid(format!("dpi-{d}")), None, d.to_string(), selected).on_click(cx.listener(move |this, _, _, cx| {
                this.extra_command(module, "setting", json!({"key": "dpi", "value": d}), cx);
            })));
        }
        body = body.child(dpi_card.child(row));
        let mut poll_card = ui::card().flex().flex_col().gap(px(10.)).child(ui::card_title(Some((Icon::Activity, ui::ACCENT)), "Fréquence d’interrogation", Some(format!("actuelle : {polling} Hz").into())));
        let mut row = div().flex().flex_wrap().gap(px(8.));
        for hz in [125u64, 250, 500, 1000] {
            let selected = hz == polling;
            row = row.child(ui::choice(eid(format!("hz-{hz}")), None, format!("{hz} Hz"), selected).on_click(cx.listener(move |this, _, _, cx| {
                this.extra_command(module, "setting", json!({"key": "polling_hz", "value": hz}), cx);
            })));
        }
        body.child(poll_card.child(row))
    }

    fn roccat_body(&self, module: &'static str, state: &Value, cx: &mut Context<Self>) -> Div {
        let effect = state.get("effect").and_then(|v| v.as_str()).unwrap_or("");
        let brightness = state.get("brightness").and_then(|v| v.as_u64()).unwrap_or(0);
        let speed = state.get("speed").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut body = div().flex().flex_col().gap(px(14.));
        body = body.child(Self::facts_card("Éclairage lu", Icon::Sparkles, state));
        let effects = ["off", "wave", "breathe", "reactive", "custom"];
        let mut card = ui::card().flex().flex_col().gap(px(10.)).child(ui::card_title(Some((Icon::Sparkles, ui::ACCENT)), "Effet", Some(format!("actuel : {effect}").into())));
        let mut row = div().flex().flex_wrap().gap(px(8.));
        for e in effects {
            let selected = e == effect;
            row = row.child(ui::choice(eid(format!("fx-{e}")), None, e, selected).on_click(cx.listener(move |this, _, _, cx| {
                this.extra_command(module, "setting", json!({"key": "effect", "value": e}), cx);
            })));
        }
        body = body.child(card.child(row));
        let mut card = ui::card().flex().flex_col().gap(px(10.)).child(ui::card_title(Some((Icon::Sun, ui::ACCENT)), "Luminosité", Some(format!("actuelle : {brightness} %").into())));
        let mut row = div().flex().flex_wrap().gap(px(8.));
        for b in [0u64, 25, 50, 75, 100] {
            let selected = b == brightness;
            row = row.child(ui::choice(eid(format!("br-{b}")), None, format!("{b} %"), selected).on_click(cx.listener(move |this, _, _, cx| {
                this.extra_command(module, "setting", json!({"key": "brightness", "value": b}), cx);
            })));
        }
        body = body.child(card.child(row));
        let mut card = ui::card().flex().flex_col().gap(px(10.)).child(ui::card_title(Some((Icon::Gauge, ui::ACCENT)), "Vitesse", Some(format!("actuelle : {speed}").into())));
        let mut row = div().flex().flex_wrap().gap(px(8.));
        for s in 0u64..=3 {
            let selected = s == speed;
            row = row.child(ui::choice(eid(format!("sp-{s}")), None, format!("{s}"), selected).on_click(cx.listener(move |this, _, _, cx| {
                this.extra_command(module, "setting", json!({"key": "speed", "value": s}), cx);
            })));
        }
        body.child(card.child(row))
    }

    fn gamepad_body(&self, module: &'static str, state: &Value, cx: &mut Context<Self>) -> Div {
        let pads = state.get("pads").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let mut card = ui::card().flex().flex_col().gap(px(8.)).child(ui::card_title(Some((Icon::Game, ui::ACCENT)), "Manettes XInput", Some("4 slots".into())));
        let mut first = true;
        for pad in &pads {
            let index = pad.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let connected = pad.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
            let battery = pad.get("battery").and_then(|v| v.as_u64()).map(|b| format!("{b} %")).unwrap_or_else(|| "—".into());
            let buttons = pad.get("buttons").and_then(|v| v.as_u64()).unwrap_or(0);
            let label = format!("Slot {index} · {}", if connected { format!("connectée · batterie {battery} · {buttons} boutons actifs") } else { "vide".into() });
            let trailing = if connected {
                ui::choice(eid(format!("vib-{index}")), Some(Icon::Activity), "Vibrer 500 ms", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra_command(module, "vibrate", json!({"index": index, "left": 65535, "right": 65535, "ms": 500}), cx);
                    }))
                    .into_any_element()
            } else {
                ui::tag("—").into_any_element()
            };
            card = card.child(ui::list_row(None, label, None, trailing, first));
            first = false;
        }
        div().child(card)
    }

    fn netcard_body(&self, module: &'static str, state: &Value, cx: &mut Context<Self>) -> Div {
        let adapters = state.get("adapters").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let mut card = ui::card().flex().flex_col().gap(px(8.)).child(ui::card_title(Some((Icon::External, ui::ACCENT)), "Cartes réseau", None));
        let mut first = true;
        for a in &adapters {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let status = a.get("status").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let mac = a.get("mac").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let ips = a.get("ips").and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|v| v.as_str().map(str::to_owned)).collect::<Vec<_>>().join(", ");
            let dns = a.get("dns").and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|v| v.as_str().map(str::to_owned)).collect::<Vec<_>>().join(", ");
            let speed = a.get("speed_bps").and_then(|v| v.as_u64()).unwrap_or(0);
            let speed_txt = if speed >= 1_000_000_000 { format!("{} Gb/s", speed as f64 / 1e9) } else if speed > 0 { format!("{} Mb/s", speed / 1_000_000) } else { "vitesse inconnue".into() };
            let sub = format!("{status} · {mac} · {speed_txt} · IP {} · DNS {}", if ips.is_empty() { "—".into() } else { ips }, if dns.is_empty() { "—".into() } else { dns });
            let mut actions = div().flex().gap(px(6.));
            for (label, action, enabled) in [("Activer", "enable", status != "Up"), ("Désactiver", "disable", status == "Up"), ("Renouveler", "renew", status == "Up")] {
                let name = name.clone();
                let action = action.to_owned();
                if enabled {
                    actions = actions.child(ui::choice(eid(format!("net-{action}-{name}")), None, label, false).on_click(cx.listener(move |this, _, _, cx| {
                        this.extra_command(module, &action, json!({"name": name}), cx);
                    })));
                }
            }
            card = card.child(ui::list_row(None, name, Some(sub.into()), actions, first));
            first = false;
        }
        div().child(card)
    }
}

/// Identifiant d'élément nommé dynamique.
fn eid(s: String) -> gpui::ElementId {
    gpui::ElementId::Name(s.into())
}
