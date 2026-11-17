// Ponta a ponta: interface + ponte + núcleo Rust de verdade, contra o óculos SIMULADO.
// Rodar com: flutter test integration_test/app_test.dart -d linux
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:xr_lab/core/glasses_api.dart';
import 'package:xr_lab/core/session_controller.dart';
import 'package:xr_lab/src/rust/frb_generated.dart';
import 'package:xr_lab/ui/app.dart';
import 'package:xr_lab/ui/widgets/confirm_dialog.dart';

Future<void> waitFor(WidgetTester tester, Finder f, {Duration timeout = const Duration(seconds: 10)}) async {
  final end = DateTime.now().add(timeout);
  while (f.evaluate().isEmpty) {
    if (DateTime.now().isAfter(end)) fail('não apareceu: $f');
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pump();
  }
}

Future<void> wait(WidgetTester tester, Duration d) async {
  await tester.runAsync(() => Future<void>.delayed(d));
  await tester.pump();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => RustLib.init());

  testWidgets('núcleo real com o óculos simulado', (tester) async {
    final dir = Directory.systemTemp.createTempSync('xr-lab-teste');
    final audit = '${dir.path}/ajustes.jsonl';
    final c = SessionController(
      ({required bool simulated}) => RustGlassesApi.start(simulated: simulated, auditPath: audit),
    );
    await tester.runAsync(() => c.start(simulated: true));
    await tester.pumpWidget(XrLabApp(controller: c));

    await waitFor(tester, find.text('15.SIMULADO'));
    expect(find.text('1.9.1.20260914175625'), findsOneWidget);
    expect(find.text('Conexão (simulado)'), findsOneWidget);

    // orientação chega e fica nivelada (o simulado está parado e nivelado)
    await tester.tap(find.text('Orientação'));
    await waitFor(tester, find.byKey(const Key('recentrar')));
    final o = c.orientation.value!;
    expect(o.pitch.abs(), lessThan(1.5));
    expect(o.roll.abs(), lessThan(1.5));

    // ajuste pela trava de verdade: preparar → diálogo → confirmar
    await tester.tap(find.text('Ajustes'));
    await wait(tester, const Duration(milliseconds: 200));
    await tester.tap(find.byKey(const Key('modo-ajustes')));
    await waitFor(tester, find.textContaining('Começa desligado')); // continua visível com o modo ligado
    await wait(tester, const Duration(milliseconds: 500));
    expect(c.state.value!.adjustmentsEnabled, isTrue);
    await tester.ensureVisible(find.byKey(const Key('aplicar-brilho')));
    await tester.tap(find.byKey(const Key('aplicar-brilho')));
    await waitFor(tester, find.byType(ConfirmAdjustmentDialog));
    expect(File(audit).existsSync(), isFalse, reason: 'preparar não pode enviar nem registrar nada');
    await wait(tester, ConfirmAdjustmentDialog.armDelay + const Duration(milliseconds: 400));
    await tester.tap(find.byKey(const Key('confirmar')));
    await waitFor(tester, find.text('brilho 5 aplicado'));
    final lines = File(audit).readAsLinesSync();
    expect(lines.length, 1);
    expect(lines.single, contains('"resultado":"ok"'));
    expect(c.state.value!.brightness, 5);

    await tester.runAsync(() => c.stop());
    dir.deleteSync(recursive: true);
  });
}
