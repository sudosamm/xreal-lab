// Números no jeito brasileiro (vírgula decimal).
String fmt(double v, [int digits = 1]) => v.isNaN ? '—' : v.toStringAsFixed(digits).replaceAll('.', ',');

String onOff(bool? v) => v == null ? '—' : (v ? 'ligado' : 'desligado');

String hms(DateTime t) {
  String two(int n) => n.toString().padLeft(2, '0');
  return '${two(t.hour)}:${two(t.minute)}:${two(t.second)}.${t.millisecond.toString().padLeft(3, '0')}';
}
