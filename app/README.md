# xr-lab (app)

Interface em Flutter para Linux e Android. A lógica fica no núcleo Rust em `../core`, ligado por flutter_rust_bridge (crate `rust/`, gerado com `flutter_rust_bridge_codegen generate`).

- `lib/core/`: interface com o núcleo (`GlassesApi`) e estado da sessão.
- `lib/ui/`: telas Painel, Orientação, Botões e Ajustes, e o diálogo de confirmação.
- `test/`: testes com uma API falsa e capturas de tela (`test/goldens/`).
- `integration_test/`: ponta a ponta com o núcleo real e o óculos simulado.

```bash
flutter run -d linux --dart-entrypoint-args=--sim
```
