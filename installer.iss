[Setup]
AppName=RustInk
AppVersion=1.0.0
DefaultDirName={autopf}\RustInk
DefaultGroupName=RustInk
OutputDir=Output
OutputBaseFilename=RustInk-Setup-v1
Compression=lzma
SolidCompression=yes
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

SetupIconFile=src\ui\icons\rustInk_logo.ico

UninstallDisplayIcon={app}\rustInk.exe

[Files]
Source: "dist\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "src\ui\icons\*"; DestDir: "{app}\icons"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\RustInk"; Filename: "{app}\rustInk.exe"
Name: "{autodesktop}\RustInk"; Filename: "{app}\rustInk.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create desktop Icon"; GroupDescription: "Quick choice:"