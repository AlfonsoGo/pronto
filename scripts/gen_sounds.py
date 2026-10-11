import math, struct, wave, sys
SR = 44100
def tone(path, f0, f1, ms=120, amp=0.28):
    n = int(SR * ms / 1000)
    frames = bytearray()
    phase = 0.0
    for i in range(n):
        t = i / n
        f = f0 + (f1 - f0) * t
        phase += 2 * math.pi * f / SR
        # envelope: ataque 12 ms, caida suave (coseno)
        a = min(1.0, i / (SR * 0.012))
        r = 0.5 * (1 + math.cos(math.pi * t))
        env = a * (0.35 + 0.65 * r) * (1 - t) ** 0.35
        s = (math.sin(phase) + 0.18 * math.sin(2 * phase)) / 1.18
        frames += struct.pack('<h', int(max(-1, min(1, s * env * amp)) * 32767))
    with wave.open(path, 'wb') as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR)
        w.writeframes(bytes(frames))
tone(sys.argv[1] + '/start.wav', 660, 990)   # ascendente
tone(sys.argv[1] + '/stop.wav', 880, 560)    # descendente
tone(sys.argv[1] + '/lock.wav', 820, 1240, ms=90, amp=0.26)   # bloqueo manos libres: ascenso breve
