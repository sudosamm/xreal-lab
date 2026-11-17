import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:xr_lab/core/session_controller.dart';
import 'package:xr_lab/src/rust/api/glasses.dart';
import 'package:xr_lab/ui/app.dart';
import 'package:xr_lab/ui/widgets/confirm_dialog.dart';

import 'fake_glasses_api.dart';

Future<(SessionController, FakeGlassesApi)> boot(
  WidgetTester tester, {
  Size size = const Size(1200, 800),
  int page = 0,
  int expiresInMs = 30000,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final fake = FakeGlassesApi(expiresInMs: expiresInMs);
  final c = SessionController(({required bool simulated}) async => fake);
  await c.start(simulated: false);
  await tester.pumpWidget(
    MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: ThemeData(colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF00897B)), useMaterial3: true),
      home: HomeShell(controller: c, initialIndex: page),
    ),
  );
  fake.emitState();
  await settle(tester);
  return (c, fake);
}

/// Os fluxos entregam no microtask seguinte: um quadro para receber, outro para desenhar.
Future<void> settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump();
}

Future<void> tapKey(WidgetTester tester, String key) async {
  final f = find.byKey(Key(key));
  await tester.ensureVisible(f);
  await tester.pump();
  await tester.tap(f);
  await settle(tester);
}

const orient = OrientationDto(
  yaw: 12.5,
  pitch: 8.0,
  roll: -15.0,
  ratePitch: 0.1,
  rateYaw: -0.2,
  rateRoll: 0.0,
  still: true,
  tempC: 37.0,
);

