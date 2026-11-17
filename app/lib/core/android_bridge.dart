// Ponte com o código Kotlin do Android: rede do óculos e serviço em primeiro plano.
import 'dart:io';

import 'package:flutter/services.dart';

class AndroidBridge {
  static const _rede = EventChannel('xr_lab/rede');
  static const _servico = MethodChannel('xr_lab/servico');

  static bool get available => Platform.isAndroid;

  /// Handle da rede USB do óculos (`Network.getNetworkHandle()`), ou null quando não há.
  static Stream<int?> glassesNetwork() => _rede.receiveBroadcastStream().map((e) => e as int?);

  static Future<void> startService() => _servico.invokeMethod<bool>('iniciar');
  static Future<void> stopService() => _servico.invokeMethod<bool>('parar');
}
