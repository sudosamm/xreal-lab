# Arquitetura do app nativo — versão 1

Definida em 16/11/2026. Linux (CachyOS) e Beam Pro primeiro; Windows depois.

## Decisões
- Núcleo em Rust (`core/`), interface em Flutter (`app/`), ligados por flutter_rust_bridge 2.x.
- Linux na v1: o núcleo roda dentro do app. Serviço systemd + D-Bus + bandeja (`core/xr-daemon`) fica para a v1.1.
- Beam Pro: mesmo núcleo dentro de um serviço em primeiro plano (`connectedDevice`). Se o socket não alcançar 169.254.2.1 sozinho, o Kotlin acha a rede do óculos (ConnectivityManager) e o Rust liga o socket com `android_setsocknetwork`.
- Brilho e filtro: antes de liberar os ajustes, investigação passiva da 52999 mudando brilho e filtro pelo menu do óculos (ferramenta: `tools/xr_control_peek.py roteiro`).

## Camadas
| Crate | Papel | Origem da lógica |
| --- | --- | --- |
| `core/xr-proto` | sem rede: quadro (magic + u32 BE), varint/protobuf mínimo, IMU 134 B, vsync 27 31, controle, eventos | `tools/imu_peek.py`, `tools/xr_control_peek.py` |
| `core/xr-fusion` | calibração (só campos necessários, descarta número de série), rotação IMU→tela, filtro Mahony + recentrar | `load_rotation` do imu_peek |
| `core/xr-client` | tokio: 3 conexões, religar, estado, gravar/reproduzir, trava de ajustes, registro de ajustes | — |
| `core/xr-sim` | servidor simulado das 3 portas para testes | — |
| `core/xr-cli` | `xr info / listen / imu / record` (só leitura) | substitui os peeks |
| `app/rust` | ponte flutter_rust_bridge | — |

## Trava de ajustes
- `Adjustment` é um enum fechado: brilho 0–9 (27 1c), filtro 0/1/2 (27 27), botões ativos/desativados (28 29), modo normal/lado a lado (28 22). Não existe API para bytes livres.
- Duas etapas: `prepare` (descrição, valor atual ou "desconhecido", como desfazer, avisos; vale 30 s, uso único) e `confirm` disparado pelo diálogo.
- "Modo ajustes" desligado a cada abertura; ajustes só para firmware da lista de testados (1.9.1.20260914175625).
- Cada envio vai para um registro JSONL (hora, firmware, comando, valor anterior, desfazer).
- Um pedido por vez, id de transação fixo `1 | 0x80000000` (outros ids não foram testados).

## Enquadramento comum às três portas (verificado nas capturas em 16/11) — certeza alta
Toda mensagem é `magic (2) | tamanho u32 BE | corpo`. Um único decodificador serve às portas 52996, 52998 e 52999.
- IMU (52998): magic `28 36`, tamanho 128 (os "6 bytes mágicos" `28 36 00 00 00 80` do imu_peek são magic + tamanho). `captures/imu-movimentos.bin`: 80 695 mensagens seguidas, sem lixo (57 639 do tipo 11, 23 056 do tipo 4).
- Vsync (52996): magic `27 31`, tamanho 32. Corpo: 8 bytes zero, u64 LE @8 = tempo em ns (mediana entre quadros 8 337 750 ns ≈ 120 Hz), u64 LE @16 = contador (+1 a cada mensagem, inclusive através da pausa de troca de modo), 8 bytes zero. `captures/52996-acoes.bin`: 6 070 mensagens em 77,3 s; a diferença para 6 070 × 8,34 ms ≈ 50,6 s é a pausa.

## Onde fica a trava
A trava de ajustes (`gate`) fica em `xr-proto`, não em `xr-client`: assim a montagem dos bytes de ajuste pode ser privada do crate e só sai pela confirmação. `xr-client` só transporta o que a trava liberou.

## Testes
Referências geradas pela lógica Python (`tools/gen_golden.py`), dados sintéticos em `core/fixtures/`, capturas de `captures/` usadas só se existirem, xr-sim para a trava e o cliente. Nada depende do óculos ao vivo.

## Ordem
1. xr-proto + referências · 2. xr-fusion · 3. xr-sim + xr-client + trava · 4. `xr` no Linux e no Beam (adb) · 5. app Flutter no Linux · 6. APK no Beam.

