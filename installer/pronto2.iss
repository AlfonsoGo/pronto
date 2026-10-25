; Instalador de Pronto 2 (Rust + Tauri 2) para Windows, sin admin.
; Compilar con installer\build_installer.ps1 (reune el stage y llama a ISCC).
;
; ACTUALIZACION EN SITIO DESDE PRONTO 1
; Pronto 1 (Flutter) se actualiza descargando el primer .exe de la ultima release,
; matando `pronto` y ejecutando este instalador con
;   /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL
; para despues reabrir su propio pronto.exe. Por eso este instalador usa el MISMO
; AppId que el de Pronto 1: Inno detecta la instalacion previa, reutiliza SU carpeta
; (UsePreviousAppDir) y deja aqui el pronto.exe de Pronto 2, que es el que el ayudante
; reabre. NO cambiar AppId, AppName, ni el nombre del exe (pronto.exe).
;
; Variante de prueba: /DTestSandbox=1 /DTestDir=<carpeta> => otro AppId, otro nombre y
; otro directorio por defecto, para probar sin tocar la instalacion real.

#define AppName "Pronto"
; build_installer.ps1 inyecta /DAppVersion=x.y.z leido de src-tauri\tauri.conf.json.
#ifndef AppVersion
  #define AppVersion "2.3.0"
#endif
#define AppPublisher "Pronto"
#define AppExe "pronto.exe"
; Carpeta con SOLO lo que se instala (pronto.exe + DLL necesarias).
#ifndef StageDir
  #define StageDir "stage"
#endif
#ifndef OutDir
  #define OutDir "dist"
#endif

#ifdef TestSandbox
  #define MyAppId "{{B7E5B2A0-9C4D-4E1A-9F3B-7A2D6E8C1F4F}"
  #define MyAppName "ProntoTest"
  #define MyOutName "ProntoTest-Setup-" + AppVersion
  #ifndef TestDir
    #error TestSandbox necesita /DTestDir=<carpeta de pruebas>
  #endif
  #define MyDefaultDir TestDir + "\default-dir"
#else
  ; MISMO AppId y nombre que Pronto 1 (ver pronto.iss de Pronto 1). Ojo: alli es
  ; "{{...F40}}", con DOS llaves de cierre, y el registro guarda "{...F40}}_is1". Hay que
  ; copiarlo tal cual; con una sola llave Inno lo toma por otra app (fallo de la 2.0.0).
  #define MyAppId "{{B7E5B2A0-9C4D-4E1A-9F3B-7A2D6E8C1F40}}"
  ; AppId que uso por error la 2.0.0: su entrada de desinstalacion se borra al instalar.
  #define StaleAppId "{B7E5B2A0-9C4D-4E1A-9F3B-7A2D6E8C1F40}"
  #define MyAppName AppName
  #define MyOutName "Pronto-Setup-" + AppVersion
  #define MyDefaultDir "{autopf}\Pronto"
