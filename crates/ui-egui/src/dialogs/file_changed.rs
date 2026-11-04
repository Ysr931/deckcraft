//! Eigene Ergänzung im Zweig `eigen`: Die Nachfrage, bevor Speichern eine von außen geänderte
//! Datei ersetzt (siehe `deckcraft_engine::file_stamp`). DeckCraft hat keine Übersetzungen, die
//! Oberfläche ist sonst englisch; diese Warnung steht bewusst auf Deutsch.

use super::{Dialog, SlideApp, Tokens, Ui, json, vec2};

impl SlideApp {
    /// Zeigt die Nachfrage und weckt dafür das Fenster: Sie darf nicht auf die nächste Eingabe warten.
    pub(crate) fn ask_file_changed(&mut self) {
        self.dialog = Some(Dialog::new("fileChanged"));
        if let Some(ctx) = &self.file_changed_ctx {
            let viewport = ctx.viewport_id();
            if ctx.input_for(viewport, |i| i.viewport().visible()) != Some(true) {
                ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::RequestPaintWhileHidden);
            }
            ctx.request_repaint_of(viewport);
            ctx.request_repaint();
        }
    }
}

/// Der Dialog: Unter neuem Namen speichern behält beide Fassungen, Überschreiben ersetzt die
/// Datei, Abbrechen und Esc lassen alles, wie es ist. Gibt zurück, ob er schließen soll.
pub(super) fn file_changed_dialog(app: &mut SlideApp, ui: &mut Ui) -> bool {
    let Some(doc) = app.session.active() else { return true };
    let name = doc.title();
    // Eine PowerPoint-Datei bleibt beim Speichern unter neuem Namen eine PowerPoint-Datei.
    let format = if doc.path.as_deref().is_some_and(|p| p.to_ascii_lowercase().ends_with(".pptx")) { "pptx" } else { "deckcraft" };
    ui.label(egui::RichText::new(format!("„{name}“ wurde inzwischen geändert")).strong());
    ui.label(
        "Die Datei wurde seit dem Öffnen oder letzten Speichern von außen verändert, zum Beispiel auf einem anderen Rechner. Beim Überschreiben ersetzt deine Fassung diese Änderung.",
    );
    ui.label("Unter neuem Namen speichern behält beide Fassungen, dann kannst du sie vergleichen.");
    ui.add_space(8.0);
    let mut done = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let t = Tokens::get(ui.ctx());
            let save_as = egui::Button::new(egui::RichText::new("Unter neuem Namen speichern…").color(t.accent_text)).fill(t.accent);
            if ui.add(save_as.min_size(vec2(72.0, 24.0))).clicked() {
                app.save_as_dialog(format);
                done = true;
            }
            if ui.add(egui::Button::new("Überschreiben").min_size(vec2(72.0, 24.0))).clicked() {
                match app.session.execute("file.save", &json!({"overwrite": true})) {
                    Ok(_) => app.set_status("Saved"),
                    Err(e) => app.set_status(e.to_string()),
                }
                done = true;
            }
            if ui.add(egui::Button::new("Abbrechen").min_size(vec2(72.0, 24.0))).clicked() {
                done = true;
            }
        });
    });
    done
}

#[cfg(test)]
mod tests {
    use deckcraft_engine::Session;
    use egui_kittest::kittest::Queryable;
    use serde_json::json;

    use crate::{Services, SlideApp};

