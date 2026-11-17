//! The window: one small screen with the status, the MT-32's display, the
//! ROMs in use, and the few settings there are.

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};

use crate::engine::{Cmd, Config, Engine, Status};
use crate::paths::{APP_NAME, Settings};

const BUILT_IN: &str = "Built-in MT-32";
const LCD_BACK: Color32 = Color32::from_rgb(0x9C, 0xB5, 0x3A);
const LCD_TEXT: Color32 = Color32::from_rgb(0x1E, 0x2A, 0x0A);
const LCD_DIM: Color32 = Color32::from_rgb(0x6F, 0x82, 0x2A);
const LED_ON: Color32 = Color32::from_rgb(0xFF, 0x5A, 0x1F);
const LED_OFF: Color32 = Color32::from_rgb(0x5A, 0x2A, 0x1E);
const WARN: Color32 = Color32::from_rgb(0xD0, 0x4A, 0x2A);

pub fn run(mut config: Config, mut settings: Settings) -> Result<(), String> {
    // Things given on the command line win for this run.
    if config.to.is_some() {
        settings.output = config.to.clone();
    } else {
        config.to = settings.output.clone();
    }
    config.roms_chosen = settings.rom_folder.clone();
    config.volume = settings.volume;
    let engine = Engine::start(config)?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([440.0, 340.0])
            .with_min_inner_size([360.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(APP_NAME, options, Box::new(move |_cc| Ok(Box::new(App { engine: Some(engine), settings }))))
        .map_err(|e| e.to_string())
}

struct App {
    engine: Option<Engine>,
    settings: Settings,
}

/// What the person asked for on this frame.
#[derive(Debug, PartialEq)]
pub enum Action {
    ChooseFolder,
    Output(Option<String>),
    Volume { done: bool },
    TestSound,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        let Some(engine) = self.engine.as_ref() else { return };
        let st = engine.status();
        let mut actions = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| actions = draw(ui, &st, &mut self.settings));
        });
        for action in actions {
            match action {
                Action::ChooseFolder => {
                    if let Some(dir) = pick_folder(self.settings.rom_folder.as_ref()) {
                        self.settings.rom_folder = Some(dir.clone());
                        self.settings.save();
                        engine.send(Cmd::ChooseRoms(dir));
                    }
                }
                Action::Output(choice) => {
                    self.settings.output = choice.clone();
                    self.settings.save();
                    engine.send(Cmd::SetOutput(choice));
                }
                Action::Volume { done } => {
                    engine.set_volume(self.settings.volume);
                    if done {
                        self.settings.save();
                    }
                }
                Action::TestSound => engine.send(Cmd::TestSound),
            }
        }
    }

    fn on_exit(&mut self) {
        if let Some(engine) = self.engine.take() {
            engine.stop();
        }
    }
}

fn pick_folder(start: Option<&PathBuf>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title("Choose the folder with your ROM files");
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder()
}

/// Draws the screen for `st`. The volume slider changes `settings` in
/// place; everything else comes back as actions.
pub fn draw(ui: &mut egui::Ui, st: &Status, settings: &mut Settings) -> Vec<Action> {
    let mut actions = Vec::new();
    ui.spacing_mut().item_spacing.y = 8.0;

    let (title, hint) = headline(st);
    ui.label(RichText::new(title).size(22.0).strong());
    ui.label(hint);
    for problem in problems(st) {
        ui.colored_label(WARN, problem);
    }
    ui.add_space(4.0);

    if st.output.is_none() {
        lcd(ui, st);
        ui.add_space(4.0);
    }

    match (&st.roms, &st.output) {
        (_, Some(out)) => {
            ui.label(format!("Playing on {out}. No ROMs needed."));
        }
        (Some(r), None) => {
            ui.label(format!("Using {} ROMs ({})", r.family, r.control));
            ui.label(RichText::new(r.folder.display().to_string()).small().weak());
        }
        (None, None) => {
            if st.roms_missing.is_none() {
                ui.label("Looking for ROMs\u{2026}");
            }
        }
    }
    if st.chosen_empty {
        ui.colored_label(WARN, "The folder you chose has no CM-32L or MT-32 ROMs in it.");
    }
    if ui.button("Choose ROM folder\u{2026}").clicked() {
        actions.push(Action::ChooseFolder);
    }
    ui.add_space(4.0);

    egui::Grid::new("settings").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
        let label = ui.label("Play on");
        let current = settings.output.clone().unwrap_or_else(|| BUILT_IN.to_string());
        let combo =
            egui::ComboBox::from_id_salt("output").selected_text(current.as_str()).width(240.0).show_ui(ui, |ui| {
                if ui.selectable_label(settings.output.is_none(), BUILT_IN).clicked() && settings.output.is_some() {
                    actions.push(Action::Output(None));
                }
                for name in &st.outputs {
                    let this = settings.output.as_ref() == Some(name);
                    if ui.selectable_label(this, name).clicked() && !this {
                        actions.push(Action::Output(Some(name.clone())));
                    }
                }
            });
        combo.response.labelled_by(label.id);
        ui.end_row();

        let label = ui.label("Volume");
        let slider = ui
            .add_enabled(
                settings.output.is_none(),
                egui::Slider::new(&mut settings.volume, 0.0..=1.0).show_value(false),
            )
            .labelled_by(label.id);
        if slider.changed() || slider.drag_stopped() {
            actions.push(Action::Volume { done: !slider.dragged() });
        }
        ui.end_row();
    });
    ui.add_space(4.0);
    let can_play = if st.output.is_some() { st.output_connected } else { st.roms.is_some() };
    let test = ui.add_enabled(can_play, egui::Button::new("Test sound"));
    if test.on_hover_text("Plays a few notes, to check that you can hear the player").clicked() {
        actions.push(Action::TestSound);
    }
    actions
}

