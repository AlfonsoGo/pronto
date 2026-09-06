import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Lo poco que hay que recordar del asistente de instalación entre arranques.
///
/// Deliberadamente pequeño. La mayoría de los bloques del asistente NO guardan
/// una marca propia: se preguntan a la realidad cada vez que se dibuja la
/// pantalla —¿está el modelo en disco?, ¿hay permiso de micrófono?— porque una
/// marca puede quedarse diciendo «hecho» de un modelo que el usuario borró
/// después. Aquí solo vive lo que no se puede deducir mirando el equipo: lo que
/// el usuario ELIGIÓ, y si ya terminó.
@immutable
class OnboardingProgress {
  /// El usuario tocó el idioma. Sin esto no se distingue «eligió español» de
  /// «español estaba puesto de fábrica y nadie miró».
  final bool idiomaElegido;

  /// Ya hizo el dictado de prueba al menos una vez.
  final bool pruebaHecha;

  /// Eligió a conciencia un Camino de formateo (aunque eligiera «sin formateo»).
  final bool formateoElegido;

  /// El asistente está cerrado: no se vuelve a abrir solo.
  final bool terminado;

  /// Dónde dejó el mando rapidez ↔ precisión (0 = rapidez, 1 = precisión).
  final double preferenciaMotor;

  const OnboardingProgress({
    this.idiomaElegido = false,
    this.pruebaHecha = false,
    this.formateoElegido = false,
    this.terminado = false,
    this.preferenciaMotor = 0.5,
  });

  /// El que se da por terminado sin haber pasado por el asistente. Es el estado
  /// de quien ya tenía Pronto instalado y funcionando antes de que existiera
  /// esta pantalla (ver [OnboardingController._migrarInstalacionExistente]).
  static const yaInstalado = OnboardingProgress(terminado: true);

  OnboardingProgress copyWith({
    bool? idiomaElegido,
    bool? pruebaHecha,
    bool? formateoElegido,
    bool? terminado,
    double? preferenciaMotor,
  }) {
    return OnboardingProgress(
      idiomaElegido: idiomaElegido ?? this.idiomaElegido,
      pruebaHecha: pruebaHecha ?? this.pruebaHecha,
      formateoElegido: formateoElegido ?? this.formateoElegido,
      terminado: terminado ?? this.terminado,
      preferenciaMotor: preferenciaMotor ?? this.preferenciaMotor,
    );
  }

  Map<String, dynamic> toJson() => {
        'idiomaElegido': idiomaElegido,
        'pruebaHecha': pruebaHecha,
        'formateoElegido': formateoElegido,
        'terminado': terminado,
        'preferenciaMotor': preferenciaMotor,
      };

  /// Reconstruye desde disco. Tolerante a propósito: un JSON de una versión
  /// anterior (o a medio escribir) deja el asistente en su estado inicial, que
  /// como mucho hace repetir un paso — nunca revienta el arranque.
  factory OnboardingProgress.fromJson(Map<String, dynamic> json) {
    bool leerBool(String clave) => json[clave] == true;
    final pref = json['preferenciaMotor'];
    return OnboardingProgress(
      idiomaElegido: leerBool('idiomaElegido'),
      pruebaHecha: leerBool('pruebaHecha'),
      formateoElegido: leerBool('formateoElegido'),
      terminado: leerBool('terminado'),
      preferenciaMotor:
          pref is num ? pref.toDouble().clamp(0.0, 1.0) : 0.5,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is OnboardingProgress &&
      other.idiomaElegido == idiomaElegido &&
      other.pruebaHecha == pruebaHecha &&
      other.formateoElegido == formateoElegido &&
      other.terminado == terminado &&
      other.preferenciaMotor == preferenciaMotor;

  @override
  int get hashCode => Object.hash(
        idiomaElegido,
        pruebaHecha,
        formateoElegido,
        terminado,
        preferenciaMotor,
      );
}

final onboardingProvider =
    NotifierProvider<OnboardingController, OnboardingProgress>(
  OnboardingController.new,
);

/// Se completa cuando el avance ya se ha leído de disco.
final avanceLeidoProvider = FutureProvider<void>((ref) async {
  await ref.read(onboardingProvider.notifier).loaded;
});

/// ¿Hay que abrir el asistente de instalación?
///
/// `null` mientras el avance se está leyendo. Ese tercer valor es el que evita
/// la avería fea: sin él, durante los milisegundos que tarda [SharedPreferences]
/// en responder, el estado inicial dice «sin terminar» y a quien lleva meses
/// usando Pronto le parpadearía un asistente de instalación en la cara —
/// redimensionando la ventana de paso.
final debeAbrirAsistenteProvider = Provider<bool?>((ref) {
  if (!ref.watch(avanceLeidoProvider).hasValue) return null;
  return !ref.watch(onboardingProvider).terminado;
});

/// Guarda y recupera el avance del asistente de instalación.
///
/// Cada cambio se persiste en el acto: el asistente es **reanudable**, y para
/// eso «guardar al salir» no vale — de un cierre a lo bruto no hay salida.
class OnboardingController extends Notifier<OnboardingProgress> {
  /// Clave del JSON del avance en [SharedPreferences].
  static const String prefsKey = 'pronto_onboarding_v1';

  /// Marca vieja de «el motor se preparó bien al menos una vez», que escribía
  /// el splash desde la 0.7. Es la huella de una instalación que YA funcionaba.
  static const String legacyReadyOnceKey = 'pronto_engine_ready_once';

  late final Future<void> loaded;

  @override
  OnboardingProgress build() {
    loaded = _cargar();
    return const OnboardingProgress();
  }

  Future<void> _cargar() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final crudo = prefs.getString(prefsKey);
      if (crudo != null) {
        final json = jsonDecode(crudo);
        if (json is Map<String, dynamic>) {
          state = OnboardingProgress.fromJson(json);
          return;
        }
      }
      state = await _migrarInstalacionExistente(prefs);
    } catch (e) {
      // Sin avance legible se arranca en el estado inicial. Peor sería no
      // arrancar: el asistente es una pantalla, no el producto.
      debugPrint('[TF] no se pudo leer el avance del asistente: $e');
    }
  }

