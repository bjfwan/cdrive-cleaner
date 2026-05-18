use cdrive_cleaner_lib::scanner::space_breakdown::explain_path;

#[test]
fn test_explain_chrome_cache() {
    let result = explain_path(
        r"C:\Users\test\AppData\Local\Google\Chrome\User Data\Default\Cache\data_1",
    );
    assert!(result.explanation.contains("Chrome"));
    assert!(result.safe_to_delete);
    assert!(result.will_regenerate);
    assert_eq!(result.app_name, Some("Google Chrome".to_string()));
}

#[test]
fn test_explain_node_modules() {
    let result = explain_path(r"D:\projects\my-app\node_modules");
    assert!(result.explanation.contains("Node"));
    assert!(result.safe_to_delete);
    assert!(result.will_regenerate);
    assert_eq!(result.app_name, Some("Node.js".to_string()));
}

#[test]
fn test_explain_pagefile() {
    let result = explain_path(r"C:\pagefile.sys");
    assert!(result.explanation.contains("虚拟内存") || result.explanation.contains("页面文件"));
    assert!(!result.safe_to_delete);
    assert!(!result.will_regenerate);
    assert_eq!(result.app_name, Some("Windows".to_string()));
}

#[test]
fn test_explain_unknown_path() {
    let result = explain_path(r"C:\SomeRandomFolder\unknown.dat");
    assert!(result.explanation.contains("未识别"));
    assert!(!result.safe_to_delete);
}

#[test]
fn test_explain_vscode_logs() {
    let result = explain_path(r"C:\Users\test\AppData\Roaming\Code\logs\main.log");
    assert!(result.explanation.contains("VS Code"));
    assert!(result.safe_to_delete);
    assert!(result.will_regenerate);
}

#[test]
fn test_explain_cargo_registry() {
    let result = explain_path(r"C:\Users\test\.cargo\registry\cache");
    assert!(result.explanation.contains("Rust") || result.explanation.contains("Cargo") || result.explanation.contains("crate"));
    assert!(result.safe_to_delete);
    assert!(result.will_regenerate);
}

#[test]
fn test_explain_hiberfil() {
    let result = explain_path(r"C:\hiberfil.sys");
    assert!(result.explanation.contains("休眠"));
    assert!(!result.safe_to_delete);
}

#[test]
fn test_explain_temp_file() {
    let result = explain_path(r"C:\Users\test\AppData\Local\Temp\setup.tmp");
    assert!(result.safe_to_delete);
}
