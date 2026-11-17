#!/usr/bin/env python3
"""imu_peek.py — leitor SÓ DE LEITURA do fluxo de sensores do XREAL One Pro.

Layout deduzido da captura de 14/11/2026 (porta TCP 52998), código próprio:
  pacote de 134 bytes, começa com 28 36 00 00 00 80
  @14 u64  timestamp em ns (relógio do óculos)
  @30 u32  tipo: 11 = giroscópio + acelerômetro, 4 = magnetômetro
  @34 3xf32 giroscópio (rad/s)
  @46 3xf32 acelerômetro (m/s²)
  @58 3xf32 magnetômetro (provavelmente µT; -3200 = sem dado)
  @70 f32  temperatura (°C) no tipo 11

Uso:
  python3 imu_peek.py                       # mostra os valores ~10x por segundo
  python3 imu_peek.py --save captura.bin    # também grava o fluxo bruto para replay
  python3 imu_peek.py --replay captura.bin  # lê de um arquivo em vez do óculos
  python3 imu_peek.py --config ../notes/config-onepro.json
      # gira os eixos para o referencial da tela usando a calibração de fábrica
      # (display.target_q_left_display, quatérnio Hamilton xyzw). Referencial resultante,
      # conferido com capturas: x = direita, y = baixo, z = frente.
      # Mostra: arfagem (olhar cima +), guinada (virar à direita +), rolagem (orelha direita para baixo +).
"""
import argparse
import json
import math
import socket
import struct
import time

MAGIC = bytes.fromhex("283600000080")
PKT = 134


def parse(p):
    ts = struct.unpack_from("<Q", p, 14)[0]
    kind = struct.unpack_from("<I", p, 30)[0]
    gyro = struct.unpack_from("<3f", p, 34)
    accel = struct.unpack_from("<3f", p, 46)
    mag = struct.unpack_from("<3f", p, 58)
    temp = struct.unpack_from("<f", p, 70)[0]
    return ts, kind, gyro, accel, mag, temp


def load_rotation(path):
    with open(path, encoding="utf-8") as f:
        x, y, z, w = json.load(f)["display"]["target_q_left_display"]
    R = [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
         [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
         [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]]
    # vetor do IMU -> referencial da tela: R transposta
    return lambda v: tuple(sum(R[j][i] * v[j] for j in range(3)) for i in range(3))


def chunks(args):
    if args.replay:
        with open(args.replay, "rb") as f:
            while data := f.read(4096):
                yield data
        return
    with socket.create_connection((args.host, args.port), timeout=5) as s:
        while data := s.recv(4096):
            yield data


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="169.254.1.1")
    ap.add_argument("--port", type=int, default=52998)
    ap.add_argument("--save")
    ap.add_argument("--replay")
    ap.add_argument("--config")
    args = ap.parse_args()

    out = open(args.save, "wb") if args.save else None
    to_display = load_rotation(args.config) if args.config else None
    buf = b""
    count = {}
    last_print = time.monotonic()
    first_ts = None
    gyro = accel = mag = (0.0, 0.0, 0.0)
    temp = float("nan")

    try:
        for data in chunks(args):
            if out:
                out.write(data)
            buf += data
            while True:
                i = buf.find(MAGIC)
                if i < 0 or len(buf) - i < PKT:
                    buf = buf[i:] if i >= 0 else buf[-len(MAGIC):]
                    break
                p, buf = buf[i:i + PKT], buf[i + PKT:]
                ts, kind, g, a, m, t = parse(p)
                first_ts = first_ts or ts
                count[kind] = count.get(kind, 0) + 1
                if kind == 11:
                    gyro, accel, temp = g, a, t
                elif kind == 4:
                    mag = m

            now = time.monotonic()
            if (args.replay or now - last_print >= 0.1) and to_display:
                g = [math.degrees(v) for v in to_display(gyro)]
                a = to_display(accel)
                arf = math.degrees(math.atan2(a[2], -a[1]))
                rol = math.degrees(math.atan2(-a[0], -a[1]))
                print(
                    f"arfagem {arf:6.1f}° rolagem {rol:6.1f}° | "
                    f"vel. °/s: arfagem {g[0]:7.1f} guinada {g[1]:7.1f} rolagem {g[2]:7.1f} | "
                    f"{temp:5.2f} °C | pacotes {count}",
                    flush=True,
                )
                last_print = now
                if args.replay:
                    time.sleep(0.1)
            elif args.replay or now - last_print >= 0.1:
                pitch = math.degrees(math.atan2(-accel[0], math.hypot(accel[1], accel[2])))
                roll = math.degrees(math.atan2(accel[1], accel[2]))
                gdeg = [math.degrees(x) for x in gyro]
                print(
                    f"giro °/s {gdeg[0]:7.1f} {gdeg[1]:7.1f} {gdeg[2]:7.1f} | "
                    f"acel {accel[0]:6.2f} {accel[1]:6.2f} {accel[2]:6.2f} "
                    f"(|g|={math.hypot(*accel):5.2f}) | "
                    f"pitch {pitch:6.1f} roll {roll:6.1f} | "
                    f"mag {mag[0]:6.1f} {mag[1]:6.1f} {mag[2]:6.1f} | "
                    f"{temp:5.2f} °C | pacotes {count}",
                    flush=True,
                )
                last_print = now
                if args.replay:
                    time.sleep(0.1)
    except KeyboardInterrupt:
        pass
    finally:
        if out:
            out.close()


if __name__ == "__main__":
    main()
