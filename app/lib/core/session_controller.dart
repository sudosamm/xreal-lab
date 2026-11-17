// Guarda o último estado recebido do núcleo e um histórico curto para a interface.
import 'dart:async';
import 'dart:collection';

import 'package:material_ui/material_ui.dart';

import '../src/rust/api/glasses.dart';
import 'glasses_api.dart';

class EventEntry {
  EventEntry(this.time, this.event);
  final DateTime time;
  final EventDto event;
}

class TempSample {
  TempSample(this.time, this.values);
  final DateTime time;
  final List<double> values;
}

class SessionController {
  SessionController(this._factory);

  final GlassesApiFactory _factory;
  GlassesApi? _api;
  final List<StreamSubscription<Object?>> _subs = [];

  final state = ValueNotifier<StateDto?>(null);
  final orientation = ValueNotifier<OrientationDto?>(null);
  final events = ValueNotifier<UnmodifiableListView<EventEntry>>(UnmodifiableListView(const []));
  final temps = ValueNotifier<UnmodifiableListView<TempSample>>(UnmodifiableListView(const []));
  final simulated = ValueNotifier<bool>(false);
  final error = ValueNotifier<String?>(null);

  static const maxEvents = 500;
  static const tempWindow = Duration(minutes: 10);

  GlassesApi? get api => _api;

  Future<void> start({required bool simulated}) async {
    await stop();
    this.simulated.value = simulated;
    try {
      final api = await _factory(simulated: simulated);
      _api = api;
      error.value = null;
      _subs.add(api.watchState().listen(_onState));
      _subs.add(api.watchOrientation().listen((o) => orientation.value = o));
      _subs.add(api.watchEvents().listen(_onEvent));
    } catch (e) {
      error.value = 'Não foi possível iniciar: $e';
    }
  }

  Future<void> stop() async {
    for (final s in _subs) {
      await s.cancel();
    }
    _subs.clear();
    _api?.dispose();
    _api = null;
    state.value = null;
    orientation.value = null;
  }

  void _onState(StateDto s) {
    state.value = s;
  }

  void _onEvent(EventDto e) {
    final now = DateTime.now();
    final list = List<EventEntry>.of(events.value)..add(EventEntry(now, e));
    if (list.length > maxEvents) list.removeRange(0, list.length - maxEvents);
    events.value = UnmodifiableListView(list);
    if (e.kind == EventKind.sensor) {
      final s = state.value;
      if (s != null) {
        final t = List<TempSample>.of(temps.value)
          ..add(TempSample(now, s.temperatures.map((v) => v.toDouble()).toList()));
        t.removeWhere((x) => now.difference(x.time) > tempWindow);
        temps.value = UnmodifiableListView(t);
      }
    }
  }

  void clearEvents() => events.value = UnmodifiableListView(const []);

  void dispose() {
    stop();
  }
}
