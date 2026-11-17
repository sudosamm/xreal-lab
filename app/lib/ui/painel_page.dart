import 'package:material_ui/material_ui.dart';

import '../core/session_controller.dart';
import '../src/rust/api/glasses.dart';
import 'format.dart';
import 'widgets/sparkline.dart';

class PainelPage extends StatelessWidget {
  const PainelPage({super.key, required this.controller});
  final SessionController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<StateDto?>(
      valueListenable: controller.state,
      builder: (context, s, _) {
        final cards = <Widget>[
          _ConexaoCard(s: s),
          _AparelhoCard(s: s),
          _TaxasCard(s: s),
          _TemperaturasCard(s: s, controller: controller),
          _NiveisCard(s: s),
        ];
        return LayoutBuilder(
          builder: (context, c) {
            final cols = c.maxWidth >= 1000 ? 3 : (c.maxWidth >= 640 ? 2 : 1);
            return SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Wrap(
                spacing: 16,
                runSpacing: 16,
                children: [
                  for (final card in cards)
                    SizedBox(width: (c.maxWidth - 32 - 16 * (cols - 1)) / cols, child: card),
                ],
              ),
            );
          },
        );
      },
    );
  }
}

class InfoCard extends StatelessWidget {
  const InfoCard({super.key, required this.title, required this.icon, required this.children});
  final String title;
  final IconData icon;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(icon, size: 20, color: Theme.of(context).colorScheme.primary),
                const SizedBox(width: 8),
                Text(title, style: Theme.of(context).textTheme.titleMedium),
              ],
            ),
            const SizedBox(height: 12),
            ...children,
          ],
        ),
      ),
    );
  }
}

class KeyValue extends StatelessWidget {
  const KeyValue(this.label, this.value, {super.key});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 130,
            child: Text(label, style: TextStyle(color: Theme.of(context).colorScheme.onSurfaceVariant)),
          ),
          Expanded(child: SelectableText(value)),
        ],
      ),
    );
  }
}

class _ConexaoCard extends StatelessWidget {
  const _ConexaoCard({required this.s});
  final StateDto? s;

  @override
  Widget build(BuildContext context) {
    Widget link(String name, String? host) {
      final ok = host != null;
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 3),
        child: Row(
          children: [
            Icon(Icons.circle, size: 12, color: ok ? Colors.green : Theme.of(context).colorScheme.error),
            const SizedBox(width: 8),
            SizedBox(width: 122, child: Text(name)),
            Text(ok ? host : 'desconectado'),
          ],
        ),
      );
    }

    return InfoCard(
      title: s?.simulated == true ? 'Conexão (simulado)' : 'Conexão',
      icon: Icons.usb,
      children: [
        link('Controle (52999)', s?.controlHost),
        link('IMU (52998)', s?.imuHost),
        link('Quadros (52996)', s?.vsyncHost),
      ],
    );
  }
}

class _AparelhoCard extends StatelessWidget {
  const _AparelhoCard({required this.s});
  final StateDto? s;

  @override
  Widget build(BuildContext context) {
    return InfoCard(
      title: 'Aparelho',
      icon: Icons.visibility,
      children: [
        KeyValue('Firmware', s?.swVersion ?? '—'),
        KeyValue('DSP', s?.dspVersion ?? '—'),
        KeyValue('Sensor de rosto', onOff(s?.proximityEnabled)),
        KeyValue('Estabilização', onOff(s?.eisEnabled)),
        KeyValue('Calibração', s?.calibrationLoaded == true ? 'carregada' : '—'),
      ],
    );
  }
}

class _TaxasCard extends StatelessWidget {
  const _TaxasCard({required this.s});
  final StateDto? s;

  @override
  Widget build(BuildContext context) {
    final big = Theme.of(context).textTheme.headlineMedium;
    Widget rate(String label, double? v) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(v == null ? '—' : fmt(v, 0), style: big),
          Text(label),
        ],
      ),
    );
    return InfoCard(
      title: 'Taxas',
      icon: Icons.speed,
      children: [
        Row(children: [rate('IMU por segundo', s?.imuHz), rate('quadros por segundo', s?.vsyncHz)]),
      ],
    );
  }
}

class _TemperaturasCard extends StatelessWidget {
  const _TemperaturasCard({required this.s, required this.controller});
  final StateDto? s;
  final SessionController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<List<TempSample>>(
      valueListenable: controller.temps,
      builder: (context, hist, _) {
        Widget row(int i) {
          final v = (s != null && s!.temperatures.length > i) ? s!.temperatures[i].toDouble() : double.nan;
          return Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              children: [
                SizedBox(width: 80, child: Text('Sensor $i')),
                SizedBox(width: 70, child: Text(v.isNaN ? '—' : '${fmt(v)} °C')),
                Expanded(child: Sparkline(values: [for (final h in hist) if (h.values.length > i) h.values[i]])),
              ],
            ),
          );
        }

        return InfoCard(
          title: 'Temperaturas',
          icon: Icons.thermostat,
          children: [
            row(0),
            row(1),
            row(2),
            Text(
              'Prováveis; chegam a cada ~10 s. Histórico dos últimos 10 min.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        );
      },
    );
  }
}

class _NiveisCard extends StatelessWidget {
  const _NiveisCard({required this.s});
  final StateDto? s;

  @override
  Widget build(BuildContext context) {
    return InfoCard(
      title: 'Brilho e filtro',
      icon: Icons.brightness_6,
      children: [
        KeyValue('Brilho', s?.brightness == null ? 'desconhecido' : '${s!.brightness} de 9'),
        KeyValue('Filtro', s?.dimmer == null ? 'desconhecido' : '${s!.dimmer} de 2'),
        Text(
          'O óculos não informa o valor ao conectar: aparece depois que alguém mexe nos botões ou ajusta por aqui.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ],
    );
  }
}
