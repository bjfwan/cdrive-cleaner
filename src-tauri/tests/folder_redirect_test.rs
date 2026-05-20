use cdrive_cleaner_lib::folder_redirect;

#[tokio::test]
async fn get_known_folders_returns_seven_items() {
    let folders = folder_redirect::get_known_folders().await.unwrap();
    assert_eq!(folders.len(), 7);

    let ids: Vec<&str> = folders.iter().map(|f| f.id.as_str()).collect();
    assert!(ids.contains(&"downloads"));
    assert!(ids.contains(&"documents"));
    assert!(ids.contains(&"desktop"));
    assert!(ids.contains(&"pictures"));
    assert!(ids.contains(&"videos"));
    assert!(ids.contains(&"music"));
    assert!(ids.contains(&"temp"));
}

#[tokio::test]
async fn each_folder_has_non_empty_current_path() {
    let folders = folder_redirect::get_known_folders().await.unwrap();
    for folder in &folders {
        assert!(
            !folder.current_path.is_empty(),
            "folder {} has empty current_path",
            folder.id
        );
    }
}

#[tokio::test]
async fn known_folders_have_suggested_target_paths() {
    let folders = folder_redirect::get_known_folders().await.unwrap();
    for folder in &folders {
        assert!(
            !folder.suggested_target_path.is_empty(),
            "folder {} has empty suggested_target_path",
            folder.id
        );
    }
}
