import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';

import 'core/glasses_api.dart';
import 'core/session_controller.dart';
import 'src/rust/frb_generated.dart';
import 'ui/app.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dir = await getApplicationSupportDirectory();
  final audit = '${dir.path}${Platform.pathSeparator}ajustes.jsonl';
  final controller = SessionController(
    ({required bool simulated}) => RustGlassesApi.start(simulated: simulated, auditPath: audit),
  );
  final simulated = args.contains('--sim') || Platform.environment['XR_SIM'] == '1';
  await controller.start(simulated: simulated);
  runApp(XrLabApp(controller: controller));
}