#endif

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#AppVersion}
AppVerName={#MyAppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={#MyDefaultDir}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#OutDir}
OutputBaseFilename={#MyOutName}
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#MyAppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#MyAppName}
VersionInfoDescription=Instalador de {#MyAppName}

; --- Actualizacion en sitio ---
; Reutiliza la carpeta de la instalacion previa (la de Pronto 1 si viene de ahi).
UsePreviousAppDir=yes
DisableDirPage=auto
; Pronto 2 no crea ningun mutex. Tampoco dejamos que Inno cierre/reinicie apps por su
; cuenta (Pronto se minimiza a la bandeja y el ayudante de Pronto 1 ya lo cierra y lo
; reabre): [Code] comprueba si pronto.exe esta en uso y pide cerrarlo, como en Pronto 1.
CloseApplications=no
RestartApplications=no

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
es.AppRunning=Pronto está abierto y hay que cerrarlo para instalar.%n%nCiérralo del todo: haz clic derecho en el icono de Pronto en la bandeja del sistema (abajo a la derecha, junto al reloj) y pulsa "Salir". Cuando lo hayas cerrado, pulsa Aceptar para continuar.
en.AppRunning=Pronto is running and must be closed to install.%n%nQuit it completely: right-click the Pronto icon in the system tray (near the clock) and choose "Quit". Once it is closed, click OK to continue.
es.AppRunningAbort=Pronto sigue abierto, así que no se ha instalado nada. Ciérralo desde el icono de la bandeja y vuelve a ejecutar el instalador.
en.AppRunningAbort=Pronto is still running, so nothing was installed. Quit it from the tray icon and run the installer again.
es.NoWebView2=Pronto necesita Microsoft Edge WebView2 Runtime y no parece estar instalado en este equipo (Windows 11 lo trae de serie; Windows 10 casi siempre).%n%n¿Abrir ahora la página de descarga y salir del instalador?%n%nSí = abrir la descarga y salir (instala WebView2 y vuelve a ejecutar este instalador).%nNo = continuar la instalación de todos modos.
en.NoWebView2=Pronto needs the Microsoft Edge WebView2 Runtime and it does not seem to be installed on this computer (Windows 11 ships with it; Windows 10 almost always has it).%n%nOpen the download page now and exit Setup?%n%nYes = open the download and exit (install WebView2, then run this installer again).%nNo = continue installing anyway.
es.UninstallAppRunning=Pronto está abierto. Ciérralo del todo desde el icono de la bandeja (clic derecho -> "Salir") y vuelve a intentarlo.
en.UninstallAppRunning=Pronto is running. Quit it from the tray icon (right-click -> "Quit") and try again.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

; Limpieza de restos de Pronto 1 (Flutter). Solo si se detecta una instalacion de
; Pronto 1 en {app} (flutter_windows.dll), para no borrar una carpeta "data" ajena si
; alguien instala en un directorio compartido. NO se toca models\ (puede contener el
; modelo de voz que Pronto 2 reutiliza). Pronto 2 es un unico pronto.exe autocontenido
; (sherpa-onnx, SQLite y el cargador de WebView2 enlazados estaticamente), asi que
; ninguna de estas DLL le hace falta.
[InstallDelete]
Type: filesandordirs; Name: "{app}\data"; Check: IsPronto1Install
Type: files; Name: "{app}\flutter_windows.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\*_plugin.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\native_assets.json"; Check: IsPronto1Install
Type: files; Name: "{app}\ggml*.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\whisper.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\vcomp140.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\onnxruntime.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\sherpa-onnx-c-api.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\sqlite3.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\msvcp140.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\vcruntime140.dll"; Check: IsPronto1Install
Type: files; Name: "{app}\vcruntime140_1.dll"; Check: IsPronto1Install

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Desinstalar {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WebView2Guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  WebView2Url = 'https://developer.microsoft.com/microsoft-edge/webview2/';

var
  Pronto1Found: Boolean;

function CreateFileW(lpFileName: String; dwDesiredAccess, dwShareMode, lpSecurityAttributes,
  dwCreationDisposition, dwFlagsAndAttributes, hTemplateFile: Cardinal): Integer;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(hObject: Integer): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

{ True si el fichero esta abierto por otro proceso (un .exe en ejecucion no se puede
  abrir para escritura). No modifica nada. Si no existe, no esta en uso. }
function IsFileInUse(const Path: String): Boolean;
var
  H: Integer;
begin
  Result := False;
  if not FileExists(Path) then Exit;
  { GENERIC_WRITE, sin compartir, OPEN_EXISTING }
  H := CreateFileW(Path, $40000000, 0, 0, 3, $80, 0);
  if H = -1 then
    Result := True
  else
    CloseHandle(H);
end;

function WebView2KeyHasVersion(const Root: Integer; const SubKey: String): Boolean;
var
  V: String;
begin
  Result := False;
  case Root of
    0: Result := RegQueryStringValue(HKLM32, SubKey, 'pv', V);
    1: Result := RegQueryStringValue(HKLM64, SubKey, 'pv', V);
    2: Result := RegQueryStringValue(HKCU, SubKey, 'pv', V);
  end;
  if Result then
    Result := (V <> '') and (V <> '0.0.0.0');
end;

{ Instalacion de WebView2 Runtime: por equipo (HKLM, vista de 32 bits) o por usuario
  (HKCU). Windows 11 lo trae preinstalado. }
function IsWebView2Installed: Boolean;
var
  Sub: String;
begin
  Sub := 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WebView2Guid;
  Result := WebView2KeyHasVersion(0, Sub) or WebView2KeyHasVersion(1, Sub)
    or WebView2KeyHasVersion(2, 'Software\Microsoft\EdgeUpdate\Clients\' + WebView2Guid);
end;

function InitializeSetup(): Boolean;
var
  ErrCode: Integer;
begin
  Result := True;
  if IsWebView2Installed then Exit;
  Log('WebView2 Runtime no encontrado en el registro.');
  if WizardSilent then
  begin
    { Actualizacion automatica desde Pronto 1: sin WebView2 la app nueva no arrancaria
      y se perderia la que funciona. Abortamos antes de tocar nada; Pronto 1 sigue. }
    Result := False;
    Exit;
  end;
  if MsgBox(CustomMessage('NoWebView2'), mbConfirmation, MB_YESNO) = IDYES then
  begin
    ShellExec('open', WebView2Url, '', '', SW_SHOWNORMAL, ewNoWait, ErrCode);
    Result := False;
  end;
end;

function IsPronto1Install: Boolean;
begin
  Result := Pronto1Found;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Exe: String;
  Tries: Integer;
begin
  Result := '';
  Exe := ExpandConstant('{app}\{#AppExe}');
  Tries := 0;
  while IsFileInUse(Exe) do
  begin
    if WizardSilent then
    begin
      { El ayudante de Pronto 1 ya lo ha cerrado; damos hasta ~20 s a que Windows
        libere el fichero. }
      Inc(Tries);
      if Tries > 40 then
      begin
        Result := CustomMessage('AppRunningAbort');
        Exit;
      end;
      Sleep(500);
    end
    else if MsgBox(CustomMessage('AppRunning'), mbError, MB_OKCANCEL) = IDCANCEL then
    begin
      Result := CustomMessage('AppRunningAbort');
      Exit;
    end;
  end;
  { Se evalua aqui, ANTES de que [InstallDelete] borre flutter_windows.dll. }
  Pronto1Found := FileExists(ExpandConstant('{app}\flutter_windows.dll'));
  if Pronto1Found then Log('Detectada instalacion de Pronto 1 (Flutter): se limpian sus restos.');
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  if IsFileInUse(ExpandConstant('{app}\{#AppExe}')) then
  begin
    MsgBox(CustomMessage('UninstallAppRunning'), mbError, MB_OK);
    Result := False;
  end;
end;

#ifdef StaleAppId
{ La 2.0.0 se instalo con otro AppId: Windows mostraria dos "Pronto". Se quita esa entrada
  y su desinstalador (si no es el nuestro). }
procedure CurStepChanged(CurStep: TSetupStep);
var
  Key, Unins: String;
begin
  if CurStep <> ssPostInstall then Exit;
  Key := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#StaleAppId}_is1';
  if not RegKeyExists(HKCU, Key) then Exit;
  if RegQueryStringValue(HKCU, Key, 'UninstallString', Unins) then
  begin
    Unins := RemoveQuotes(Unins);
    if CompareText(Unins, ExpandConstant('{uninstallexe}')) <> 0 then
    begin
      DeleteFile(Unins);
      DeleteFile(ChangeFileExt(Unins, '.dat'));
    end;
  end;
  if RegDeleteKeyIncludingSubkeys(HKCU, Key) then
    Log('Entrada de desinstalacion de la 2.0.0 eliminada: ' + Key);
end;
#endif