## Andamento (16/11/2026, noite)
| Etapa | Estado |
| --- | --- |
| 1. xr-proto | pronto: quadros, varint, IMU, vsync, leituras, eventos (27 12 como "ação": níveis de brilho e filtro), trava com `observe()` para o brilho visto nos botões. 22 testes + 5 de referência contra o Python (inclui a captura local). |
| 2. xr-fusion | pronto: calibração (só os campos usados), rotação igual ao Python, Mahony + bias aprendido parado + recentrar. 8 testes + 2 (na captura local: mediana 0,68° entre filtro e acelerômetro com o óculos parado). |
| 3. xr-sim + xr-client | pronto: 3 tarefas tokio que religam sozinhas, troca de endereço, estado, eventos, orientação a 60 Hz, ajustes só por prepare/confirm, registro JSONL. 5 testes contra o simulado (20 de 20 rodadas estáveis). |
| 4. `xr` (linha de comando) | pronto no PC: `xr info / listen / imu / record`, opções `--host` e `--sim`, sem nenhum comando de ajuste. 5 testes com o simulado embutido. Ao vivo no PC (16/11, firmware 1.9.1.20260914175625): `xr info` conectou em 169.254.2.1 e leu tudo; `xr imu` deu IMU 998–1000/s, vsync 120/s, orientação estável parada; `xr listen` recebeu as temperaturas. No Beam (16/11, 22:3x, via `adb push` para /data/local/tmp, binário arm64 do NDK r28c): `xr info`, `xr imu` e `xr listen` funcionaram com o Nebula rodando, e o Nebula continuou rodando. |
| 5. Flutter no Linux | pronto (16/11): `app/` (Flutter 3.50, flutter_rust_bridge 2.13, ponte `app/rust` = crate `xr_bridge`). Telas Painel, Orientação, Botões, Ajustes; diálogo de confirmação (Confirmar só arma após 750 ms; fecha sozinho quando a proposta vence); fonte "Óculos (USB)" ou "Simulado" no menu do topo; registro em `~/.local/share/dev.xreallab.xr_lab/ajustes.jsonl`. 11 testes de widget com API falsa + 4 capturas (`app/test/goldens/`) + 1 teste de ponta a ponta com o núcleo real e o simulado (`flutter test integration_test/app_test.dart -d linux`). Versão final: `app/build/linux/x64/release/bundle/xr_lab` (`--sim` para o simulado). Ainda não aberto com o óculos de verdade. |
| 6. APK no Beam | a fazer |

### Janela do Linux deslocada com NVIDIA (16/11/2026) — certeza alta na causa
Sintoma: o conteúdo aparecia com o centro no canto inferior esquerdo da janela (mostrando só o quarto superior direito do layout), em Wayland e em X11. Causa, lida no código do Flutter 3.50 (`engine/src/flutter/shell/platform/linux`): com fornecedor de GL "NVIDIA", `fl_opengl_manager.cc` desliga o `glBlitFramebuffer` (issue flutter#152099) e o desenho passa pelo shader em `fl_compositor_opengl.cc::composite_layer`, que usa `offset = 2x/w − 1` e `scale = tw/w` sobre vértices em [−1, 1] — o quad vai para [−2, 0]. O Impeller não era a causa. Contorno no app: `app/linux/runner/main.cc` define `FLUTTER_LINUX_RENDERER=software` (o usuário pode trocar para `opengl`) e `my_application.cc` desliga o Impeller nesse caso — com os dois juntos o motor aborta (`Impeller DlText cannot be drawn to a Skia canvas`). Confirmado em 16/11 (janela inteira, modo simulado) e pelo teste de ponta a ponta. Candidato a relatar ao Flutter.

## Bias do giroscópio — óculos parado na mesa, 16/11/2026 21:34 (certeza alta)
Captura: `captures/imu-parado.bin` (140,7 s, de cabeça para baixo, 32,3 → 39,4 °C; duas batidas leves aos 38 s e aos 136 s).
- O giroscópio parado tem um desvio estável da ordem de 0,005 rad/s (dois eixos) e quase zero no terceiro; quase não varia no tempo nem com a temperatura entre 32 e 39 °C.
- O ruído parado (~0,001 rad/s por eixo) é igual ao `imu_intrinsics.gyro_std` da fábrica: as unidades e os eixos batem.
- O bias de fábrica (`gyro_bias_temp_data`) não bate com o medido (um dos eixos tem até o sinal trocado): subtraí-lo deixaria o erro maior do que não corrigir nada. Nenhuma troca de eixos ou de sinal explica a diferença. Hipóteses: o firmware já desconta algum bias, ou o bias mudou desde a calibração.
- Decisão: `FilterConfig::use_factory_bias = false`. O filtro aprende o bias sempre que o óculos fica parado. Nesta captura, o bias aprendido ficou a menos de 2e-4 rad/s da média, e a guinada derivou 0,04° em 131 s (sem correção seriam ~28°).
- Ideia para depois: guardar o último bias aprendido no app e usá-lo ao abrir.

## Beam Pro: rede do óculos e vsync (16/11/2026) — certeza alta para o shell
- O Beam cria `eth0` 169.254.2.10/24 (NCM, óculos em .2.1) e `eth1` 169.254.1.10/24 (ECM, óculos em .1.1). Para o usuário shell (uid 2000), `ip route get 169.254.2.1` sai por eth0 sem nenhuma ligação especial.
- Com o Nebula conectado, a IMU chega a ~1000/s e as temperaturas chegam normalmente numa segunda conexão.
- **O vsync no Beam é 60/s** (no PC é 120/s): o Beam provavelmente usa a tela a 60 Hz.
- Ainda em aberto: um APK roda com outro uid, e o Android pode não deixar um app comum usar a rede eth0 (que não tem internet) sem `ConnectivityManager` + `bindSocket`/`android_setsocknetwork`. Testar no primeiro APK; o plano B já está na arquitetura.

## Pendências
- Ainda não sabemos se o óculos emite 27 12 quando o brilho ou o filtro mudam por comando (27 1c / 27 27). Isso só aparece no primeiro ajuste real, que precisa de aprovação explícita.
