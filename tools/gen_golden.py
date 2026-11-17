#!/usr/bin/env python3
"""gen_golden.py — gera dados sintéticos e os resultados esperados para os testes do núcleo Rust.

Os resultados esperados saem da MESMA lógica de tools/imu_peek.py e tools/xr_control_peek.py,
para garantir que o Rust reproduz o que já foi conferido com o óculos.

Saídas:
  core/fixtures/*.bin, core/fixtures/golden/*.json   sintéticos, sem dado do aparelho (podem ir para o git)
  core/fixtures/local/*.json                        derivados de captures/ (privados, ignorados pelo git);
                                                    só gerados se as capturas existirem

Floats f32 vão como bits (inteiro u32) para comparação exata e para aceitar NaN.

Uso: python3 tools/gen_golden.py
"""
import json
import math
import os
import random
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import imu_peek  # noqa: E402
import xr_control_peek as ctl  # noqa: E402

FIX = os.path.join(ROOT, "core", "fixtures")
GOLD = os.path.join(FIX, "golden")
LOCAL = os.path.join(FIX, "local")


def bits(v):
    return struct.unpack("<I", struct.pack("<f", v))[0]


def frame(magic, body):
    return magic + struct.pack(">I", len(body)) + body


def write_json(path, obj):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=1)
        f.write("\n")


def imu_packet(rng, ts, kind):
    p = bytearray(rng.randbytes(imu_peek.PKT))  # bytes não usados: lixo de propósito
    p[0:6] = imu_peek.MAGIC
    struct.pack_into("<Q", p, 14, ts)
    struct.pack_into("<I", p, 30, kind)
    if kind == 11:
        struct.pack_into("<3f", p, 34, *(rng.uniform(-3, 3) for _ in range(3)))
        struct.pack_into("<3f", p, 46, *(rng.uniform(-12, 12) for _ in range(3)))
        struct.pack_into("<3f", p, 58, -3200.0, -3200.0, -3200.0)
        struct.pack_into("<f", p, 70, rng.uniform(30, 55))
    else:
        struct.pack_into("<3f", p, 34, 0.0, 0.0, 0.0)
        struct.pack_into("<3f", p, 46, 0.0, 0.0, 0.0)
        struct.pack_into("<3f", p, 58, *(rng.uniform(-60, 60) for _ in range(3)))
        struct.pack_into("<f", p, 70, 0.0)
    return bytes(p)


def imu_golden(packets):
    out = []
    for p in packets:
        ts, kind, g, a, m, t = imu_peek.parse(p)
        out.append({"ts": ts, "kind": kind, "gyro": [bits(x) for x in g], "accel": [bits(x) for x in a],
                    "mag": [bits(x) for x in m], "temp": bits(t)})
    return out


def gen_imu(rng):
    packets, ts = [], 1_000_000_000
    for i in range(300):
        ts += 1_000_000
        packets.append(imu_packet(rng, ts, 4 if i % 5 == 2 else 11))
    with open(os.path.join(FIX, "imu-sintetico.bin"), "wb") as f:
        f.write(b"".join(packets))
    write_json(os.path.join(GOLD, "imu-sintetico.json"), imu_golden(packets))


def gen_vsync():
    frames, gold, ts, cnt = [], [], 700_000_000_000, 6000
    for i in range(240):
        ts += 8_333_333 if i != 120 else 12_000_000_000  # uma pausa de troca de modo
        cnt += 1
        frames.append(frame(b"\x27\x31", bytes(8) + struct.pack("<QQ", ts, cnt) + bytes(8)))
        gold.append({"ts": ts, "counter": cnt})
    with open(os.path.join(FIX, "vsync-sintetico.bin"), "wb") as f:
        f.write(b"".join(frames))
    write_json(os.path.join(GOLD, "vsync-sintetico.json"), gold)


def str_response(magic, text):
    s = text.encode()
    inner = b"\x0a" + bytes([len(s)]) + s
    return frame(magic, struct.pack(">I", 1) + b"\x22" + bytes([len(inner)]) + inner)


def gen_control():
    """Fluxo como o óculos mandaria: respostas a leituras misturadas com eventos."""
    msgs = [
        ("resposta", "sw_version", str_response(b"\x27\x1d", "1.9.1.SINTETICO")),
        ("evento", None, frame(b"\x27\x2e", struct.pack("<III", 1, 1, 4242) + bytes(52))),
        ("evento", None, frame(b"\x27\x2e", struct.pack("<III", 1, 2, 4300) + bytes(52))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 1, 1, 0, 5_123_456_789) + bytes(44))),
        ("resposta", "dsp_version", str_response(b"\x27\x2d", "15.SINTETICO")),
        ("evento", None, frame(b"\x27\x8a", b"\x1a\x05\x15" + struct.pack("<f", 50.0))),
        ("evento", None, frame(b"\x27\x8a", b"\x1a\x07\x08\x01\x15" + struct.pack("<f", 44.4))),
        ("evento", None, frame(b"\x27\x8a", b"\x1a\x07\x08\x02\x15" + struct.pack("<f", 35.9))),
        ("resposta", "proximity_enabled", frame(b"\x27\x18", struct.pack(">I", 1) + b"\x22\x02\x08\x01")),
        ("resposta", "eis_enabled", frame(b"\x28\xa6", struct.pack(">I", 1) + b"\x22\x00")),
        ("evento", None, frame(b"\x27\x2e", struct.pack("<III", 3, 1, 9000) + bytes(52))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 2, 4, 0, 9_999_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 35, 0, 0, 10_000_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 7, 8, 0xF8F751F0, 91_217_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 6, 9, 0xF8F751F0, 125_409_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 12, 1, 0x00F63800, 143_098_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 1, 9, 0, 1) + bytes(44))),
        ("evento", None, frame(b"\x27\x12", struct.pack("<IIIQ", 12, 2, 0x00F63800, 203_950_000_000) + bytes(44))),
        ("evento", None, frame(b"\x27\x66", bytes.fromhex("1802278a"))),
        ("evento", None, frame(b"\x27\x2e", struct.pack("<III", 7, 9, 1) + bytes(52))),
    ]
    gold = []
    for kind, name, raw in msgs:
        magic, body = raw[:2], raw[6:]
        entry = {"tipo": kind, "magic": magic.hex()}
        if kind == "resposta":
            value = ctl.parse_value(body[4:], ctl.ALLOWED[name][1])
            entry.update({"nome": name, "valor": value})
        elif magic == ctl.KEY_MAGIC:
            entry["descricao"] = ctl.describe_key(body)
        else:
            entry["descricao"] = ctl.describe_event(magic, body)
        gold.append(entry)
    with open(os.path.join(FIX, "controle-sintetico.bin"), "wb") as f:
        f.write(b"".join(m[2] for m in msgs))
    write_json(os.path.join(GOLD, "controle-sintetico.json"), gold)

    # pedidos de leitura exatamente como o xr_control_peek.get() manda
    reqs = {}
    for name, (magic, _) in ctl.ALLOWED.items():
        body = ctl.GET_BODY
        reqs[name] = (magic + struct.pack(">I", len(body) + 4) + struct.pack(">I", 1 | 0x80000000) + body).hex()
    write_json(os.path.join(GOLD, "pedidos-leitura.json"), reqs)


def gen_rotation(rng):
    """Calibração FALSA: só o quatérnio, ~35° em torno de x (parecido com o real, sem dado do aparelho)."""
    ang = math.radians(35.0)
    q = [math.sin(ang / 2), 0.0, 0.0, math.cos(ang / 2)]
    cfg = {"FSN": "FALSO", "display": {"target_q_left_display": q}}
    cfg_path = os.path.join(FIX, "config-falsa.json")
    write_json(cfg_path, cfg)
    to_display = imu_peek.load_rotation(cfg_path)
    cases = []
    for _ in range(50):
        v = [rng.uniform(-10, 10) for _ in range(3)]
        a = to_display(v)
        cases.append({"in": v, "out": list(a),
                      "arfagem": math.degrees(math.atan2(a[2], -a[1])),
                      "rolagem": math.degrees(math.atan2(-a[0], -a[1]))})
    write_json(os.path.join(GOLD, "rotacao.json"), cases)


def gen_local():
    cap = os.path.join(ROOT, "captures", "imu-movimentos.bin")
    if not os.path.exists(cap):
        print("captures/imu-movimentos.bin ausente: pulando referências locais")
        return
    os.makedirs(LOCAL, exist_ok=True)
    with open(cap, "rb") as f:
        data = f.read()
    packets = [data[i:i + imu_peek.PKT] for i in range(0, len(data) - imu_peek.PKT + 1, imu_peek.PKT)]
    assert all(p[:6] == imu_peek.MAGIC for p in packets), "captura fora do formato esperado"
    counts = {}
    for p in packets:
        k = struct.unpack_from("<I", p, 30)[0]
        counts[str(k)] = counts.get(str(k), 0) + 1
    write_json(os.path.join(LOCAL, "imu-movimentos.json"), {
        "arquivo": "captures/imu-movimentos.bin", "total": len(packets), "por_tipo": counts,
        "passo": 50, "amostras": imu_golden(packets[::50])})
    print(f"referência local: {len(packets)} pacotes de IMU")


def main():
    os.makedirs(GOLD, exist_ok=True)
    rng = random.Random(20261116)
    gen_imu(rng)
    gen_vsync()
    gen_control()
    gen_rotation(rng)
    gen_local()
    print("ok:", FIX)


if __name__ == "__main__":
    main()
