# Canal de controle do One Pro — TCP 52999 (169.254.2.1)

Formato entendido estudando o xr-tools (0xcaff, licença MIT), 16/11/2026. O código deste repositório é próprio.

## Formato
| Parte | Tamanho | Observação |
| --- | --- | --- |
| magic | 2 bytes | tipo da mensagem |
| tamanho | u32 big-endian | inclui o id de transação |
| id de transação | u32 big-endian | pedido: 1 \| 0x80000000; resposta: 1; ausente em eventos de tecla |
| corpo | protobuf simples | leitura: `18 00`; escrita: `1a <len> 08 <valor>`; resposta: `22 <len> <tag> <valor>` |

## Mensagens
| Magic | Nome | Tipo | Risco |
| --- | --- | --- | --- |
| 27 1d | versão de software | leitura (texto) | seguro |
| 27 2d | versão do DSP | leitura (texto) | seguro |
| 27 29 | id do óculos | leitura (texto) | seguro, mas é dado pessoal do aparelho |
| 27 1f | configuração/calibração | leitura (texto) | seguro, não publicar |
| 27 18 | sensor de rosto ligado? | leitura (0/1) | seguro |
| 28 a6 | estabilização EIS ligada? | leitura (0/1) | seguro |
| 27 2e | evento de tecla (64 bytes: tipo u32 LE, estado u32 LE, tempo u32 LE) | evento | só escutar |
| 27 1c | brilho 0–9 | ajuste | com aprovação |
| 27 27 | filtro eletrocrômico 0/1/2 | ajuste | com aprovação |
| 28 29 | modo de cena (0 botões ativos, 1 botões desativados) | ajuste | com aprovação |
| 28 22 | modo de entrada (0 normal, 1 lado a lado) | ajuste | com aprovação; tela pode ficar ~12 s apagada |
| ?? | definir EDID / configuração de tela | ajuste | NÃO usar |

Tipos de tecla (confirmado em 16/11): 1 botão inferior = menu, 2 rocker frontal = volume +, 3 rocker traseiro = volume − (confirmado por uma sequência de teste), 4 botão superior = atalho. Estado: 1 apertado, 2 solto. Ao contrário do HID, aqui as teclas aparecem também com o menu aberto.

## Eventos não pedidos vistos em 16/11 (interpretação provisória)
| Magic | Conteúdo | Hipótese |
| --- | --- | --- |
| 27 8a | protobuf `1a <len> [08 índice] 15 <float>`; chegam 3 juntos (índices 0, 1, 2) a cada ~10 s: 50,0 / 44,4 / 35,9 e depois 51,0 / 45,3 / 36,3 | três sensores de temperatura (°C). Hipótese (16/11, `xr listen` + `xr imu` ao vivo): o índice 2 (37,3–37,7) fica perto da temperatura da IMU (36,9–37,1 no pacote da 52998) |
| 27 12 | 64 bytes: u32 a, u32 b, u32 ?, u64 tempo em ns (relógio do óculos) e depois lixo de memória | **evento de ação** (certeza média-alta, ver seção abaixo): a = código da ação, b = parâmetro. 1 = clique (b = tecla), 2 = duplo clique (b = tecla), 6 = brilho + (b = novo nível), 7 = brilho − (b = novo nível), 12 = filtro (b = novo nível 0/1/2), 35 = ? (visto após duplo clique no menu) |
| 27 66 | 4 bytes (`18 02 27 8a`?) | desconhecido |

Observação: o tamanho no cabeçalho cobre tudo depois dele; só respostas a pedidos começam com id de transação.

## Estado do aparelho (16/11)
- Software 1.9.1.20260914175625; DSP 15.A.01.097_20250430; sensor de rosto ligado; EIS desligado.
- Calibração salva em notes/config-onepro.json (privada). `display.target_q_left_display` gira o IMU ~35° em torno de x; aplicando a transposta, o referencial vira x direita, y baixo, z frente (conferido: olhar o monitor dá arfagem ≈ 1°). Telas separadas ~63 mm.

Ferramenta: `tools/xr_control_peek.py` (só leitura).

## Convivência com o Nebula (16/11)
Com o óculos ligado no Beam Pro e o Nebula rodando, `nc` pelo adb leu a 52998 (IMU) e a 52999 (teclas e gestos) ao mesmo tempo que o Nebula: o óculos aceita mais de uma conexão por porta. Um app próprio no Beam pode conviver com o Nebula.

## Enquadramento comum (16/11, verificado nas capturas) — certeza alta
As portas 52996 (vsync, magic 27 31, corpo 32 B) e 52998 (IMU, magic 28 36, corpo 128 B) usam o mesmo `magic | tamanho u32 BE | corpo` desta porta. Evidência e posições dos campos: notes/arquitetura-v1.md. O tempo do vsync é em ns (mediana 8 337 750 ns entre quadros).

## Brilho e filtro pelos botões — escuta passiva de 16/11/2026
Captura: `captures/brilho-filtro.jsonl` (`tools/xr_control_peek.py roteiro`), óculos ligado só no PC, firmware 1.9.1.20260914175625 / DSP 15.A.01.097_20250430. Nenhum ajuste enviado; só as leituras de versão.

Nenhuma mensagem nova apareceu (só 27 2e, 27 12 e 27 8a). O nível de brilho vem no **27 12**:

| Ação no óculos | 27 12 observado |
| --- | --- |
| 1º toque no rocker (abre o menu rápido, não muda nada) | a=1 (clique), b=tecla |
| brilho começando em 9, 10 toques em volume − | a=7, b = 8, 7, 6, 5, 4, 3, 2, 1, 0 (um por toque) |
| 11º toque em volume − (já em 0) | só 27 2e, sem 27 12 |
| 9 toques em volume + | a=6, b = 1, 2, … 9 |
| menu rápido → botão menu (passa de volume para filtro) | só 27 2e |
| volume + / − no filtro, com auto-escurecimento ligado e sem olhar a tela (o filtro não mudou) | a=12, b=1 em volume +, b=0 em volume − |
| duplo clique no menu | a=2, b=1 |
| navegação dentro do menu principal | só 27 2e |

Conclusões:
- **Certeza alta:** a=6/7 trazem o novo nível de brilho (0–9). O brilho terminou em 9, igual ao início.
- **Certeza média:** a=6 é "brilho +" e a=7 é "brilho −" (o código muda com a direção, e b é o resultado). Ainda não se sabe se o óculos emite o 27 12 quando o brilho muda por comando (27 1c).
- **Certeza alta (confirmado em `captures/filtro.jsonl`, 16/11 21:37):** a=12 é o filtro e b o novo nível. Olhando para a tela: volume + → 1, 2; volume − → 1, 0; volume + → 1, 2 (o filtro terminou em 2). Nessa vez o menu rápido já abriu no filtro, sem precisar do botão menu.
- Na primeira captura (auto-escurecimento ligado, sem olhar a tela) o óculos mandou a=12 com 1 e 0 mesmo sem o filtro mudar: o nível informado pode não ser o que a lente mostra quando o auto-escurecimento está agindo.
- Ainda não há leitura do valor atual: o app só sabe o brilho depois que alguém mexe nos botões.
- O campo u32 em @8 varia por ação (a=6/7: `f051f7f8`; a=12: `0038f600`; a=1/2: zero): provável lixo ou ponteiro, ignorar.
- O tempo do 27 2e (u32 @8) sobe ~1 por segundo (contador em segundos; não bate com a hora Unix).
