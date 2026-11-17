#!/usr/bin/env python3
"""xr_control_peek.py — SÓ LEITURA do canal de controle do XREAL One Pro (TCP 52999).

Código próprio, para uso pessoal. O formato foi entendido estudando o xr-tools (0xcaff).
NÃO envia nenhum comando de ajuste: só os pedidos de leitura da lista ALLOWED abaixo.

Formato (big-endian no cabeçalho):
  magic (2 bytes) | tamanho u32 BE | [id de transação u32 BE] | corpo protobuf
  - pedido: id = 1 | 0x80000000, corpo de leitura = 18 00 (campo 3, varint 0)
  - resposta: corpo = 22 <len> <tag do valor> <valor>
  - evento de tecla (magic 27 2e): sem id de transação, corpo de 64 bytes
      u32 LE tipo (1 inferior, 2 rocker frontal, 3 rocker traseiro, 4 superior),
      u32 LE estado (1 apertado, 2 solto), u32 LE tempo do óculos

Uso:
  python3 xr_control_peek.py listen            # só escuta eventos (teclas) por 60 s
  python3 xr_control_peek.py info              # versões, sensor de rosto e EIS
  python3 xr_control_peek.py config out.json   # salva a configuração/calibração de fábrica
  python3 xr_control_peek.py roteiro saida.jsonl
      # lê as versões e depois só escuta, guiando passo a passo (Enter avança).
      # Grava cada mensagem bruta com a hora e o passo em que chegou.
"""
import json
import os
import socket
import struct
import sys
import threading
import time

HOST = os.environ.get("XR_HOST", "169.254.2.1")
PORT = int(os.environ.get("XR_PORT", "52999"))

# Somente leituras. Nada de "set" aqui, de propósito.
ALLOWED = {
    "sw_version": (b"\x27\x1d", "str"),
    "dsp_version": (b"\x27\x2d", "str"),
    "glasses_id": (b"\x27\x29", "str"),  # identificador do aparelho: não compartilhe
    "config": (b"\x27\x1f", "str"),
    "proximity_enabled": (b"\x27\x18", "bool"),
    "eis_enabled": (b"\x28\xa6", "bool"),
}
KEY_MAGIC = b"\x27\x2e"
KEY_TYPES = {1: "botão inferior (menu)", 2: "rocker frontal (volume +)", 3: "rocker traseiro (volume −)", 4: "botão superior (atalho)"}
KEY_STATES = {1: "apertado", 2: "solto"}
GET_BODY = b"\x18\x00"


def recv_exact(s, n):
    buf = b""
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("conexão fechada pelo óculos")
        buf += chunk
    return buf


def varint(b, i):
    v = shift = 0
    while True:
        c = b[i]
        i += 1
        v |= (c & 0x7F) << shift
        shift += 7
        if not c & 0x80:
            return v, i


def read_message(s):
    """Devolve (magic, bytes brutos). O tamanho cobre tudo depois do cabeçalho."""
    magic, length = struct.unpack(">2sI", recv_exact(s, 6))
    return magic, recv_exact(s, length)


def describe_event(magic, raw):
    """Eventos não pedidos vistos em 16/11/2026 (interpretação provisória)."""
    if magic == b"\x27\x8a" and raw[:1] == b"\x1a":
        # protobuf: campo 3 { campo 1 = índice (omitido se 0), campo 2 = float }
        _, i = varint(raw, 1)
        idx = 0
        if raw[i] == 0x08:
            idx, i = varint(raw, i + 1)
        if raw[i] == 0x15:
            val = struct.unpack_from("<f", raw, i + 1)[0]
            return f"sensor {idx} = {val:.1f} (provável temperatura, °C)"
    if magic == b"\x27\x12" and len(raw) >= 20:
        # evento de ação (16/11/2026): a = código, b = parâmetro. Ver notes/protocolo-controle-52999.md
        a, b = struct.unpack_from("<II", raw, 0)
        ts = struct.unpack_from("<Q", raw, 12)[0]
        if a in (1, 2):
            what = f"{'clique' if a == 1 else 'duplo clique'} em {KEY_TYPES.get(b, b)}"
        elif a in (6, 7):
            what = f"brilho {'+' if a == 6 else '−'} → nível {b}"
        elif a == 12:
            what = f"filtro → nível {b}"
        else:
            what = f"código {a}, parâmetro {b}"
        return f"ação: {what} (t={ts / 1e9:.3f}s)"
    return f"magic={magic.hex()} bruto={raw[:48].hex()}"


def describe_key(body):
    ktype, kstate, t = struct.unpack_from("<III", body, 0)
    return f"tecla: {KEY_TYPES.get(ktype, ktype)} {KEY_STATES.get(kstate, kstate)} (t={t})"


