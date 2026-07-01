#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <windows.h>

#include <cstdio>
#include <cwchar>

#include "flutter_window.h"
#include "utils.h"

// Registra en %TEMP%\pronto_crash.log cualquier crash nativo no controlado.
// Lo importante: EXCEPTION_ILLEGAL_INSTRUCTION (0xC000001D) significa que una
// DLL usa instrucciones de CPU que el equipo no soporta (p. ej. ggml compilado
// con AVX-512 corriendo en una CPU sin AVX-512). Convierte un "se cierra sin
// decir nada" en un diagnostico claro que el usuario nos puede mandar.
static LONG WINAPI ProntoCrashHandler(EXCEPTION_POINTERS *info) {
  wchar_t path[MAX_PATH] = {0};
  ::GetTempPathW(MAX_PATH, path);
  ::wcsncat_s(path, MAX_PATH, L"pronto_crash.log", _TRUNCATE);
  HANDLE h = ::CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ, nullptr,
                           OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (h != INVALID_HANDLE_VALUE) {
    char buf[160];
    int len = ::sprintf_s(buf, "Pronto crash: code=0x%08lX addr=0x%p\r\n",
                          info->ExceptionRecord->ExceptionCode,
                          info->ExceptionRecord->ExceptionAddress);
    if (len > 0) {
      DWORD written = 0;
      ::SetFilePointer(h, 0, nullptr, FILE_END);
      ::WriteFile(h, buf, static_cast<DWORD>(len), &written, nullptr);
    }
    ::CloseHandle(h);
  }
  return EXCEPTION_EXECUTE_HANDLER;
}

int APIENTRY wWinMain(_In_ HINSTANCE instance, _In_opt_ HINSTANCE prev,
                      _In_ wchar_t *command_line, _In_ int show_command) {
  // Endurecimiento contra DLL planting (secuestro del orden de búsqueda de
  // DLLs): restringe la búsqueda a los directorios seguros por defecto
  // (System32 + la carpeta del .exe) y saca el directorio de trabajo actual de
  // la ruta de búsqueda. Así una DLL maliciosa colocada en la carpeta desde la
  // que se lanza Pronto (Descargas, un USB…) NO se carga en lugar de la buena.
  ::SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
  ::SetDllDirectory(L"");

  // Lo primero: capturar crashes nativos a un log (ver ProntoCrashHandler).
  ::SetUnhandledExceptionFilter(ProntoCrashHandler);

  // Attach to console when present (e.g., 'flutter run') or create a
  // new console when running with a debugger.
  if (!::AttachConsole(ATTACH_PARENT_PROCESS) && ::IsDebuggerPresent()) {
    CreateAndAttachConsole();
  }

  // Mutex con nombre, con DOBLE función:
  //  1) El instalador (Inno Setup, AppMutex) detecta una instancia en ejecución
  //     antes de actualizar, evitando que pronto.exe quede bloqueado.
  //  2) INSTANCIA ÚNICA: si Pronto ya está abierto (p. ej. minimizado en la
  //     bandeja como píldora), NO abrimos un segundo proceso. En su lugar
  //     traemos al frente la ventana existente y salimos. Antes no se comprobaba
  //     y se podían tener "dos Pronto" a la vez (dos iconos, panel fantasma…).
  ::CreateMutexW(nullptr, FALSE, L"ProntoAppMutex");
  if (::GetLastError() == ERROR_ALREADY_EXISTS) {
    HWND existing = ::FindWindowW(nullptr, L"Pronto");
    if (existing != nullptr) {
      if (::IsIconic(existing)) {
        ::ShowWindow(existing, SW_RESTORE);
      }
      ::SetForegroundWindow(existing);
    }
    return EXIT_SUCCESS;
  }

  // Initialize COM, so that it is available for use in the library and/or
  // plugins.
  ::CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

  flutter::DartProject project(L"data");

  std::vector<std::string> command_line_arguments =
      GetCommandLineArguments();

  project.set_dart_entrypoint_arguments(std::move(command_line_arguments));

  FlutterWindow window(project);
  Win32Window::Point origin(10, 10);
  Win32Window::Size size(1280, 720);
  if (!window.Create(L"Pronto", origin, size)) {
    return EXIT_FAILURE;
  }
  window.SetQuitOnClose(true);

  ::MSG msg;
  while (::GetMessage(&msg, nullptr, 0, 0)) {
    ::TranslateMessage(&msg);
    ::DispatchMessage(&msg);
  }

  ::CoUninitialize();
  return EXIT_SUCCESS;
}
