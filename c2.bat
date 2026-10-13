@echo off
rem Runs c2 from the repository folder: build\release\c2.exe, built first when missing.
if not exist "%~dp0build\release\c2.exe" (
    cargo build --release --manifest-path "%~dp0Cargo.toml" || exit /b 1
)
"%~dp0build\release\c2.exe" %*
