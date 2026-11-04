//! Eigene Ergänzung im Zweig `eigen`: Wurde die Datei einer offenen Präsentation inzwischen von
//! außen geändert, zum Beispiel von Syncthing mit der neueren Fassung eines anderen Rechners?
//! Ein normales Speichern würde diese Fassung still ersetzen, und das Sync-Programm verteilt das
//! Ergebnis dann auf alle Rechner. `file.save` verweigert das deshalb, solange niemand mit
//! `"overwrite": true` ausdrücklich zustimmt. Die Oberfläche fragt vorher nach.

/// Was sich eine Präsentation beim Lesen und Schreiben über ihre Datei merkt. Größe und
/// Änderungszeit lassen sich vor jedem Speichern günstig lesen, und Syncthing übernimmt die
/// Änderungszeit des anderen Rechners mit dem Inhalt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: std::time::SystemTime,
}

impl FileStamp {
    /// Ein Stand, der zu keiner Datei passt: Das nächste Speichern fragt in jedem Fall nach.
    pub fn unknown() -> Self {
        FileStamp { len: u64::MAX, modified: std::time::SystemTime::UNIX_EPOCH }
    }
}

/// Die Fehlermeldung, mit der `file.save` ablehnt. [`changed_elsewhere`] erkennt sie wieder.
pub const CHANGED_ELSEWHERE: &str = "The file changed on disk after it was opened or last saved here, so it was not replaced. Save it under another name, or pass \"overwrite\": true.";

/// Der aktuelle Stand der Datei; `None`, wenn sie fehlt oder nicht lesbar ist, und im Browser, wo
/// es keine Dateipfade gibt.
pub fn file_stamp(path: &str) -> Option<FileStamp> {
    let (len, modified) = crate::io::file_len_and_time(path)?;
    Some(FileStamp { len, modified })
}

/// Bedeutet dieser Fehler, dass `file.save` die von außen geänderte Datei nicht ersetzt hat?
pub fn changed_elsewhere(err: &str) -> bool {
    err.contains(CHANGED_ELSEWHERE)
}

