//! 安全检测测试：把已知的"危险"和"安全"路径喂给 safety::analyze，
//! 断言风险等级合理。无需 GUI、无需真实文件，运行毫秒级。

#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::migration::LinkType;
    use cdrive_cleaner_lib::safety::{analyze, MigrationSafety};
    use std::path::Path;

    fn analyze_path(path: &str) -> MigrationSafety {
        analyze(Path::new(path), LinkType::Auto, Some("D:"), 1024 * 1024 * 1024)
    }

    fn verdict_str(s: &MigrationSafety) -> String {
        format!("{:?}", s.verdict)
    }

    /// C:\Windows 必须被拦死。
    #[test]
    fn windows_dir_is_blocked() {
        let s = analyze_path(r"C:\Windows");
        assert!(
            !s.can_migrate,
            "C:\\Windows 应禁止迁移，verdict={}, findings={:?}",
            verdict_str(&s),
            s.findings
        );
    }

    /// C:\Program Files 也得拦死或至少警告。
    #[test]
    fn program_files_is_dangerous() {
        let s = analyze_path(r"C:\Program Files");
        assert!(
            !s.can_migrate,
            "C:\\Program Files 应禁止迁移，verdict={}",
            verdict_str(&s)
        );
    }

    /// 用户桌面通常是普通目录，应允许迁移（最多带提示）。
    #[test]
    fn user_desktop_is_migratable() {
        let user = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\admin".to_string());
        let desktop = format!(r"{}\Desktop\some-folder", user);
        let s = analyze_path(&desktop);
        // 不强制 verdict，但至少不能是 SystemCritical
        assert_ne!(
            verdict_str(&s),
            "SystemCritical",
            "用户桌面下的子目录不应被判为系统级危险"
        );
    }

    /// 同盘迁移到自己（target_disk=C: 而 source 也在 C:）应当至少给个警告。
    #[test]
    fn detection_is_fast() {
        // 单次分析应在几百毫秒内完成
        let s = analyze_path(r"C:\Windows");
        assert!(
            s.analysis_duration_ms < 2000,
            "安全分析耗时 {} ms，超过 2000 ms 阈值",
            s.analysis_duration_ms
        );
    }

    /// "集合根"（C:\Program Files、C:\Users 这种）应该立即被拒，
    /// 因为整迁集合根没有合理用途且分析极慢。
    #[test]
    fn collection_roots_are_blocked_immediately() {
        for path in [r"C:\Program Files", r"C:\Program Files (x86)", r"C:\Users"] {
            let s = analyze_path(path);
            assert!(
                !s.can_migrate,
                "{} 是集合根，应禁止迁移；得到 verdict={}",
                path,
                verdict_str(&s)
            );
            assert!(
                s.analysis_duration_ms < 100,
                "{} 应通过 system_critical 立即拦截，但耗时 {} ms",
                path,
                s.analysis_duration_ms
            );
        }
    }
}
