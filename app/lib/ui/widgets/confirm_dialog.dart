import 'dart:async';

import 'package:material_ui/material_ui.dart';

import '../../src/rust/api/glasses.dart';

/// Mostra o que vai ser enviado, o valor atual, como desfazer e os avisos.
/// Devolve true só se o usuário tocar em "Confirmar" antes de a proposta vencer.
class ConfirmAdjustmentDialog extends StatefulWidget {
  const ConfirmAdjustmentDialog({super.key, required this.proposal});

  final ProposalDto proposal;

  /// Tempo antes de o botão Confirmar ficar ativo (evita toque duplo acidental).
  static const armDelay = Duration(milliseconds: 750); // múltiplo do tique de 250 ms

  @override
  State<ConfirmAdjustmentDialog> createState() => _ConfirmAdjustmentDialogState();
}

class _ConfirmAdjustmentDialogState extends State<ConfirmAdjustmentDialog> {
  static const _tick = Duration(milliseconds: 250);
  late final Timer _timer;
  late int _leftMs;
  bool _armed = false;

  @override
  void initState() {
    super.initState();
    // margem de 1 s para a confirmação chegar ao núcleo antes de a proposta vencer lá
    _leftMs = widget.proposal.expiresInMs - 1000;
    var elapsed = Duration.zero;
    _timer = Timer.periodic(_tick, (_) {
      if (!mounted) return;
      elapsed += _tick;
      _leftMs -= _tick.inMilliseconds;
      if (_leftMs <= 0) {
        _timer.cancel();
        Navigator.of(context).pop(false);
        return;
      }
      setState(() => _armed = elapsed >= ConfirmAdjustmentDialog.armDelay);
    });
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.proposal;
    final left = (_leftMs / 1000).ceil().clamp(0, 999);
    final t = Theme.of(context).textTheme;
    return AlertDialog(
      icon: const Icon(Icons.tune),
      title: const Text('Confirmar ajuste'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(p.description, style: t.titleMedium),
          const SizedBox(height: 12),
          _row(context, 'Valor atual', p.current),
          _row(context, 'Como desfazer', p.undoText),
          for (final w in p.warnings)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.warning_amber, size: 18, color: Theme.of(context).colorScheme.error),
                  const SizedBox(width: 8),
                  Expanded(child: Text(w)),
                ],
              ),
            ),
          const SizedBox(height: 12),
          Text('Esta proposta vence em $left s.', style: t.bodySmall),
        ],
      ),
      actions: [
        TextButton(
          key: const Key('cancelar'),
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancelar'),
        ),
        FilledButton(
          key: const Key('confirmar'),
          onPressed: _armed ? () => Navigator.of(context).pop(true) : null,
          child: const Text('Confirmar'),
        ),
      ],
    );
  }

  Widget _row(BuildContext context, String label, String value) => Padding(
    padding: const EdgeInsets.only(bottom: 4),
    child: Text.rich(
      TextSpan(
        children: [
          TextSpan(text: '$label: ', style: const TextStyle(fontWeight: FontWeight.w600)),
          TextSpan(text: value),
        ],
      ),
    ),
  );
}
