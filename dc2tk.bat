@echo off
rem Runs dc2tk from the repository folder: build\release\dc2tk.exe, built first when missing.
if not exist "%~dp0build\release\dc2tk.exe" (
    pushd "%~dp0src"
    cargo build --release
    popd
    if not exist "%~dp0build\release\dc2tk.exe" exit /b 1
)
"%~dp0build\release\dc2tk.exe" %*