impl crate::DocState {
    /// Hat sich die Datei geändert, seit diese Präsentation sie zuletzt gelesen oder geschrieben
    /// hat? Eine gelöschte Datei zählt nicht: Speichern legt sie neu an, dabei geht nichts verloren.
    pub fn file_changed_elsewhere(&self) -> bool {
        let (Some(path), Some(known)) = (&self.path, self.file_stamp) else { return false };
        file_stamp(path).is_some_and(|now| now != known)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{changed_elsewhere, file_stamp};
    use crate::Session;

    fn ordner(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("deckcraft-file-stamp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pfad(dir: &std::path::Path, name: &str) -> String {
        dir.join(name).to_string_lossy().into_owned()
    }

    /// Schreibt eine Präsentation mit dieser Zahl an Folien.
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

    /// Geöffnet und um eine Folie ergänzt.
    fn geoeffnet(path: &str) -> Session {
        let mut s = Session::new();
        s.execute("file.open", &json!({"path": path})).unwrap();
        s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        s
    }

    #[test]
    fn speichern_ersetzt_keine_von_aussen_geaenderte_datei() {
        let dir = ordner("abgelehnt");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let mut s = geoeffnet(&path);
        von_aussen_ersetzen(&path, 5);

        assert!(s.doc().unwrap().file_changed_elsewhere());
        let e = s.execute("file.save", &json!({})).unwrap_err().to_string();
        assert!(changed_elsewhere(&e), "{e}");
        assert_eq!(folien(&path), 5, "die Datei bleibt, wie sie ist");
        assert!(s.doc().unwrap().is_dirty(), "die Präsentation gilt weiter als ungespeichert");
        // Derselbe Pfad, ausdrücklich genannt, ist kein Schlupfloch.
        let e = s.execute("file.save", &json!({"path": path})).unwrap_err().to_string();
        assert!(changed_elsewhere(&e), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nach_der_zustimmung_wird_ueberschrieben_und_nicht_mehr_gefragt() {
        let dir = ordner("zustimmung");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let mut s = geoeffnet(&path);
        von_aussen_ersetzen(&path, 5);

        s.execute("file.save", &json!({"overwrite": true})).unwrap();
        assert_eq!(folien(&path), 2);
        assert!(!s.doc().unwrap().is_dirty());
        assert!(!s.doc().unwrap().file_changed_elsewhere());
        s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        s.execute("file.save", &json!({})).unwrap();
        assert_eq!(folien(&path), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nur die Änderungszeit weicht ab, die Größe ist gleich.
    #[test]
    fn gleiche_groesse_mit_anderer_zeit_zaehlt_als_aenderung() {
        let dir = ordner("zeit");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let s = geoeffnet(&path);
        let vorher = file_stamp(&path).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(file.metadata().unwrap().modified().unwrap() + std::time::Duration::from_secs(5)).unwrap();
        drop(file);
        assert_ne!(file_stamp(&path).unwrap(), vorher);
        assert!(s.doc().unwrap().file_changed_elsewhere());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normales_arbeiten_loest_nichts_aus() {
        let dir = ordner("normal");
        let path = pfad(&dir, "a.deckcraft");
        let mut s = Session::with_new();
        assert!(!s.doc().unwrap().file_changed_elsewhere(), "noch nie gespeichert");
        s.execute("file.save", &json!({"path": path})).unwrap();
        for _ in 0..3 {
            s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
            s.execute("file.save", &json!({})).unwrap();
        }
        assert!(!s.doc().unwrap().file_changed_elsewhere());
        assert_eq!(folien(&path), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ein Mac hat die Datei gelöscht: Speichern legt sie wieder an, verloren geht dabei nichts.
    #[test]
    fn eine_geloeschte_datei_wird_neu_angelegt() {
        let dir = ordner("geloescht");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let mut s = geoeffnet(&path);
        std::fs::remove_file(&path).unwrap();
        s.execute("file.save", &json!({})).unwrap();
        assert_eq!(folien(&path), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn speichern_unter_neuem_namen_laesst_beide_fassungen_bestehen() {
        let dir = ordner("kopie");
        let (path, kopie) = (pfad(&dir, "a.deckcraft"), pfad(&dir, "b.deckcraft"));
        deck(1, &path);
        let mut s = geoeffnet(&path);
        von_aussen_ersetzen(&path, 5);
        s.execute("file.saveAs", &json!({"path": kopie})).unwrap();
        assert_eq!(folien(&path), 5);
        assert_eq!(folien(&kopie), 2);
        assert_eq!(s.doc().unwrap().path.as_deref(), Some(kopie.as_str()));
        assert!(!s.doc().unwrap().file_changed_elsewhere());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wer in „Speichern unter“ die Datei selbst auswählt, hat dem Ersetzen schon zugestimmt.
    #[test]
    fn speichern_unter_mit_derselben_datei_ersetzt_sie() {
        let dir = ordner("saveas");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let mut s = geoeffnet(&path);
        von_aussen_ersetzen(&path, 5);
        s.execute("file.saveAs", &json!({"path": path})).unwrap();
        assert_eq!(folien(&path), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nach einem Absturz ist der frühere Stand der Datei unbekannt: Das erste Speichern fragt
    /// nach, danach läuft alles normal.
    #[test]
    fn eine_wiederhergestellte_praesentation_fragt_beim_ersten_speichern() {
        let dir = ordner("absturz");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let s = geoeffnet(&path);
        let sicherung = dir.join("sicherung");
        crate::recovery::save(&s, &sicherung).unwrap();

        let mut neu = Session::new();
        crate::recovery::open(&mut neu, &sicherung).unwrap();
        assert_eq!(neu.doc().unwrap().path.as_deref(), Some(path.as_str()));
        let e = neu.execute("file.save", &json!({})).unwrap_err().to_string();
        assert!(changed_elsewhere(&e), "{e}");
        assert_eq!(folien(&path), 1, "die Datei bleibt, wie sie ist");

        neu.execute("file.save", &json!({"overwrite": true})).unwrap();
        assert_eq!(folien(&path), 2);
        neu.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        neu.execute("file.save", &json!({})).unwrap();
        assert_eq!(folien(&path), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Auch `file.adopt` (Öffnen im Hintergrund) merkt sich den Stand von vor dem Lesen.
    #[test]
    fn oeffnen_im_hintergrund_merkt_sich_den_stand_von_vor_dem_lesen() {
        let dir = ordner("adopt");
        let path = pfad(&dir, "a.deckcraft");
        deck(1, &path);
        let geladen = crate::cmd::file::read_presentation_file(&path).unwrap();
        // Die Datei ändert sich, nachdem sie gelesen wurde und bevor sie im Fenster ankommt.
        von_aussen_ersetzen(&path, 5);
        let mut s = Session::new();
        let token = s.stage_loaded(geladen);
        s.execute("file.adopt", &json!({"path": path, "token": token})).unwrap();
        assert!(s.doc().unwrap().file_changed_elsewhere());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
