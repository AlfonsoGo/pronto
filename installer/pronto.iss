; Instalador de Pronto — generado para distribución en Windows (sin admin).
; Compilar con: ISCC.exe installer\pronto.iss

#define AppName "Pronto"
; La versión la inyecta build_installer.ps1 con /DAppVersion=x.y.z (desde el
; pubspec). El valor de abajo es solo el respaldo si se compila a mano.
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppPublisher "Pronto"
#define AppExe "pronto.exe"
#define RelDir "..\build\windows\x64\runner\Release"

[Setup]
AppId={{B7E5B2A0-9C4D-4E1A-9F3B-7A2D6E8C1F40}}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=Pronto-Setup-{#AppVersion}
SetupIconFile=..\windows\runner\resources\app_icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

; --- Actualización en sitio: las versiones futuras reemplazan la instalada ---
; Mismo AppId => Inno detecta la versión previa, reutiliza su MISMA carpeta y la
; actualiza (no crea una instalación duplicada). AppMutex permite cerrar Pronto
; si está abierto para no chocar con ficheros bloqueados.
; NO intentamos cerrar Pronto automaticamente (a veces no se puede: la app se
; minimiza a la bandeja en vez de salir). En su lugar, AppMutex hace que Inno
; DETECTE la instancia abierta y pida al usuario que la cierre (mensaje claro
; mas abajo en [Messages]).
AppMutex=ProntoAppMutex
UsePreviousAppDir=yes
DisableDirPage=auto
VersionInfoVersion={#AppVersion}

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Messages]
; Mensaje claro cuando Pronto esta abierto al instalar/actualizar (AppMutex).
es.SetupAppRunningError=Pronto esta abierto y hay que cerrarlo para actualizar.%n%nCierralo del todo: haz clic derecho en el icono de Pronto en la bandeja del sistema (abajo a la derecha, junto al reloj) y pulsa "Salir". Cuando lo hayas cerrado, pulsa Aceptar para continuar.
en.SetupAppRunningError=Pronto is running and must be closed to update.%n%nQuit it completely: right-click the Pronto icon in the system tray (near the clock) and choose "Quit". Once it is closed, click OK to continue.
es.UninstallAppRunningError=Pronto esta abierto. Cierralo del todo desde el icono de la bandeja (clic derecho -> "Salir") y vuelve a intentarlo.
en.UninstallAppRunningError=Pronto is running. Quit it from the tray icon (right-click -> "Quit") and try again.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#RelDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Desinstalar {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent