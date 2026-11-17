import 'package:material_ui/material_ui.dart';

import '../core/session_controller.dart';
import '../src/rust/api/glasses.dart';
import 'format.dart';

class BotoesPage extends StatefulWidget {
  const BotoesPage({super.key, required this.controller});
  final SessionController controller;

  @override
  State<BotoesPage> createState() => _BotoesPageState();
}

class _BotoesPageState extends State<BotoesPage> {
  bool _showSensors = false;

  static IconData _icon(EventKind k) => switch (k) {
    EventKind.key => Icons.radio_button_checked,
    EventKind.action => Icons.touch_app,
    EventKind.sensor => Icons.thermostat,
    EventKind.unknown => Icons.help_outline,
    EventKind.adjustmentOk => Icons.check_circle,
    EventKind.adjustmentFailed => Icons.error,
  };

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: Row(
            children: [
              FilterChip(
                label: const Text('Mostrar temperaturas'),
                selected: _showSensors,
                onSelected: (v) => setState(() => _showSensors = v),
              ),
              const Spacer(),
              TextButton.icon(
                onPressed: widget.controller.clearEvents,
                icon: const Icon(Icons.delete_sweep),
                label: const Text('Limpar'),
              ),
            ],
          ),
        ),
        Expanded(
          child: ValueListenableBuilder<List<EventEntry>>(
            valueListenable: widget.controller.events,
            builder: (context, all, _) {
              final list = all.where((e) => _showSensors || e.event.kind != EventKind.sensor).toList().reversed.toList();
              if (list.isEmpty) {
                return const Center(child: Text('Aperte um botão do óculos para ver os eventos aqui.'));
              }
              return ListView.builder(
                itemCount: list.length,
                itemBuilder: (context, i) {
                  final e = list[i];
                  return ListTile(
                    dense: true,
                    leading: Icon(_icon(e.event.kind)),
                    title: Text(e.event.text),
                    trailing: Text(hms(e.time), style: Theme.of(context).textTheme.bodySmall),
                  );
                },
              );
            },
          ),
        ),
      ],
    );
  }
}