fn headline(st: &Status) -> (String, String) {
    if st.output.is_none() && st.roms.is_none() && st.roms_missing.is_some() {
        return (
            "No ROMs yet".into(),
            "Click \u{201C}Choose ROM folder\u{2026}\u{201D} and pick the folder with the ROM files from your CM-32L or MT-32."
                .into(),
        );
    }
    if let Some(name) = &st.input {
        return (format!("Connected to {}", short(name)), "The game's music plays here while it runs.".into());
    }
    ("Waiting for the cart".into(), "Plug the cart's USB-C port into this computer, then start the game.".into())
}

fn problems(st: &Status) -> Vec<String> {
    let mut p = Vec::new();
    if let Some(e) = &st.midi_error {
        p.push(e.clone());
    }
    if st.output.is_none() {
        if let Some(e) = &st.audio_error {
            p.push(format!("No sound output: {e}"));
        }
        if let Some(e) = &st.synth_error {
            p.push(e.clone());
        }
    } else if !st.output_connected {
        p.push(format!("Waiting for {}", st.output.as_deref().unwrap_or("")));
    }
    p
}

// Port names can carry extra words (ALSA adds the client and port numbers).
fn short(name: &str) -> &str {
    name.split(':').next().unwrap_or(name).trim()
}

// The MT-32's 20-character display and its MIDI MESSAGE light.
fn lcd(ui: &mut egui::Ui, st: &Status) {
    let active = st.output.is_none() && st.roms.is_some();
    let text = if active { format!("{:<20}", st.lcd) } else { " ".repeat(20) };
    egui::Frame::new().fill(LCD_BACK).corner_radius(6.0).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(text).monospace().size(22.0).color(if active { LCD_TEXT } else { LCD_DIM }));
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, if active && st.led { LED_ON } else { LED_OFF });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RomSummary;
    use egui_kittest::Harness;

    fn waiting_no_roms() -> Status {
        Status { roms_missing: Some(vec![PathBuf::from("/x")]), ..Default::default() }
    }

    fn connected() -> Status {
        Status {
            input: Some("Parrot Cart MIDI".into()),
            roms: Some(RomSummary {
                family: "CM-32L",
                folder: PathBuf::from("/home/pat/ROMs/Roland"),
                control: "CM-32L/LAPC-I Control v1.02".into(),
            }),
            lcd: "   Cabal 1.0.0git".into(),
            led: true,
            audio: Some("Speakers".into()),
            outputs: vec!["USB MIDI Interface".into()],
            ..Default::default()
        }
    }

    fn forwarding() -> Status {
        Status {
            output: Some("USB MIDI Interface".into()),
            output_connected: false,
            outputs: vec!["USB MIDI Interface".into()],
            ..Default::default()
        }
    }

    #[test]
    fn headlines() {
        assert_eq!(headline(&waiting_no_roms()).0, "No ROMs yet");
        assert_eq!(headline(&connected()).0, "Connected to Parrot Cart MIDI");
        assert_eq!(headline(&forwarding()).0, "Waiting for the cart");
        assert_eq!(problems(&forwarding()), vec!["Waiting for USB MIDI Interface".to_string()]);
        assert_eq!(short("Parrot Cart MIDI:Parrot Cart MIDI MIDI 1 20:0"), "Parrot Cart MIDI");
    }

    #[test]
    fn draws_every_state() {
        for st in [waiting_no_roms(), connected(), forwarding(), Status::default()] {
            let mut settings = Settings::default();
            let mut harness = Harness::builder().with_size([440.0, 340.0]).build_ui(|ui| {
                draw(ui, &st, &mut settings);
            });
            harness.run();
        }
    }

    // Renders the screen to PNG files, to look at without opening a window:
    //   PARROT_SNAPSHOTS=/some/dir cargo test --release -- --ignored screenshots
    #[test]
    #[ignore]
    fn screenshots() {
        let dir = PathBuf::from(std::env::var("PARROT_SNAPSHOTS").expect("PARROT_SNAPSHOTS=dir"));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, st) in [("no-roms", waiting_no_roms()), ("connected", connected()), ("mt32-out", forwarding())] {
            for theme in [egui::Theme::Light, egui::Theme::Dark] {
                let mut settings = Settings { output: st.output.clone(), ..Settings::default() };
                let mut harness =
                    Harness::builder().with_size([440.0, 340.0]).with_pixels_per_point(2.0).with_theme(theme).build_ui(
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                draw(ui, &st, &mut settings);
                            });
                        },
                    );
                harness.run();
                let image = harness.render().expect("render");
                image.save(dir.join(format!("{name}-{theme:?}.png").to_lowercase())).unwrap();
            }
        }
    }
}
