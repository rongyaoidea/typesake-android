"""Typesake promo BGM — 全部用代码合成（pydub/numba-free），不含任何外部音源。

120 BPM、I-vi-IV-V 循环 3 遍 ≈ 24s，与 16:9 视频同长。
"""
import math, random
from pydub import AudioSegment
from pydub.generators import Sine, Triangle, Square, WhiteNoise

random.seed(7)

SR = 44100
BPM = 120
BEAT = 60000 / BPM / 1000.0  # 0.5s
BAR = BEAT * 4.0
TOTAL = 24.0 * 1000  # ms

A4 = 440.0
def freq(semi_from_a4): return A4 * (2 ** (semi_from_a4 / 12.0))

def note(freqhz, dur_ms, gen, vol_db=-12):
    seg = gen(freqhz).to_audio_segment(duration=dur_ms)
    seg = seg + vol_db
    # quick fade to avoid clicks
    seg = seg.fade_in(8).fade_out(60)
    return seg

# note indices relative to A4 (semitone). A4=0, C4=-9, E4=-5, G4=-2, C5=3, E5=7, G5=10
CHORDS = [
    ("C",  ["C4","E4","G4"],  [-9, -5, -2]),
    ("Am", ["A3","C4","E4"],  [-12,-9, -5]),
    ("F",  ["F3","A3","C4"],  [-14,-12,-9]),
    ("G",  ["G3","B3","D4"],  [-17,-14,-7]),
]

def seg_for(chord, gen, dur_ms, vol_db):
    out = AudioSegment.silent(duration=int(dur_ms))
    for semi in chord[2]:
        out = out.overlay(note(freq(semi), dur_ms, gen, vol_db))
    return out

def bass_for(chord, dur_ms, vol_db):
    # root note one octave below chord root
    root_semi = chord[2][0]
    out = AudioSegment.silent(duration=int(dur_ms))
    gap = dur_ms / 2.0
    for start in (0, gap):
        n = note(freq(root_semi - 12), gap * 0.75, Sine, vol_db)
        out = out.overlay(n, position=int(start))
    return out

def kick(dur_ms=180, vol_db=-10):
    n = Sine(120).to_audio_segment(duration=dur_ms).fade_out(140)
    return n + vol_db

def hat(dur_ms=50, vol_db=-20):
    n = WhiteNoise().to_audio_segment(duration=dur_ms).fade_out(40)
    return n + vol_db

def snare(dur_ms=140, vol_db=-16):
    n = WhiteNoise().to_audio_segment(duration=dur_ms).fade_out(120)
    return n + vol_db

# melody phrase per chord (8th notes): play chord[2] tones stepping up then down
def melody_for(chord, bar_ms, vol_db=-11):
    tones = [chord[2][0] + 12, chord[2][1] + 12, chord[2][2] + 12, chord[2][2] + 12 + 3]
    out = AudioSegment.silent(duration=int(bar_ms))
    eighth = bar_ms / 8.0
    for i, semi in enumerate(tones):
        n = note(freq(semi), eighth * 0.85, Square, vol_db)
        out = out.overlay(n, position=int(i * eighth))
        if i + 4 < 8:
            out = out.overlay(note(freq(semi), eighth * 0.85, Square, vol_db - 2), position=int((i + 4) * eighth))
    return out

# build 4-bar loop, then repeat 3x
bar_ms = BAR * 1000
loop = AudioSegment.silent(duration=int(BAR * 4 * 1000))
for i, chord in enumerate(CHORDS):
    bar = AudioSegment.silent(duration=int(bar_ms))
    bar = bar.overlay(seg_for(chord, Triangle, bar_ms, -16))
    bar = bar.overlay(bass_for(chord, bar_ms, -13))
    bar = bar.overlay(melody_for(chord, bar_ms, -11))
    # drums: kick on beats 1 & 3, snare on 2 & 4, hats every 8th
    for b in range(4):
        if b in (0, 2):
            bar = bar.overlay(kick(), position=int(b * BEAT * 1000))
        else:
            bar = bar.overlay(snare(), position=int(b * BEAT * 1000))
        bar = bar.overlay(hat(), position=int(b * BEAT * 1000))
        bar = bar.overlay(hat(), position=int((b + 0.5) * BEAT * 1000))
    loop = loop.overlay(bar, position=int(i * bar_ms))

bgm = loop + loop + loop
# soft fade
bgm = bgm.fade_in(1200).fade_out(2200)
bgm = bgm.apply_gain(-3)
bgm.export('/tmp/opencode/promo/bgm.wav', format='wav')
print('bgm length ms:', len(bgm))
