@echo off
rem Runs dc2tk from the repository folder: build\release\dc2tk.exe, built first when missing.
if not exist "%~dp0build\release\dc2tk.exe" (
    cargo build --release --manifest-path "%~dp0Cargo.toml" || exit /b 1
)
"%~dp0build\release\dc2tk.exe" %*
