// Interface entre a tela e o núcleo Rust. Os testes usam uma versão falsa.
import 'dart:async';

import '../src/rust/api/glasses.dart';

abstract class GlassesApi {
  bool get simulated;
  Stream<StateDto> watchState();
  Stream<OrientationDto> watchOrientation();
  Stream<EventDto> watchEvents();
  void recenter();
  Future<void> setAdjustmentsEnabled(bool on);

  /// Etapa 1 do ajuste: só descreve. Nada é enviado ao óculos.
  Future<ProposalDto> prepare(AdjustmentDto adjustment);

  /// Etapa 2: chamada apenas pelo botão "Confirmar" do diálogo.
  Future<String> confirm(int proposalId);
  Future<void> cancel();
  void dispose();
}

typedef GlassesApiFactory = Future<GlassesApi> Function({required bool simulated});

class RustGlassesApi implements GlassesApi {
  RustGlassesApi._(this._session, this.simulated);

  final Session _session;
  @override
  final bool simulated;

  static Future<RustGlassesApi> start({required bool simulated, String? auditPath}) async {
    final s = await Session.start(
      config: SessionConfig(hosts: const [], simulated: simulated, auditPath: auditPath),
    );
    return RustGlassesApi._(s, simulated);
  }

  @override
  Stream<StateDto> watchState() => _session.watchState();
  @override
  Stream<OrientationDto> watchOrientation() => _session.watchOrientation();
  @override
  Stream<EventDto> watchEvents() => _session.watchEvents();
  @override
  void recenter() => _session.recenter();
  @override
  Future<void> setAdjustmentsEnabled(bool on) => _session.setAdjustmentsEnabled(on_: on);
  @override
  Future<ProposalDto> prepare(AdjustmentDto adjustment) => _session.prepare(adjustment: adjustment);
  @override
  Future<String> confirm(int proposalId) => _session.confirm(proposalId: proposalId);
  @override
  Future<void> cancel() => _session.cancel();
  @override
  void dispose() => _session.dispose();
}
