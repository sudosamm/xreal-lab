# xreal-lab

Ferramentas e app próprio para usar o **XREAL One Pro** no Linux e no **XREAL Beam Pro**: painel do aparelho, orientação da cabeça ao vivo, registro dos botões e ajustes voláteis (brilho, filtro, botões, modo lado a lado) sempre com confirmação.

> Projeto independente, sem relação com a XREAL. Use por sua conta e risco.

## Segurança

- O app e as ferramentas **só leem** o óculos, a menos que você ligue o "Modo ajustes".
- Os ajustes são uma lista fechada de configurações voláteis. Cada um passa por duas etapas (preparar → confirmar num diálogo), vence em 30 s, só é liberado para versões de firmware testadas e fica registrado num arquivo JSONL.
- Nada aqui envia atualização de firmware, reboot, ativação, calibração, EDID ou HDCP, nem escreve na interface HID de atualização (`hid_ota`).

## Estrutura

| Pasta | Conteúdo |
| --- | --- |
| `core/xr-proto` | formato das mensagens (portas 52996, 52998, 52999) e a trava de ajustes; sem rede |
| `core/xr-fusion` | calibração e orientação (filtro Mahony, desvio do giroscópio aprendido com o óculos parado, recentrar) |
| `core/xr-client` | conexões, estado, eventos, ajustes e registro (tokio) |
| `core/xr-sim` | óculos simulado para testes e para desenvolver sem o aparelho |
| `core/xr-cli` | ferramenta de linha de comando `xr` (só leitura) |
| `app/` | interface em Flutter (Linux e Android), ligada ao núcleo por flutter_rust_bridge |
| `tools/` | scripts Python de leitura e geração dos dados de referência dos testes |
| `notes/` | protocolo e decisões de projeto, com evidência e grau de certeza |

## Como compilar

Requisitos: Rust estável, Flutter 3.50+, `flutter_rust_bridge_codegen` 2.13 e, para Android, Android SDK 36 e NDK 28.

```bash
cd core && cargo test && cargo build --release -p xr-cli
```

```bash
cd app && flutter build linux --release
```

O app do Linux fica em `app/build/linux/x64/release/bundle/xr_lab`. Com `--sim`, ele abre ligado ao óculos simulado.

### Ferramenta `xr`

```bash
xr info                       # versões, sensor de rosto, estabilização, calibração
xr listen [segundos]          # teclas, ações (brilho, filtro) e temperaturas
xr imu [segundos]             # orientação ao vivo
xr record <pasta> [segundos]  # grava o fluxo bruto das 3 portas, sem enviar nada
```

Opções: `--host <ip>` e `--sim`. No Beam Pro, compile com `cargo ndk -t arm64-v8a -P 34 build --release -p xr-cli` e copie com `adb push` para `/data/local/tmp`.

## Testes

- `core/`: testes de unidade, comparação com os resultados dos scripts Python (`tools/gen_golden.py`) e testes do cliente contra o óculos simulado.
- `app/`: `flutter test` (interface com API falsa e capturas de tela) e `flutter test integration_test/app_test.dart -d linux` (ponta a ponta com o núcleo real e o simulado).

Nenhum teste depende do óculos ligado.

## Observações

- Placas NVIDIA no Linux: o app usa o renderizador por software do Flutter por causa de um erro no caminho OpenGL do Flutter 3.50 (detalhes em `notes/arquitetura-v1.md`). Para testar o OpenGL: `FLUTTER_LINUX_RENDERER=opengl`.
- Brilho e filtro não são informados ao conectar: o app passa a conhecê-los quando eles mudam pelos botões do óculos ou por um ajuste feito no app.

## Referências

- [xr-tools](https://github.com/0xcaff/xr-tools) (Martin Charles, licença MIT): o formato do canal de controle foi entendido estudando esse projeto. O código deste repositório é próprio.
- [XRLinuxDriver](https://github.com/wheaney/XRLinuxDriver) e [breezy-desktop](https://github.com/wheaney/breezy-desktop): integração de óculos XR com o desktop Linux.
