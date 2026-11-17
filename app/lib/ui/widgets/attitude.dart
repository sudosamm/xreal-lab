import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

/// Horizonte artificial: o céu e o chão giram com a rolagem e sobem/descem com a arfagem.
class AttitudeIndicator extends StatelessWidget {
  const AttitudeIndicator({super.key, required this.pitch, required this.roll});

  final double pitch;
  final double roll;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final label = Theme.of(context).textTheme.labelSmall ?? const TextStyle(fontSize: 11);
    return AspectRatio(
      aspectRatio: 1,
      child: CustomPaint(painter: _AttitudePainter(pitch, roll, cs, label)),
    );
  }
}

class _AttitudePainter extends CustomPainter {
  _AttitudePainter(this.pitch, this.roll, this.cs, this.label);
  final double pitch;
  final double roll;
  final ColorScheme cs;
  final TextStyle label;

  static const _degPerRadius = 45.0; // 45° de arfagem = um raio

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final r = size.shortestSide / 2;
    canvas.save();
    canvas.clipPath(Path()..addOval(Rect.fromCircle(center: c, radius: r)));
    canvas.translate(c.dx, c.dy);
    // rolagem + (orelha direita para baixo) = o horizonte gira no sentido anti-horário na tela
    canvas.rotate(-roll * math.pi / 180);
    final shift = pitch / _degPerRadius * r; // olhar para cima: horizonte desce
    final big = r * 4;
    canvas.drawRect(Rect.fromLTRB(-big, -big, big, shift), Paint()..color = const Color(0xFF3B7DD8));
    canvas.drawRect(Rect.fromLTRB(-big, shift, big, big), Paint()..color = const Color(0xFF8A5A2B));
    final line = Paint()
      ..color = Colors.white
      ..strokeWidth = 2;
    canvas.drawLine(Offset(-big, shift), Offset(big, shift), line);
    final tp = TextPainter(textDirection: TextDirection.ltr);
    for (var d = -60; d <= 60; d += 10) {
      if (d == 0) continue;
      final y = shift - d / _degPerRadius * r;
      final w = d % 20 == 0 ? r * 0.35 : r * 0.18;
      canvas.drawLine(Offset(-w / 2, y), Offset(w / 2, y), line..strokeWidth = 1.5);
      if (d % 20 == 0) {
        tp.text = TextSpan(text: '$d', style: label.copyWith(color: Colors.white));
        tp.layout();
        tp.paint(canvas, Offset(w / 2 + 4, y - tp.height / 2));
      }
    }
    canvas.restore();
    // referência fixa (o óculos)
    final ref = Paint()
      ..color = Colors.amber
      ..strokeWidth = 4
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(c.translate(-r * 0.5, 0), c.translate(-r * 0.15, 0), ref);
    canvas.drawLine(c.translate(r * 0.15, 0), c.translate(r * 0.5, 0), ref);
    canvas.drawCircle(c, 4, ref);
    canvas.drawCircle(
      c,
      r - 1,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..color = cs.outline,
    );
  }

  @override
  bool shouldRepaint(_AttitudePainter old) => old.pitch != pitch || old.roll != roll || old.cs != cs;
}

/// Bússola da guinada (relativa ao último "recentrar").
class HeadingDial extends StatelessWidget {
  const HeadingDial({super.key, required this.yaw});
  final double yaw;

  @override
  Widget build(BuildContext context) {
    final label = Theme.of(context).textTheme.labelSmall ?? const TextStyle(fontSize: 11);
    return AspectRatio(
      aspectRatio: 1,
      child: CustomPaint(painter: _HeadingPainter(yaw, Theme.of(context).colorScheme, label)),
    );
  }
}

class _HeadingPainter extends CustomPainter {
  _HeadingPainter(this.yaw, this.cs, this.label);
  final double yaw;
  final ColorScheme cs;
  final TextStyle label;

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final r = size.shortestSide / 2 - 2;
    canvas.drawCircle(c, r, Paint()..color = cs.surfaceContainerHighest);
    canvas.drawCircle(
      c,
      r,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..color = cs.outline,
    );
    final tick = Paint()
      ..color = cs.onSurfaceVariant
      ..strokeWidth = 1.5;
    final tp = TextPainter(textDirection: TextDirection.ltr);
    for (var d = 0; d < 360; d += 30) {
      final a = (d - yaw - 90) * math.pi / 180; // o mostrador gira; a seta fica parada no topo
      final dir = Offset(math.cos(a), math.sin(a));
      canvas.drawLine(c + dir * (r * 0.82), c + dir * r, tick);
      final text = d == 0 ? '0' : (d <= 180 ? '$d' : '${d - 360}');
      tp.text = TextSpan(text: text, style: label.copyWith(color: cs.onSurface));
      tp.layout();
      final p = c + dir * (r * 0.66);
      tp.paint(canvas, p - Offset(tp.width / 2, tp.height / 2));
    }
    final arrow = Path()
      ..moveTo(c.dx, c.dy - r * 0.95)
      ..lineTo(c.dx - 8, c.dy - r * 0.75)
      ..lineTo(c.dx + 8, c.dy - r * 0.75)
      ..close();
    canvas.drawPath(arrow, Paint()..color = Colors.amber);
  }

  @override
  bool shouldRepaint(_HeadingPainter old) => old.yaw != yaw || old.cs != cs;
}
