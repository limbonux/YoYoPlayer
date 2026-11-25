@echo off
rem Runs cargo with the MSVC 14.44 toolset from Visual Studio 2026.
rem Without this, rustc picks Visual Studio 2015's link.exe, which fails with LNK1318
rem (this VS 2026 install has no vcvars64.bat to set the environment).
setlocal
set "VCT=C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Tools\MSVC\14.44.35207"
set "WK=C:\Program Files (x86)\Windows Kits\10"
set "WKV=10.0.22621.0"
set "LIB=%VCT%\lib\x64;%WK%\Lib\%WKV%\ucrt\x64;%WK%\Lib\%WKV%\um\x64"
set "INCLUDE=%VCT%\include;%WK%\Include\%WKV%\ucrt;%WK%\Include\%WKV%\um;%WK%\Include\%WKV%\shared"
set "PATH=%VCT%\bin\Hostx64\x64;%WK%\bin\%WKV%\x64;%USERPROFILE%\.cargo\bin;%PATH%"
set "VCINSTALLDIR=C:\Program Files\Microsoft Visual Studio\18\Professional\VC\"
set "VCToolsInstallDir=%VCT%\"
set "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=%VCT%\bin\Hostx64\x64\link.exe"
cargo %*
