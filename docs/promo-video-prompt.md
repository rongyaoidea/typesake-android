# Typesake 产品宣传视频提示词（Opus 5.5 直接可用）

> 基于 awesome-opus-5.5-video 产品广告案例模板（@twoclipping 苹果发布会风格 + @moritzkremb
> 一句提示词 SaaS 片）改写。模型本身不出视频，它能产出 HTML/Canvas 动画 + 用 Playwright 和
> ffmpeg 渲染成片——这正符合「用 Claude Opus 5.5 + 代码做广告视频」的工作流。

把下面整段粘给 Claude（建议 Opus 5.5），它会先问 `<inputs>` 里列的素材，再给节奏图和 4 张静帧，
确认后才写完整影片。

```text
<inputs>
Ask me for: 1) the brand name shown as the wordmark ("Typesake", one word),
2) 6–9 high-res screenshots of the Android input method in use (typing Chinese pinyin,
   the glass keyboard, the 译词 line under candidates, the study center, the 9-scheme
   settings, the heatmap), 3) a royalty-free song around 120 BPM with a drop and a
   quiet breakdown (Mixkit), 4) one free stock clip of a plain desk with moving light
   shadows (Pexels).
</inputs>

<direction>
A single continuous take, 2D only, nothing fades/blurs/cuts. Objects change shape
instead: text rises out of a mask line, the coral dot grows into the Typesake
wordmark, a keyboard unfolds from a pill, pages push, a coral shape floods the frame
and contracts into the next scene. Warm cream canvas (#FAF9F5), black UI, the app's
glass keyboard (coral #D97757 + cream, 1dp rim light) over the screenshots. Archivo
(wdth 125, weight 800) for the Typesake wordmark, Geist for UI, 思源黑体 for Chinese
glyphs. A cursor drives every change with real clicks, swipes, and a long-press.
The camera zooms screen-studio style so each moment fills the square, and the
cursor scales with it.
Banned: crossfades, blur-ins, brightness "developing", 3D flips, particles, glows,
holds longer than 1s, anything that looks like a template.
</direction>

<structure>
120 BPM, roughly 60 beats, something happens on every beat.
Open: on the cream canvas, a coral dot pulses once, grows into a black pill, the
Typesake wordmark rises inside it letter by letter. Click: the pill morphs into the
glossy phone keyboard (a screenshot), and a cursor starts typing pinyin.
Type: as each letter appears, Chinese candidates rise under the composing text,
and under them the English-gloss line fills in — "今天 · today  ● fresh". The
camera zooms into the candidate bar so the 译词 (gloss) line fills the frame.
Correct: backspace on a wrong pinyin, retype with a typo (zhnag), and the engine
auto-corrects — the candidate shows "输入→纠正" with an orange 纠 mark.
Schemes: a chip bar snaps across the top with 9 input schemes
(全拼/小鹤/自然码/微软/搜狗/智能ABC/小浪/首道/大千注音), the keyboard switches
morphing between them, ending on 大千注音 with a 注音 keyboard.
Modes: long-press a candidate shows 置顶/删词/译词； a T9 flip collapses the
keyboard into a phone keypad； the candidate bar rotates into a vertical column
with a 更多 entry; the glass blur toggle switches the palette to dark.
Shortcuts: the cursor types `v1+2*3` → the candidate bar pops "7"; `i20265` →
"二〇二六年"; `im123.45` → "壹佰贰拾叁元肆角伍分"; `u4e0a` → "上". Each result is
stamped with its category tag (算式/中文数字/金额/字符).
Study: a double-space tap on the candidate bar收藏 the sentence, the scene pushes
into the learning center: a card flips into its saved English, a 语法讲解 line
expands, a CEFR level chip (A2) slides in, and a 12-week heatmap draws itself.
Engine: a dark panel rolls up: "Rust 离线引擎 · 165k 词 FST · 9 套方案 · 
全离线、零网络权限" — a small rust-ferris-like crab icon blinks.
Wall: the panel contracts into a coral dot, the dot becomes the period of the
wordmark, the letters spring back out, landing on the beat return. Last frame =
first frame.
</structure>

<build>
1. One HTML file, square 1440x1440. Every style is computed from time inside an
   async seek(t): no CSS transitions, no timers, no state between frames.
2. Springs are closed-form step responses; a value with many targets is the sum of
   one spring per change.
3. Liquid glass: each glass element holds its own clone of the scene behind it,
   filtered through an SVG feDisplacementMap distance field for chromatic edges.
4. Footage for backgrounds: re-encode all-intra (ffmpeg -g 1), blob URL, await
   `seeked` before drawing each frame.
5. SFX from Mixkit per event; the song starts on a downbeat; loudnorm to -14 LUFS.
6. Render with Playwright: 4 subframes per frame blended with ffmpeg tmix, 60fps;
   check one frame per beat, then scan for single-frame pops.
</build>

<gotchas>
backdrop-filter: url() misreads displacement maps in Chromium — clone the scene.
A flood must overscale past the corners and take ~0.3s. A child with
visibility: visible shows through a hidden parent — use inherit. Text that swaps
inside a morphing shape needs its own mask. python http.server can't range-seek
video — use the blob URL.
</gotchas>

<start>
Ask me for the inputs first, then show me the beat map and 4 stills
(open, typing+gloss, schemes switch, study center heatmap) before you write the
full film.
</start>
```

## 怎么用

1. 把上面整段（<inputs> 到 </start>）粘给 Claude（Opus 5.5 / Sonnet 5.5）。
2. 它会要模型名字、9 张以内的 Typesake 截屏、一首 120 BPM 的免版税歌、一段光影素材。
   截屏建议就是以下真实场景：打拼音上候选行+英文译词、草写纠错高亮、9 方案设置页、
   T9 九键、竖排候选+更多、学习中心 12 周热力图。
3. 模型先给节奏图（beat map）+ 4 张静帧，确认后再写完整 HTML，最后用
   Playwright + ffmpeg 本地渲染成 mp4。

## 参考来源（awesome-opus-5.5-video）

- 苹果发布会风格产品视频（HTML/Canvas + ffmpeg+Playwright 工作流）：
  https://x.com/twoclipping/status/2103835273813496100
- 一句提示词生成的 SaaS 发布视频（一句话 520 字版本）：
  https://x.com/moritzkremb/status/2103066071838466494

产物说明：这个文件只是生产级提示词，**本机没有模型/素材就不会产出 mp4**。
要自动化批量出片可以再走 `/workspace/OpenMontage`（本地已有一套 AgentKits 视频流水线）。