  /// Decide qué hacer con quien ya tenía Pronto antes de que existiera esta
  /// pantalla.
  ///
  /// 🔴 Pronto **se actualiza solo** (PRNT-20): esta versión aterriza en
  /// equipos ajenos donde el dictado ya funcionaba. Abrirles un asistente de
  /// instalación al reiniciar sería secuestrarles la app para configurar lo que
  /// ya estaba configurado. Si hay huella de una instalación que funcionó, el
  /// asistente nace **terminado** y se guarda así, y quien lo quiera lo lanza a
  /// mano desde Ajustes.
  Future<OnboardingProgress> _migrarInstalacionExistente(
    SharedPreferences prefs,
  ) async {
    final yaFuncionaba = prefs.getBool(legacyReadyOnceKey) ?? false;
    if (!yaFuncionaba) return const OnboardingProgress();
    const progreso = OnboardingProgress.yaInstalado;
    await _guardar(prefs, progreso);
    return progreso;
  }

  Future<void> _guardar(
    SharedPreferences prefs,
    OnboardingProgress progreso,
  ) async {
    await prefs.setString(prefsKey, jsonEncode(progreso.toJson()));
  }

  Future<void> _actualizar(OnboardingProgress progreso) async {
    state = progreso;
    try {
      final prefs = await SharedPreferences.getInstance();
      await _guardar(prefs, progreso);
    } catch (e) {
      // El avance no se pudo apuntar: el asistente sigue usable en esta
      // sesión y como mucho se repite un paso al volver a abrir.
      debugPrint('[TF] no se pudo guardar el avance del asistente: $e');
    }
  }

  Future<void> marcarIdiomaElegido() =>
      _actualizar(state.copyWith(idiomaElegido: true));

  Future<void> marcarPruebaHecha() =>
      _actualizar(state.copyWith(pruebaHecha: true));

  Future<void> marcarFormateoElegido() =>
      _actualizar(state.copyWith(formateoElegido: true));

  Future<void> setPreferenciaMotor(double valor) =>
      _actualizar(state.copyWith(preferenciaMotor: valor.clamp(0.0, 1.0)));

  /// Cierra el asistente. A partir de aquí Pronto arranca normal.
  ///
  /// Deja también la marca vieja del splash: quien termina el asistente ya ha
  /// pasado por la descarga y por la carga del motor, y sin esto la pantalla de
  /// carga saldría justo después, a preparar lo que ya está preparado.
  Future<void> terminar() async {
    await _actualizar(state.copyWith(terminado: true));
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setBool(legacyReadyOnceKey, true);
    } catch (e) {
      debugPrint('[TF] no se pudo marcar el motor como preparado: $e');
    }
  }

  /// «Repetir la instalación» desde Ajustes: se vuelve al principio del todo,
  /// no a un asistente medio marcado de la vez anterior.
  Future<void> repetir() => _actualizar(
        OnboardingProgress(preferenciaMotor: state.preferenciaMotor),
      );
}