def parse_value(body, kind):
    if kind == "bool" and body == b"\x22\x00":
        return False
    if body[0] != 0x22:
        raise ValueError(f"resposta inesperada: {body.hex()}")
    _, i = varint(body, 1)
    tag = body[i]
    i += 1
    if kind == "str":
        n, i = varint(body, i)
        return body[i:i + n].decode("utf-8", "replace")
    v, i = varint(body, i)
    return bool(v) if kind == "bool" else v


def get(s, name):
    magic, kind = ALLOWED[name]
    body = GET_BODY
    s.sendall(magic + struct.pack(">I", len(body) + 4) + struct.pack(">I", 1 | 0x80000000) + body)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        m, raw = read_message(s)
        if m == KEY_MAGIC:
            print("  (evento)", describe_key(raw))
            continue
        if m == magic:
            return parse_value(raw[4:], kind)
        print("  (evento)", describe_event(m, raw))
    raise TimeoutError(f"sem resposta para {name}")


ROTEIRO = [
    "Base: deixe o óculos parado, sem tocar em nada, por uns 15 s.",
    "Anote o brilho atual. Pelo menu do óculos, AUMENTE o brilho até o máximo, um passo por segundo.",
    "Agora DIMINUA o brilho até o mínimo, um passo por segundo.",
    "Volte o brilho ao nível que você anotou.",
    "Anote o filtro (escurecimento) atual. Passe por cada posição do filtro, uns 3 s em cada.",
    "Volte o filtro à posição que você anotou.",
    "Feche o menu e deixe o óculos parado por uns 15 s.",
]


def roteiro(s, out_path):
    """Escuta guiada: nenhum ajuste é enviado; só os pedidos de leitura de versão."""
    versions = {n: get(s, n) for n in ("sw_version", "dsp_version")}
    print(f"firmware: {versions['sw_version']} / DSP {versions['dsp_version']}")
    s.settimeout(None)  # leitura bloqueante; no fim o socket é fechado para soltar a thread
    step = [0]
    stop = threading.Event()
    t0 = time.monotonic()
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(json.dumps({"tipo": "inicio", "hora": time.strftime("%Y-%m-%dT%H:%M:%S"), **versions}) + "\n")
        lock = threading.Lock()

        def reader():
            while not stop.is_set():
                try:
                    m, raw = read_message(s)
                except (ConnectionError, OSError) as e:
                    if not stop.is_set():
                        print("conexão perdida:", e)
                        stop.set()
                    return
                dt = time.monotonic() - t0
                desc = describe_key(raw) if m == KEY_MAGIC else describe_event(m, raw)
                with lock:
                    f.write(json.dumps({"t": round(dt, 4), "passo": step[0], "magic": m.hex(), "bruto": raw.hex()}) + "\n")
                    f.flush()
                if m != b"\x27\x8a":
                    print(f"  [{dt:7.2f}s] {desc}")

        th = threading.Thread(target=reader, daemon=True)
        th.start()
        for i, texto in enumerate(ROTEIRO, 1):
            step[0] = i
            with lock:
                f.write(json.dumps({"t": round(time.monotonic() - t0, 4), "passo": i, "instrucao": texto}) + "\n")
            print(f"\nPasso {i}/{len(ROTEIRO)}: {texto}")
            input("  (Enter quando terminar) ")
            if stop.is_set():
                break
        stop.set()
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        th.join(timeout=2)
    print(f"\ngravado em {out_path}")


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "info"
    with socket.create_connection((HOST, PORT), timeout=10) as s:
        if cmd == "roteiro":
            out = sys.argv[2] if len(sys.argv) > 2 else "captures/brilho-filtro.jsonl"
            roteiro(s, out)
            return
        if cmd == "listen":
            end = time.monotonic() + 60
            print("Escutando 60 s, sem enviar nada. Aperte os botões do óculos.")
            while time.monotonic() < end:
                try:
                    m, raw = read_message(s)
                except socket.timeout:
                    continue
                stamp = time.strftime("%H:%M:%S")
                print(stamp, describe_key(raw) if m == KEY_MAGIC else describe_event(m, raw))
        elif cmd == "info":
            for name in ("sw_version", "dsp_version", "proximity_enabled", "eis_enabled"):
                print(f"{name}: {get(s, name)}")
        elif cmd == "config":
            out = sys.argv[2] if len(sys.argv) > 2 else "config.json"
            data = get(s, "config")
            with open(out, "w", encoding="utf-8") as f:
                f.write(data)
            print(f"configuração salva em {out} ({len(data)} caracteres). Contém dados de calibração do seu aparelho: não publique.")
        else:
            print(__doc__)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\ninterrompido.")
