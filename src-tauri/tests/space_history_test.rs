use cdrive_cleaner_lib::database::SpaceHistoryDb;

fn temp_db_path(prefix: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "cdrive-cleaner-space-{}-{}-{}.db",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path.to_string_lossy().to_string()
}

#[test]
fn record_and_read_snapshots() {
    let path = temp_db_path("record");
    let db = SpaceHistoryDb::new(&path).unwrap();
    db.record_snapshot("C:\\", 500, 200, 180).unwrap();
    db.record_snapshot("C:\\", 500, 220, 200).unwrap();
    let history = db.get_history("C:\\", 30).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].used_size, 200);
    assert_eq!(history[1].used_size, 220);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn purge_old_keeps_recent() {
    let path = temp_db_path("purge");
    let db = SpaceHistoryDb::new(&path).unwrap();
    db.record_snapshot("D:\\", 1000, 500, 400).unwrap();
    let removed = db.purge_old(90).unwrap();
    assert_eq!(removed, 0);
    let history = db.get_history("D:\\", 90).unwrap();
    assert_eq!(history.len(), 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn isolate_history_by_drive() {
    let path = temp_db_path("isolate");
    let db = SpaceHistoryDb::new(&path).unwrap();
    db.record_snapshot("C:\\", 100, 50, 40).unwrap();
    db.record_snapshot("D:\\", 200, 80, 70).unwrap();
    let c_history = db.get_history("C:\\", 30).unwrap();
    let d_history = db.get_history("D:\\", 30).unwrap();
    assert_eq!(c_history.len(), 1);
    assert_eq!(d_history.len(), 1);
    assert_eq!(c_history[0].used_size, 50);
    assert_eq!(d_history[0].used_size, 80);
    let _ = std::fs::remove_file(&path);
}