    fn ordner(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("deckcraft-ui-changed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn deck(folien: usize, path: &str) {
        let mut s = Session::with_new();
        for _ in 1..folien {
            s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        }
        s.execute("file.saveAs", &json!({"path": path})).unwrap();
    }

    /// Wie Syncthing: neue Datei daneben schreiben, dann über die alte umbenennen.
    fn von_aussen_ersetzen(path: &str, folien: usize) {
        let tmp = format!("{path}.fremd.deckcraft");
        deck(folien, &tmp);
        std::fs::rename(&tmp, path).unwrap();
    }

    fn folien(path: &str) -> usize {
        let mut s = Session::new();
        s.execute("file.open", &json!({"path": path})).unwrap();
        s.doc().unwrap().doc.slides.len()
    }

    /// Eine geöffnete Präsentation mit neuer Folie, deren Datei inzwischen 5 Folien von außen hat.
    fn veraltet(dir: &std::path::Path) -> (SlideApp, String) {
        let path = dir.join("a.deckcraft").to_string_lossy().into_owned();
        deck(1, &path);
        let mut s = Session::new();
        s.execute("file.open", &json!({"path": path})).unwrap();
        s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        von_aussen_ersetzen(&path, 5);
        (SlideApp::new(s, Services::default()), path)
    }

    fn dialog(a: &SlideApp) -> Option<&str> {
        a.dialog.as_ref().map(|d| d.id.as_str())
    }

    #[test]
    fn speichern_fragt_nach_und_laesst_die_datei_in_ruhe() {
        let dir = ordner("fragt");
        let (mut a, path) = veraltet(&dir);
        a.save();
        assert_eq!(dialog(&a), Some("fileChanged"));
        assert_eq!(folien(&path), 5);
        assert!(a.session.active().unwrap().is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Beim Beenden: „Alle speichern“ stoppt an der Nachfrage, nichts wird ersetzt.
    #[test]
    fn alle_speichern_stoppt_an_der_nachfrage() {
        let dir = ordner("alle");
        let (mut a, path) = veraltet(&dir);
        assert!(!a.save_all());
        assert_eq!(dialog(&a), Some("fileChanged"));
        assert_eq!(folien(&path), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Skripte und Agenten bekommen nie einen Dialog, sondern einen Fehler.
    #[test]
    fn skripte_bekommen_einen_fehler() {
        let dir = ordner("skripte");
        let (mut a, path) = veraltet(&dir);
        let e = a.run("file.save", json!({})).unwrap_err();
        assert!(deckcraft_engine::file_stamp::changed_elsewhere(&e), "{e}");
        assert_eq!(dialog(&a), None);
        assert_eq!(folien(&path), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn fenster(a: SlideApp) -> egui_kittest::Harness<'static, SlideApp> {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            a,
        );
        h.run_steps(4);
        h
    }

    /// Die Warnung wartet nicht auf die nächste Mausbewegung: Das Fenster wird von selbst geweckt.
    #[test]
    fn speichern_weckt_das_fenster_fuer_die_warnung() {
        let dir = ordner("wecken");
        let (a, _) = veraltet(&dir);
        let mut h = fenster(a);
        h.run_steps(10);
        let geweckt = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let merker = geweckt.clone();
        h.ctx.set_request_repaint_callback(move |info| {
            if info.delay.is_zero() {
                merker.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        h.state_mut().save();
        assert_eq!(dialog(h.state()), Some("fileChanged"));
        assert!(geweckt.load(std::sync::atomic::Ordering::Relaxed), "die Warnung muss ohne weitere Eingabe erscheinen");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Der Dialog im echten Fenster: Überschreiben ersetzt die Datei.
    #[test]
    fn der_knopf_ueberschreiben_ersetzt_die_datei() {
        let dir = ordner("fenster-ueberschreiben");
        let (a, path) = veraltet(&dir);
        let mut h = fenster(a);
        h.state_mut().save();
        h.run_steps(3);
        h.get_by_label("Überschreiben").click();
        h.run_steps(3);
        assert_eq!(dialog(h.state()), None);
        assert_eq!(folien(&path), 2);
        assert!(!h.state().session.active().unwrap().is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Der Dialog im echten Fenster: Abbrechen lässt alles, wie es ist.
    #[test]
    fn der_knopf_abbrechen_laesst_alles_wie_es_ist() {
        let dir = ordner("fenster-abbrechen");
        let (a, path) = veraltet(&dir);
        let mut h = fenster(a);
        h.state_mut().save();
        h.run_steps(3);
        h.get_by_label("Abbrechen").click();
        h.run_steps(3);
        assert_eq!(dialog(h.state()), None);
        assert_eq!(folien(&path), 5);
        assert!(h.state().session.active().unwrap().is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Der Dialog im echten Fenster: Ein neuer Name behält beide Fassungen.
    #[test]
    fn der_knopf_neuer_name_behaelt_beide_fassungen() {
        let dir = ordner("fenster-name");
        let (mut a, path) = veraltet(&dir);
        let kopie = dir.join("b.deckcraft").to_string_lossy().into_owned();
        let gewaehlt = kopie.clone();
        a.services.pick_save = Some(Box::new(move |_| Some(gewaehlt.clone())));
        let mut h = fenster(a);
        h.state_mut().save();
        h.run_steps(3);
        h.get_by_label("Unter neuem Namen speichern…").click();
        h.run_steps(3);
        assert_eq!(dialog(h.state()), None);
        assert_eq!(folien(&path), 5, "die Fassung von Mac A bleibt");
        assert_eq!(folien(&kopie), 2, "die eigene Fassung liegt daneben");
        assert_eq!(h.state().session.active().unwrap().path.as_deref(), Some(kopie.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