void main() {
  testWidgets('painel mostra aparelho, conexão e taxas', (tester) async {
    await boot(tester);
    expect(find.text('1.9.1.20260914175625'), findsOneWidget);
    expect(find.text('15.A.01.097_20250430'), findsOneWidget);
    expect(find.text('169.254.2.1'), findsNWidgets(3));
    expect(find.text('1000'), findsOneWidget); // 999,6 arredondado
    expect(find.text('51,0 °C'), findsOneWidget);
    expect(find.text('9 de 9'), findsOneWidget);
  });

  testWidgets('ajustes ficam travados com o modo desligado', (tester) async {
    final (_, fake) = await boot(tester, page: 3);
    final aplicar = tester.widget<FilledButton>(find.byKey(const Key('aplicar-brilho')));
    expect(aplicar.onPressed, isNull);
    await tester.tap(find.byKey(const Key('aplicar-brilho')));
    await tester.pump();
    expect(fake.prepared, isEmpty);
    expect(find.byType(ConfirmAdjustmentDialog), findsNothing);
  });

  testWidgets('cancelar no diálogo não envia nada', (tester) async {
    final (_, fake) = await boot(tester, page: 3);
    await tapKey(tester, 'modo-ajustes');
    await tapKey(tester, 'aplicar-brilho');
    expect(find.byType(ConfirmAdjustmentDialog), findsOneWidget);
    expect(find.text('Valor atual: '), findsNothing); // texto vem em spans
    expect(find.textContaining('voltar para brilho 9'), findsOneWidget);
    // o botão Confirmar começa desarmado
    expect(tester.widget<FilledButton>(find.byKey(const Key('confirmar'))).onPressed, isNull);
    await tester.tap(find.byKey(const Key('cancelar')));
    await tester.pump(const Duration(seconds: 1));
    expect(fake.prepared.length, 1);
    expect(fake.confirmed, isEmpty);
    expect(fake.cancels, 1);
    expect(find.text('Ajuste cancelado. Nada foi enviado.'), findsOneWidget);
    await tester.pump(const Duration(seconds: 10));
  });

  testWidgets('confirmar envia uma vez e oferece desfazer', (tester) async {
    final (_, fake) = await boot(tester, page: 3);
    await tapKey(tester, 'modo-ajustes');
    await tapKey(tester, 'aplicar-brilho');
    await tester.pump(ConfirmAdjustmentDialog.armDelay + const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const Key('confirmar')));
    await settle(tester);
    await settle(tester);
    expect(fake.confirmed, [1]);
    expect(find.text('brilho 9 aplicado'), findsOneWidget);
    expect(find.text('Desfazer'), findsOneWidget);
    await tester.pump(const Duration(seconds: 10));
  });

  testWidgets('proposta vencida fecha o diálogo sem enviar', (tester) async {
    final (_, fake) = await boot(tester, page: 3, expiresInMs: 2500);
    await tapKey(tester, 'modo-ajustes');
    await tapKey(tester, 'aplicar-filtro');
    expect(find.byType(ConfirmAdjustmentDialog), findsOneWidget);
    await tester.pump(const Duration(seconds: 2));
    await tester.pump(const Duration(milliseconds: 500));
    await settle(tester);
    expect(find.byType(ConfirmAdjustmentDialog), findsNothing);
    expect(fake.confirmed, isEmpty);
    expect(fake.cancels, 1);
    await tester.pump(const Duration(seconds: 10));
  });

  testWidgets('eventos aparecem na tela de botões, sem temperaturas por padrão', (tester) async {
    final (_, fake) = await boot(tester, page: 2);
    fake.emitEvent(const EventDto(kind: EventKind.key, text: 'tecla: botão inferior (menu) apertado (t=1)'));
    fake.emitEvent(const EventDto(kind: EventKind.action, text: 'ação: brilho − → nível 8 (t=2.000s)', brightness: 8));
    fake.emitEvent(const EventDto(kind: EventKind.sensor, text: 'sensor 0 = 51.0 (provável temperatura, °C)'));
    await settle(tester);
    expect(find.textContaining('menu) apertado'), findsOneWidget);
    expect(find.textContaining('nível 8'), findsOneWidget);
    expect(find.textContaining('sensor 0'), findsNothing);
    await tester.tap(find.text('Mostrar temperaturas'));
    await settle(tester);
    expect(find.textContaining('sensor 0'), findsOneWidget);
  });

  testWidgets('orientação mostra os ângulos e recentra', (tester) async {
    final (_, fake) = await boot(tester, page: 1);
    fake.emitOrientation(orient);
    await settle(tester);
    expect(find.text('8,0°'), findsOneWidget);
    expect(find.text('-15,0°'), findsOneWidget);
    await tester.tap(find.byKey(const Key('recentrar')));
    expect(fake.recenters, 1);
  });

  group('capturas', () {
    Future<void> shot(WidgetTester tester, String name) async {
      await expectLater(find.byType(HomeShell), matchesGoldenFile('goldens/$name.png'));
    }

    testWidgets('painel', (tester) async {
      final (c, fake) = await boot(tester);
      for (final t in [50.0, 50.4, 50.9, 51.0]) {
        fake.emitState(sampleState()..temperatures[0] = t);
        fake.emitEvent(const EventDto(kind: EventKind.sensor, text: 's'));
        await settle(tester);
      }
      await shot(tester, 'painel');
      c.dispose();
    });

    testWidgets('orientacao', (tester) async {
      final (_, fake) = await boot(tester, page: 1);
      fake.emitOrientation(orient);
      await settle(tester);
      await shot(tester, 'orientacao');
    });

    testWidgets('ajustes com diálogo', (tester) async {
      final (_, fake) = await boot(tester, page: 3);
      await tapKey(tester, 'modo-ajustes');
      await tapKey(tester, 'aplicar-modo');
      await tester.pump(const Duration(seconds: 1));
      await expectLater(find.byType(MaterialApp), matchesGoldenFile('goldens/ajustes-dialogo.png'));
      await tester.tap(find.byKey(const Key('cancelar')));
      await tester.pump(const Duration(seconds: 10));
      expect(fake.confirmed, isEmpty);
    });

    testWidgets('painel no celular', (tester) async {
      await boot(tester, size: const Size(412, 900));
      await shot(tester, 'painel-celular');
    });
  });
}
