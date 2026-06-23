/// Canal de compilación de Pronto.
///
/// - "prod": release pública (la que se publica en GitHub). Es el DEFECTO.
/// - "dev":  build LOCAL de pruebas. Se marca visualmente con un badge "DEV"
///           para no confundirlo con la versión de producción.
///
/// Se fija al compilar con `--dart-define=PRONTO_CHANNEL=dev`
/// (ver `tools/pronto-dev.ps1`). Sin el define, queda en "prod".
const String kChannel = String.fromEnvironment(
  'PRONTO_CHANNEL',
  defaultValue: 'prod',
);

/// Identificador corto del build de dev (p. ej. SHA de git u hora), opcional.
/// Lo inyecta `tools/pronto-dev.ps1` con `--dart-define=PRONTO_BUILD_ID=...`.
const String kBuildId = String.fromEnvironment('PRONTO_BUILD_ID');

/// ¿Es un build de desarrollo (local), no de producción?
bool get kIsDev => kChannel == 'dev';
