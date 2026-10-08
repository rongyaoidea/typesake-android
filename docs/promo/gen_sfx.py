"""Typesake promo SFX — 全部用代码合成（pydub/numpy），与画面事件对齐。

事件时间取自 typesake_promo.html：CLICKS / TYPE_SEQ / CORRECT_T / floods / engine。
"""
import numpy as np
from pydub import AudioSegment

SR = 44100
TOTAL_MS = 24 * 1000


def ms_samples(n):
    return int(SR * n / 1000.0)


def to_seg(arr):
    """float32 [-1,1] -> 16-bit mono AudioSegment"""
    arr = np.clip(arr, -1.0, 1.0)
    pcm = (arr * 32767).astype(np.int16)
    return AudioSegment(
        pcm.tobytes(), frame_rate=SR, sample_width=2, channels=1
    )


def place(base, seg, t_sec, gain_db=0.0):
    return base.overlay(seg + gain_db, position=int(t_sec * 1000))


def tick(f=2100, dur_ms=26, decay=900):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    env = np.exp(-t * decay)
    body = np.sin(2 * np.pi * f * t) * env
    click = np.random.RandomState(1).randn(n) * np.exp(-t * 2600) * 0.55
    return to_seg(body * 0.8 + click)


def typek(f=1550, dur_ms=34):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    env = np.exp(-t * 620)
    body = np.sin(2 * np.pi * f * t + 1.6 * np.sin(2 * np.pi * f * 0.5 * t)) * env
    click = np.random.RandomState(2).randn(n) * np.exp(-t * 1900) * 0.4
    return to_seg(body * 0.75 + click)


def pop(dur_ms=90, f0=520, f1=980):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    k = t / (dur_ms / 1000.0)
    f = f0 + (f1 - f0) * k
    phase = 2 * np.pi * np.cumsum(f) / SR
    env = np.sin(np.pi * np.clip(k, 0, 1)) ** 1.4
    return to_seg(np.sin(phase) * env * 0.9)


def ding(dur_ms=560):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    e1, e2 = np.exp(-t * 5.5), np.exp(-t * 8.0)
    a = np.sin(2 * np.pi * 1318.5 * t) * e1
    b = np.sin(2 * np.pi * 1975.5 * t) * e2 * 0.55
    c = np.sin(2 * np.pi * 2637.0 * t) * np.exp(-t * 12) * 0.3
    return to_seg((a + b + c) * 0.7)


def thud(dur_ms=340, f0=170, f1=52):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    k = t / (dur_ms / 1000.0)
    f = f0 * (f1 / f0) ** k
    phase = 2 * np.pi * np.cumsum(f) / SR
    env = np.exp(-t * 7.5)
    sub = np.random.RandomState(3).randn(n) * np.exp(-t * 40) * 0.35
    return to_seg(np.sin(phase) * env + sub * env)


def onepole_lp(x, cutoff_curve):
    """one-pole lowpass, cutoff (Hz) per sample"""
    y = np.empty_like(x)
    alpha = 1.0 - np.exp(-2 * np.pi * cutoff_curve / SR)
    acc = 0.0
    for i in range(len(x)):
        acc += alpha[i] * (x[i] - acc)
        y[i] = acc
    return y


def whoosh(dur_ms=620, up=True):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    k = t / (dur_ms / 1000.0)
    noise = np.random.RandomState(4).randn(n)
    cut = 300 + (5200 if up else -1800) * (k if up else (1 - k))
    cut = np.clip(cut, 220, 6000)
    shaped = onepole_lp(noise, cut)
    env = np.sin(np.pi * k) ** 1.6
    return to_seg(shaped * env * 2.2)


def riser(dur_ms=900):
    n = ms_samples(dur_ms)
    t = np.arange(n) / SR
    k = t / (dur_ms / 1000.0)
    f = 220 * (2 ** (2.2 * k))
    phase = 2 * np.pi * np.cumsum(f) / SR
    env = k ** 2.2
    noise = np.random.RandomState(5).randn(n) * (k ** 3) * 0.4
    return to_seg((np.sin(phase) * 0.7 + noise) * env * 0.8)


# ---------------------------------------------------------------
base = AudioSegment.silent(duration=TOTAL_MS)

# UI clicks — CLICKS[] from the film (flood/scene triggers double as hits)
for tc in [3.00, 10.60, 11.10, 11.60, 12.10, 13.60, 14.50, 16.75, 18.00, 18.20]:
    base = place(base, tick(), tc, -8)
base = place(base, tick(1700, 30), 15.00, -6)      # coral flood hit
base = place(base, tick(1300, 34), 21.00, -5)      # ink flood hit

# typing
for tt in [4.00, 4.35, 4.70, 5.05, 5.40]:
    base = place(base, typek(), tt, -7)
for tt in [8.50, 8.62, 8.74, 8.86, 8.98]:
    base = place(base, typek(1750), tt, -7)
# backspace
for td in [7.85, 7.98, 8.11, 8.24, 8.37]:
    base = place(base, typek(980, 28), td, -9)

# correction stamp 纠
base = place(base, tick(2600, 22), 9.15, -6)
base = place(base, ding(), 9.15, -10)

# candidates pop / scheme-chip slide feedback
base = place(base, pop(), 5.55, -10)
base = place(base, pop(80, 640, 860), 14.30, -12)   # vertical candidates slide

# camera zooms (subtle)
base = place(base, whoosh(700, up=True), 6.40, -16)
base = place(base, whoosh(700, up=False), 7.45, -16)

# floods
base = place(base, whoosh(640, up=True), 15.00, -11)
base = place(base, whoosh(640, up=True), 21.00, -11)
base = place(base, whoosh(620, up=False), 23.16, -12)

# sentence card + syntax chip
base = place(base, ding(420), 18.30, -14)
base = place(base, pop(70, 700, 900), 19.30, -13)
base = place(base, pop(70, 780, 1040), 19.75, -13)

# engine entrance
base = place(base, riser(820), 20.70, -13)
base = place(base, thud(), 21.42, -9)
base = place(base, tick(2400, 24), 22.16, -7)       # APK pill

# heartbeat dot at the loop seam
base = place(base, pop(110, 420, 620), 23.60, -14)

base = base.fade_in(40).fade_out(300)
base.export('/tmp/opencode/promo/sfx.wav', format='wav')
print('sfx length ms:', len(base))
