// API falsa para os testes da interface: anota tudo o que a tela pede.
import 'dart:async';
import 'dart:typed_data';

import 'package:xr_lab/core/glasses_api.dart';
import 'package:xr_lab/src/rust/api/glasses.dart';

StateDto sampleState({bool adjustmentsEnabled = false, String? blocked, int? brightness = 9, int? dimmer}) => StateDto(
  controlHost: '169.254.2.1',
  imuHost: '169.254.2.1',
  vsyncHost: '169.254.2.1',
  swVersion: '1.9.1.20260914175625',
  dspVersion: '15.A.01.097_20250430',
  proximityEnabled: true,
  eisEnabled: false,
  calibrationLoaded: true,
  temperatures: Float32List.fromList([51.0, 46.3, 37.3]),
  brightness: brightness,
  dimmer: dimmer,
  imuHz: 999.6,
  vsyncHz: 120.0,
  adjustmentsEnabled: adjustmentsEnabled,
  adjustmentsBlocked: adjustmentsEnabled ? blocked : 'modo ajustes desligado',
  simulated: false,
);

class FakeGlassesApi implements GlassesApi {
  FakeGlassesApi({this.expiresInMs = 30000});

  final int expiresInMs;
  final _state = StreamController<StateDto>.broadcast();
  final _orientation = StreamController<OrientationDto>.broadcast();
  final _events = StreamController<EventDto>.broadcast();

  StateDto current = sampleState();
  final prepared = <AdjustmentDto>[];
  final confirmed = <int>[];
  int cancels = 0;
  int recenters = 0;
  int _nextId = 1;
  bool disposed = false;

  @override
  bool get simulated => false;

  void emitState([StateDto? s]) {
    if (s != null) current = s;
    _state.add(current);
  }

  void emitOrientation(OrientationDto o) => _orientation.add(o);
  void emitEvent(EventDto e) => _events.add(e);

  @override
  Stream<StateDto> watchState() => _state.stream;
  @override
  Stream<OrientationDto> watchOrientation() => _orientation.stream;
  @override
  Stream<EventDto> watchEvents() => _events.stream;
  @override
  void recenter() => recenters++;

  int? network;
  @override
  void setNetwork(int? handle) => network = handle;

  @override
  Future<void> setAdjustmentsEnabled(bool on) async {
    emitState(sampleState(adjustmentsEnabled: on, brightness: current.brightness, dimmer: current.dimmer));
  }

  @override
  Future<ProposalDto> prepare(AdjustmentDto adjustment) async {
    if (!current.adjustmentsEnabled) throw 'modo ajustes desligado';
    prepared.add(adjustment);
    return ProposalDto(
      id: _nextId++,
      adjustment: adjustment,
      description: 'Mudar para brilho ${adjustment.value}',
      current: 'brilho 9',
      undoText: 'voltar para brilho 9',
      undo: const AdjustmentDto(kind: AdjustmentKind.brightness, value: 9),
      warnings: adjustment.kind == AdjustmentKind.sideBySide ? ['A tela pode ficar apagada por cerca de 12 s.'] : [],
      expiresInMs: expiresInMs,
    );
  }

  @override
  Future<String> confirm(int proposalId) async {
    confirmed.add(proposalId);
    return 'brilho ${prepared.last.value} aplicado';
  }

  @override
  Future<void> cancel() async => cancels++;

  @override
  void dispose() => disposed = true;
}
