[Setup]
AppName=RustInk
AppVersion=0.1.0
DefaultDirName={autopf}\RustInk
DefaultGroupName=RustInk
OutputDir=Output
OutputBaseFilename=RustInk-Setup
Compression=lzma
SolidCompression=yes
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

[Files]
Source: "dist\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "src\ui\icons\*"; DestDir: "{app}\icons"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\RustInk"; Filename: "{app}\rustInk.exe"
Name: "{autodesktop}\RustInk"; Filename: "{app}\rustInk.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create desktop Icon"; GroupDescription: "Quick choice:"