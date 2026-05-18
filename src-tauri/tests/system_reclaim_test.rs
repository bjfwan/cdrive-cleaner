use cdrive_cleaner_lib::system_reclaim;

#[tokio::test]
async fn get_reclaim_opportunities_returns_non_empty() {
    let ops = system_reclaim::get_reclaim_opportunities().await.unwrap();
    assert!(!ops.is_empty());
    assert!(ops.len() >= 5);
    for op in &ops {
        assert!(!op.id.is_empty());
        assert!(!op.label.is_empty());
    }
}

#[tokio::test]
async fn hiberfil_size_detection() {
    let ops = system_reclaim::get_reclaim_opportunities().await.unwrap();
    let hib = ops.iter().find(|o| o.id == "hibernation").unwrap();
    let hiberfil = std::path::Path::new(r"C:\hiberfil.sys");
    if hiberfil.exists() {
        assert!(hib.current_size > 0);
    } else {
        assert_eq!(hib.reclaimable_size, 0);
    }
}
