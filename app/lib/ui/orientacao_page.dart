import 'package:material_ui/material_ui.dart';

import '../core/session_controller.dart';
import '../src/rust/api/glasses.dart';
import 'format.dart';
import 'widgets/attitude.dart';

class OrientacaoPage extends StatelessWidget {
  const OrientacaoPage({super.key, required this.controller});
  final SessionController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<OrientationDto?>(
      valueListenable: controller.orientation,
      builder: (context, o, _) {
        if (o == null) {
          return const Center(child: Text('Aguardando a IMU…'));
        }
        final big = Theme.of(context).textTheme.headlineSmall;
        Widget value(String label, double v) => Column(
          children: [
            Text('${fmt(v)}°', style: big),
            Text(label),
          ],
        );
        final numbers = Wrap(
          spacing: 32,
          runSpacing: 12,
          alignment: WrapAlignment.center,
          children: [value('arfagem', o.pitch), value('rolagem', o.roll), value('guinada', o.yaw)],
        );
        final rates = Text(
          'velocidade (°/s): arfagem ${fmt(o.ratePitch)} · guinada ${fmt(o.rateYaw)} · rolagem ${fmt(o.rateRoll)}',
          textAlign: TextAlign.center,
        );
        final actions = Wrap(
          spacing: 12,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Chip(
              avatar: Icon(o.still ? Icons.pause_circle : Icons.motion_photos_on, size: 18),
              label: Text(o.still ? 'parado (aprendendo o desvio)' : 'em movimento'),
            ),
            FilledButton.icon(
              key: const Key('recentrar'),
              onPressed: () => controller.api?.recenter(),
              icon: const Icon(Icons.center_focus_strong),
              label: const Text('Recentrar'),
            ),
          ],
        );
        return LayoutBuilder(
          builder: (context, c) {
            final wide = c.maxWidth >= 720;
            final dialSize = (wide ? c.maxWidth / 3 : c.maxWidth * 0.6).clamp(160.0, 360.0);
            final dials = [
              SizedBox(width: dialSize, child: AttitudeIndicator(pitch: o.pitch, roll: o.roll)),
              SizedBox(width: dialSize * 0.7, child: HeadingDial(yaw: o.yaw)),
            ];
            return SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: Column(
                children: [
                  wide
                      ? Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: dials)
                      : Column(children: [dials[0], const SizedBox(height: 16), dials[1]]),
                  const SizedBox(height: 24),
                  numbers,
                  const SizedBox(height: 12),
                  rates,
                  const SizedBox(height: 16),
                  actions,
                  const SizedBox(height: 8),
                  Text(
                    'A guinada é relativa: deriva devagar e volta a zero em "Recentrar".',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ],
              ),
            );
          },
        );
      },
    );
  }
}
