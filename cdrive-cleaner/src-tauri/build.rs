fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = tauri_build::WindowsResource::new();
        res.set_manifest_file("cdrive-cleaner.exe.manifest");
        res.compile().unwrap();
    }
    
    tauri_build::build()
}
