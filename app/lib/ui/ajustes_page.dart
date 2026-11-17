import 'package:material_ui/material_ui.dart';

import '../core/session_controller.dart';
import '../src/rust/api/glasses.dart';
import 'widgets/confirm_dialog.dart';

/// Ajustes voláteis. Cada um passa por: preparar (nada é enviado) → diálogo → confirmar.
class AjustesPage extends StatefulWidget {
  const AjustesPage({super.key, required this.controller});
  final SessionController controller;

  @override
  State<AjustesPage> createState() => _AjustesPageState();
}

class _AjustesPageState extends State<AjustesPage> {
  int? _brightness;
  int _dimmer = 0;
  int _buttons = 0;
  int _sideBySide = 0;
  bool _busy = false;

  SessionController get c => widget.controller;

  Future<void> apply(AdjustmentDto a) async {
    final api = c.api;
    if (api == null || _busy) return;
    setState(() => _busy = true);
    final messenger = ScaffoldMessenger.of(context);
    try {
      final ProposalDto p;
      try {
        p = await api.prepare(a);
      } catch (e) {
        messenger.showSnackBar(SnackBar(content: Text('Não foi possível preparar: $e')));
        return;
      }
      if (!mounted) return;
      final ok = await showDialog<bool>(
        context: context,
        barrierDismissible: false,
        builder: (_) => ConfirmAdjustmentDialog(proposal: p),
      );
      if (ok != true) {
        await api.cancel();
        messenger.showSnackBar(const SnackBar(content: Text('Ajuste cancelado. Nada foi enviado.')));
        return;
      }
      try {
        final msg = await api.confirm(p.id);
        final undo = p.undo;
        messenger.showSnackBar(
          SnackBar(
            content: Text(msg),
            duration: const Duration(seconds: 8),
            action: undo == null ? null : SnackBarAction(label: 'Desfazer', onPressed: () => apply(undo)),
          ),
        );
      } catch (e) {
        messenger.showSnackBar(SnackBar(content: Text('Falhou: $e')));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<StateDto?>(
      valueListenable: c.state,
      builder: (context, s, _) {
        final enabled = s?.adjustmentsEnabled ?? false;
        final blocked = s?.adjustmentsBlocked;
        final canApply = s != null && enabled && blocked == null && !_busy;
        final brightness = _brightness ?? s?.brightness ?? 5;
        final err = Theme.of(context).colorScheme.error;
        return ListView(
          padding: const EdgeInsets.all(16),
          children: [
            Card(
              child: SwitchListTile(
                key: const Key('modo-ajustes'),
                title: const Text('Modo ajustes'),
                subtitle: Text(
                  enabled && blocked != null
                      ? 'Bloqueado: $blocked'
                      : 'Começa desligado e desliga ao perder a conexão. Cada ajuste pede confirmação e fica registrado.',
                  style: enabled && blocked != null ? TextStyle(color: err) : null,
                ),
                value: enabled,
                onChanged: s == null ? null : (v) => c.api?.setAdjustmentsEnabled(v),
              ),
            ),
            const SizedBox(height: 8),
            _Section(
              title: 'Brilho',
              current: s?.brightness == null ? 'atual: desconhecido' : 'atual: ${s!.brightness}',
              control: Row(
                children: [
                  Expanded(
                    child: Slider(
                      key: const Key('brilho'),
                      value: brightness.toDouble(),
                      min: 0,
                      max: 9,
                      divisions: 9,
                      label: '$brightness',
                      onChanged: canApply ? (v) => setState(() => _brightness = v.round()) : null,
                    ),
                  ),
                  Text('$brightness'),
                ],
              ),
              onApply: canApply
                  ? () => apply(AdjustmentDto(kind: AdjustmentKind.brightness, value: brightness))
                  : null,
              applyKey: const Key('aplicar-brilho'),
            ),
            _Section(
              title: 'Filtro eletrocrômico',
              current: s?.dimmer == null ? 'atual: desconhecido' : 'atual: ${s!.dimmer}',
              control: SegmentedButton<int>(
                segments: const [
                  ButtonSegment(value: 0, label: Text('0')),
                  ButtonSegment(value: 1, label: Text('1')),
                  ButtonSegment(value: 2, label: Text('2')),
                ],
                selected: {_dimmer},
                onSelectionChanged: canApply ? (v) => setState(() => _dimmer = v.first) : null,
              ),
              onApply: canApply ? () => apply(AdjustmentDto(kind: AdjustmentKind.dimmer, value: _dimmer)) : null,
              applyKey: const Key('aplicar-filtro'),
              note: 'Com o escurecimento automático ligado, o óculos pode não mudar a lente.',
            ),
            _Section(
              title: 'Botões do óculos',
              control: SegmentedButton<int>(
                segments: const [
                  ButtonSegment(value: 0, label: Text('Ativos')),
                  ButtonSegment(value: 1, label: Text('Desativados')),
                ],
                selected: {_buttons},
                onSelectionChanged: canApply ? (v) => setState(() => _buttons = v.first) : null,
              ),
              onApply: canApply ? () => apply(AdjustmentDto(kind: AdjustmentKind.buttons, value: _buttons)) : null,
              applyKey: const Key('aplicar-botoes'),
            ),
            _Section(
              title: 'Modo de tela',
              control: SegmentedButton<int>(
                segments: const [
                  ButtonSegment(value: 0, label: Text('Normal')),
                  ButtonSegment(value: 1, label: Text('Lado a lado')),
                ],
                selected: {_sideBySide},
                onSelectionChanged: canApply ? (v) => setState(() => _sideBySide = v.first) : null,
              ),
              onApply: canApply
                  ? () => apply(AdjustmentDto(kind: AdjustmentKind.sideBySide, value: _sideBySide))
                  : null,
              applyKey: const Key('aplicar-modo'),
              note: 'A tela pode ficar apagada por cerca de 12 s.',
            ),
            const SizedBox(height: 8),
            _Historico(controller: c),
          ],
        );
      },
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({
    required this.title,
    required this.control,
    required this.onApply,
    required this.applyKey,
    this.current,
    this.note,
  });

  final String title;
  final String? current;
  final String? note;
  final Widget control;
  final VoidCallback? onApply;
  final Key applyKey;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(title, style: t.titleMedium),
                const Spacer(),
                if (current != null) Text(current!, style: t.bodySmall),
              ],
            ),
            const SizedBox(height: 8),
            control,
            if (note != null) Padding(padding: const EdgeInsets.only(top: 6), child: Text(note!, style: t.bodySmall)),
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton.tonal(key: applyKey, onPressed: onApply, child: const Text('Aplicar…')),
            ),
          ],
        ),
      ),
    );
  }
}

class _Historico extends StatelessWidget {
  const _Historico({required this.controller});
  final SessionController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<List<EventEntry>>(
      valueListenable: controller.events,
      builder: (context, all, _) {
        final list = all
            .where((e) => e.event.kind == EventKind.adjustmentOk || e.event.kind == EventKind.adjustmentFailed)
            .toList()
            .reversed;
        if (list.isEmpty) return const SizedBox.shrink();
        return Card(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                child: Text('Enviados nesta sessão', style: Theme.of(context).textTheme.titleMedium),
              ),
              for (final e in list)
                ListTile(
                  dense: true,
                  leading: Icon(e.event.kind == EventKind.adjustmentOk ? Icons.check_circle : Icons.error),
                  title: Text(e.event.text),
                ),
            ],
          ),
        );
      },
    );
  }
}
