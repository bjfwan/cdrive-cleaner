@echo off
setlocal
set "PATH=D:\DevTools\Rust\cargo\bin;%PATH%"
set "RUSTUP_HOME=D:\DevTools\Rust\rustup"
set "CARGO_HOME=D:\DevTools\Rust\cargo"
npm run tauri dev
endlocal
