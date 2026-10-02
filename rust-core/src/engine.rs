//! 拼音 -> 中文候选、纠错兜底、联想预测、词频与纠错学习。
//!
//! 候选组装顺序（贴合主流输入法手感）：
//! 1. 精确整词/单字（词典按词频排序，用户 pin 置顶）
//! 2. 整句 Viterbi 组合（`jintiankaihui` -> 今天开会）
//! 3. 前缀补全（`nih` -> 你好…）
//! 4. 纠错学习命中（用户此前选过的"错拼 -> 正确词"）
//! 5. 简拼（`bjdx` -> 北京大学）
//! 6. 模糊音（z/zh、n/l、an/ang…）
//! 7. 击键纠错（邻键、漏键、多键、换位）
//!
//! 4–7 只在前面完全无结果时触发，避免误纠。

use crate::{biglex, initials, mixed, shuangpin, t9};
use inputx_pinyin::{L0Snapshot, PinyinEngine};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

/// 一次"输入 -> 候选"的完整结果。
#[derive(Debug, Clone, Default)]
pub struct Match {
    /// 实际命中的拼音（可能已被纠正）
    pub matched: String,
    /// 是否经过纠错（模糊音/击键/学习）
    pub corrected: bool,
    /// 是否来自纠错学习（用户以前这么打错过）
    pub remembered: bool,
    pub candidates: Vec<String>,
}

/// 进程级引擎（首次调用时初始化，字典为 include_bytes 常量，初始化 ~34µs）。
pub fn engine() -> &'static PinyinEngine {
    static ENGINE: OnceLock<PinyinEngine> = OnceLock::new();
    ENGINE.get_or_init(PinyinEngine::new)
}

fn cache() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const CACHE_CAP: usize = 96;

/// 输入选项（由设置页写入；用原子量避免与 store 的锁产生顺序问题）。
static FUZZY_ENABLED: AtomicBool = AtomicBool::new(true);
static CORRECTION_ENABLED: AtomicBool = AtomicBool::new(true);
static SHUANGPIN: AtomicU8 = AtomicU8::new(0);

pub fn set_options(fuzzy: bool, correction: bool, shuangpin_scheme: u8) {
    FUZZY_ENABLED.store(fuzzy, Ordering::Relaxed);
    CORRECTION_ENABLED.store(correction, Ordering::Relaxed);
    SHUANGPIN.store(shuangpin_scheme, Ordering::Relaxed);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.clear();
    }
    t9::clear_cache();
}

pub fn options() -> (bool, bool, u8) {
    (
        FUZZY_ENABLED.load(Ordering::Relaxed),
        CORRECTION_ENABLED.load(Ordering::Relaxed),
        SHUANGPIN.load(Ordering::Relaxed),
    )
}

// ---------------- 上下文（上两个词） ----------------

fn last_error() -> &'static Mutex<String> {
    static E: OnceLock<Mutex<String>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(String::new()))
}

/// 记录最近一次引擎级错误（供设置页「引擎自检」展示）。
pub fn note_error(msg: &str) {
    {
        let mut e = last_error().lock().unwrap_or_else(|e| e.into_inner());
        *e = msg.to_string();
    }
}

pub fn last_error_snapshot() -> String {
    last_error()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn context() -> &'static Mutex<(String, String)> {
    static C: OnceLock<Mutex<(String, String)>> = OnceLock::new();
    C.get_or_init(|| Mutex::new((String::new(), String::new())))
}

/// 记录刚上屏的词（用于 bigram 重排与 trigram 联想）。
pub fn set_context(word: &str) {
    let w = word.trim();
    if w.is_empty() {
        return;
    }
    {
        let mut c = context().lock().unwrap_or_else(|e| e.into_inner());
        let prev = c.1.clone();
        c.0 = prev;
        c.1 = w.to_string();
    }
}

fn context_words() -> (String, String) {
    let c = context().lock().unwrap_or_else(|e| e.into_inner());
    (c.0.clone(), c.1.clone())
}

/// 用上词做 bigram 重排（只动前 5 个，避免打散精确匹配）。
fn rerank_with_context(cands: &mut [String]) {
    let (_, prev) = context_words();
    if prev.is_empty() || cands.len() < 2 {
        return;
    }
    let n = cands.len().min(5);
    let dict = engine().dict();
    let mut head: Vec<String> = cands[..n].to_vec();
    head.sort_by(|a, b| {
        let ba = dict.bigram_boost(Some(&prev), a);
        let bb = dict.bigram_boost(Some(&prev), b);
        bb.partial_cmp(&ba).unwrap_or(std::cmp::Ordering::Equal)
    });
    cands[..n].clone_from_slice(&head);
}

/// 扩展候选（候选翻页用）：不走纠错，只要更多候选。
pub fn more_candidates(input: &str, limit: usize) -> Vec<String> {
    let compact = canonical(input);
    if compact.is_empty() {
        return Vec::new();
    }
    let mut out = filter_blocked(&compact, candidates_with(engine(), &compact, limit));
    if out.len() < limit {
        for w in mixed::compose(engine(), &compact, limit) {
            if !out.contains(&w) {
                out.push(w);
            }
        }
    }
    if out.len() < limit {
        for w in compose_long(engine(), &compact, limit) {
            if !out.contains(&w) {
                out.push(w);
            }
        }
    }
    if out.len() < limit {
        for w in biglex::exact(&compact, limit) {
            if !out.contains(&w) {
                out.push(w);
            }
        }
    }
    if out.len() < limit {
        for w in filter_blocked(&compact, initials::candidates(&compact, limit)) {
            if !out.contains(&w) {
                out.push(w);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    rerank_with_context(&mut out);
    out
}

/// 九键候选（数字串 -> 候选），带上下文重排。
pub fn t9_candidates(digits: &str, limit: usize) -> Vec<String> {
    let raw = t9::candidates(digits, limit);
    let key: String = digits.chars().filter(|c| c.is_ascii_digit()).collect();
    let mut out = filter_blocked(&key, raw);
    rerank_with_context(&mut out);
    out
}

/// 纠错学习：「打错的拼音串 -> 用户真正想要的词」。
fn learned() -> &'static Mutex<HashMap<String, (String, u32)>> {
    static L: OnceLock<Mutex<HashMap<String, (String, u32)>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn export_learned() -> Vec<(String, String, u32)> {
    let m = learned().lock().unwrap_or_else(|e| e.into_inner());
    let mut v: Vec<(String, String, u32)> = m
        .iter()
        .map(|(k, (w, c))| (k.clone(), w.clone(), *c))
        .collect();
    v.sort();
    v
}

pub fn import_learned(items: Vec<(String, String, u32)>) {
    {
        let mut m = learned().lock().unwrap_or_else(|e| e.into_inner());
        m.clear();
        for (typed, word, count) in items {
            if !typed.is_empty() && !word.is_empty() {
                m.insert(typed, (word, count.max(1)));
            }
        }
    }
}

/// 记住一次纠错选择；同一 (错拼, 词) 多次选择会累计。
pub fn remember(typed: &str, word: &str) {
    let key = canonical(typed);
    if key.is_empty() || word.is_empty() {
        return;
    }
    {
        let mut m = learned().lock().unwrap_or_else(|e| e.into_inner());
        if m.len() > 400 {
            m.clear();
        }
        let e = m.entry(key).or_insert((word.to_string(), 0));
        if e.0 != word {
            *e = (word.to_string(), 1);
        } else {
            e.1 = e.1.saturating_add(1);
        }
    }
}

/// 删词黑名单：「该拼音串下永远不再出现这个词」。
fn blocked() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static B: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn export_blocked() -> Vec<(String, String)> {
    let m = blocked().lock().unwrap_or_else(|e| e.into_inner());
    let mut v: Vec<(String, String)> = m
        .iter()
        .flat_map(|(p, words)| words.iter().map(move |w| (p.clone(), w.clone())))
        .collect();
    v.sort();
    v
}

pub fn import_blocked(items: Vec<(String, String)>) {
    {
        let mut m = blocked().lock().unwrap_or_else(|e| e.into_inner());
        m.clear();
        for (p, w) in items {
            if !p.is_empty() && !w.is_empty() {
                m.entry(p).or_default().push(w);
            }
        }
    }
}

fn is_blocked(pinyin: &str, word: &str) -> bool {
    let m = blocked().lock().unwrap_or_else(|e| e.into_inner());
    m.get(&normalize(pinyin))
        .map(|v| v.iter().any(|w| w == word))
        .unwrap_or(false)
}

fn filter_blocked(pinyin: &str, list: Vec<String>) -> Vec<String> {
    if blocked()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty()
    {
        return list;
    }
    list.into_iter()
        .filter(|w| !is_blocked(pinyin, w))
        .collect()
}

fn learned_for(typed: &str) -> Option<String> {
    let key = normalize(typed);
    let word = learned()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .map(|(w, _)| w.clone())?;
    if is_blocked(&key, &word) {
        return None;
    }
    Some(word)
}

/// 词典条目数。
pub fn lexicon_size() -> usize {
    engine().dict().len()
}

/// 规整输入：仅保留 ascii 字母并转小写。
pub fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// 键盘击键 -> 引擎全拼：**先**按当前方案解码（双拼/注音还原成全拼，`x;` 这类
/// 会用到 `;` 的方案必须先解码再规整，否则分号先被剥掉），**再**规整。
///
/// 只在「接收键盘击键」的入口调用一次；已经还原成全拼的中间结果
/// （`candidates_with` 等）**绝不能**再过一遍——解码不是幂等的
/// （例：小鹤下 `xue` 会被读成两个新键位），二次解码会把音节打散。
pub fn canonical(input: &str) -> String {
    normalize(&shuangpin::to_full(
        input,
        SHUANGPIN.load(Ordering::Relaxed),
    ))
}

fn push_unique(out: &mut Vec<String>, word: String, limit: usize) {
    if out.len() >= limit || word.is_empty() {
        return;
    }
    if !out.iter().any(|w| w == &word) {
        out.push(word);
    }
}

/// 廉价查询：只做精确 + 前缀（用于纠错变体，避免每条都跑 Viterbi）。
pub(crate) fn lookup_cheap(eng: &PinyinEngine, compact: &str, limit: usize) -> Vec<String> {
    let dict = eng.dict();
    let mut out: Vec<String> = Vec::new();
    let mut exact: Vec<String> = Vec::new();
    dict.lookup_into(compact, &mut exact);
    for w in exact {
        push_unique(&mut out, w, limit);
    }
    if out.len() < limit && compact.len() >= 2 {
        let mut hits: Vec<(u64, String)> = Vec::new();
        let mut visited = 0usize;
        dict.prefix_for_each(compact, |code, word, freq| {
            visited += 1;
            if visited <= 200 && code.len().saturating_sub(compact.len()) <= 5 {
                hits.push((freq, word.to_string()));
            }
        });
        hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, w) in hits {
            push_unique(&mut out, w, limit);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

/// 贪心切音节（用引擎的音节表，避免 segment() 在长串上枚举爆炸）。
pub(crate) fn split_syllables(input: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    let n = input.len();
    while i < n {
        let mut take = 0usize;
        let max = 6.min(n - i);
        for len in (1..=max).rev() {
            if input.is_char_boundary(i + len)
                && inputx_pinyin::is_valid_syllable(&input[i..i + len])
            {
                take = len;
                break;
            }
        }
        if take == 0 {
            take = 1;
        }
        out.push(&input[i..i + take]);
        i += take;
    }
    out
}

/// 长句断句：在常见小句连接词前补一个逗号，句末补句号（对齐主流输入法的"长句自动标点"）。
fn punctuate(sentence: &str) -> String {
    const BREAKS: &[&str] = &[
        "但是", "所以", "然后", "因为", "而且", "不过", "如果", "我们", "他们", "你们", "今天",
        "明天", "下午", "晚上", "上午",
    ];
    if sentence.chars().count() < 12 {
        return sentence.to_string();
    }
    let mut cut: Option<usize> = None;
    for b in BREAKS {
        if let Some(pos) = sentence.find(b) {
            // 只在句中（前 4 字之后）断一次，避免过度断句
            if sentence[..pos].chars().count() >= 4 {
                cut = Some(cut.map_or(pos, |c| c.min(pos)));
            }
        }
    }
    let mut out = String::with_capacity(sentence.len() + 2);
    match cut {
        Some(pos) => {
            out.push_str(&sentence[..pos]);
            out.push('，');
            out.push_str(&sentence[pos..]);
        }
        None => out.push_str(sentence),
    }
    if !out.ends_with(['。', '！', '？']) {
        out.push('。');
    }
    out
}

/// 长句组合：按音节切块（每块 ≤ 8 音节 / ≤ 20 字母），逐块 Viterbi 再拼接。
/// 这是主流输入法处理"超长拼音串"的方式（引擎单次组合上限 30 字母）。
pub fn compose_long(eng: &PinyinEngine, compact: &str, limit: usize) -> Vec<String> {
    if compact.len() <= 24 || limit == 0 {
        return Vec::new();
    }
    let syls = split_syllables(compact);
    if syls.is_empty() {
        return Vec::new();
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_syls = 0usize;
    for s in &syls {
        if !cur.is_empty() && (cur.len() + s.len() > 20 || cur_syls >= 8) {
            chunks.push(cur.clone());
            cur.clear();
            cur_syls = 0;
        }
        cur.push_str(s);
        cur_syls += 1;
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    let dict = eng.dict();
    let mut parts: Vec<String> = Vec::with_capacity(chunks.len());
    for c in &chunks {
        let pick = if c.len() >= 4 {
            dict.top_k_compositions(c, 1)
                .first()
                .map(|(_, w)| w.clone())
        } else {
            lookup_cheap(eng, c, 1).first().cloned()
        };
        match pick {
            Some(w) if !w.is_empty() => parts.push(w),
            _ => return Vec::new(), // 任一块组合失败：宁可返回空，也不吐半句
        }
    }
    let joined = parts.concat();
    if joined.is_empty() {
        return Vec::new();
    }
    // 整句候选（长句自动补标点）+ 最后一块的备选（方便局部改错）
    let mut out = vec![punctuate(&joined)];
    if chunks.len() >= 2 {
        if let Some(last) = chunks.last() {
            for (_s, w) in dict.top_k_compositions(last, 3) {
                let head: String = parts[..parts.len() - 1].concat();
                let cand = punctuate(&format!("{head}{w}"));
                if !out.contains(&cand) {
                    out.push(cand);
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
    }
    out
}

/// 完整候选（精确 -> 整句 -> 前缀），带缓存。
pub fn candidates_with(eng: &PinyinEngine, input: &str, limit: usize) -> Vec<String> {
    let compact = normalize(input);
    if compact.is_empty() || limit == 0 {
        return Vec::new();
    }
    let dict = eng.dict();
    let mut out: Vec<String> = Vec::with_capacity(limit);

    let mut exact: Vec<String> = Vec::new();
    dict.lookup_into(&compact, &mut exact);
    for w in exact {
        push_unique(&mut out, w, limit);
    }

    if out.len() < limit && compact.len() >= 4 {
        for (_score, word) in dict.top_k_compositions(&compact, 6) {
            push_unique(&mut out, word, limit);
            if out.len() >= limit {
                break;
            }
        }
    }

    // 长句（>24 字母）：按音节切块组合（引擎单次上限 30 字母）
    if out.len() < limit {
        for w in compose_long(eng, &compact, limit) {
            push_unique(&mut out, w, limit);
        }
    }

    // 混简拼（全拼 + 单字母缩写）：仅在纯全拼没结果时兜底
    if out.is_empty() {
        for w in mixed::compose(eng, &compact, limit) {
            push_unique(&mut out, w, limit);
        }
    }

    if out.len() < limit && compact.len() >= 2 {
        let mut hits: Vec<(u64, String)> = Vec::new();
        let mut visited = 0usize;
        dict.prefix_for_each(&compact, |code, word, freq| {
            visited += 1;
            if visited <= 400 && code.len().saturating_sub(compact.len()) <= 5 {
                hits.push((freq, word.to_string()));
            }
        });
        hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, w) in hits {
            push_unique(&mut out, w, limit);
            if out.len() >= limit {
                break;
            }
        }
    }

    filter_blocked(&compact, out)
}

pub fn candidates(input: &str, limit: usize) -> Vec<String> {
    let compact = canonical(input);
    if compact.is_empty() {
        return Vec::new();
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = c.get(&compact) {
            let mut v = hit.clone();
            v.truncate(limit);
            return v;
        }
    }
    let out = candidates_with(engine(), &compact, limit);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= CACHE_CAP {
            c.clear();
        }
        c.insert(compact, out.clone());
    }
    out
}

// ---------------- 纠错变体 ----------------

/// 模糊音对（含双向）：覆盖搜狗默认的平翘舌、前后鼻音、n/l、f/h。
const FUZZY_PAIRS: &[(&str, &str)] = &[
    ("zh", "z"),
    ("ch", "c"),
    ("sh", "s"),
    ("z", "zh"),
    ("c", "ch"),
    ("s", "sh"),
    ("n", "l"),
    ("l", "n"),
    ("r", "l"),
    ("l", "r"),
    ("ang", "an"),
    ("an", "ang"),
    ("eng", "en"),
    ("en", "eng"),
    ("ing", "in"),
    ("in", "ing"),
    ("f", "h"),
    ("h", "f"),
];

/// 生成模糊音变体（单点替换，去重，受上限约束）。
pub fn fuzzy_variants(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (from, to) in FUZZY_PAIRS {
        if !input.contains(from) {
            continue;
        }
        let mut start = 0usize;
        while let Some(pos) = input[start..].find(from) {
            let at = start + pos;
            let mut v = String::with_capacity(input.len());
            v.push_str(&input[..at]);
            v.push_str(to);
            v.push_str(&input[at + from.len()..]);
            if !v.is_empty() && v != input && !out.contains(&v) {
                out.push(v);
            }
            start = at + from.len();
            if out.len() >= 48 {
                return out;
            }
        }
    }
    out
}

/// QWERTY 邻键表（击键位移纠错用）。
const NEIGHBORS: &[(char, &[char])] = &[
    ('q', &['w', 'a', 's']),
    ('w', &['q', 'e', 'a', 's', 'd']),
    ('e', &['w', 'r', 's', 'd', 'f']),
    ('r', &['e', 't', 'd', 'f', 'g']),
    ('t', &['r', 'y', 'f', 'g', 'h']),
    ('y', &['t', 'u', 'g', 'h', 'j']),
    ('u', &['y', 'i', 'h', 'j', 'k']),
    ('i', &['u', 'o', 'j', 'k', 'l']),
    ('o', &['i', 'p', 'k', 'l']),
    ('p', &['o', 'l']),
    ('a', &['q', 'w', 's', 'z', 'x']),
    ('s', &['q', 'w', 'e', 'a', 'd', 'z', 'x', 'c']),
    ('d', &['w', 'e', 'r', 's', 'f', 'x', 'c', 'v']),
    ('f', &['e', 'r', 't', 'd', 'g', 'c', 'v', 'b']),
    ('g', &['r', 't', 'y', 'f', 'h', 'v', 'b', 'n']),
    ('h', &['t', 'y', 'u', 'g', 'j', 'b', 'n', 'm']),
    ('j', &['y', 'u', 'i', 'h', 'k', 'n', 'm']),
    ('k', &['u', 'i', 'o', 'j', 'l', 'm']),
    ('l', &['i', 'o', 'p', 'k']),
    ('z', &['a', 's', 'x']),
    ('x', &['a', 's', 'd', 'z', 'c']),
    ('c', &['s', 'd', 'f', 'x', 'v']),
    ('v', &['d', 'f', 'g', 'c', 'b']),
    ('b', &['f', 'g', 'h', 'v', 'n']),
    ('n', &['g', 'h', 'j', 'b', 'm']),
    ('m', &['h', 'j', 'k', 'n']),
];

fn neighbors(c: char) -> &'static [char] {
    NEIGHBORS
        .iter()
        .find(|(k, _)| *k == c)
        .map(|(_, v)| *v)
        .unwrap_or(&[])
}

/// 插入候选（漏键）：常见元音与鼻音尾。
const INSERT_CHARS: &[char] = &['a', 'e', 'i', 'o', 'u', 'n', 'g', 'h'];

/// 击键纠错变体：(代价, 变体)。代价越小越可能是真实意图。
pub fn keystroke_variants(input: &str) -> Vec<(u8, String)> {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<(u8, String)> = Vec::new();
    if chars.len() < 2 {
        return out;
    }

    // cost 1: 相邻换位
    for i in 0..chars.len() - 1 {
        if chars[i] == chars[i + 1] {
            continue;
        }
        let mut v = chars.clone();
        v.swap(i, i + 1);
        out.push((1, v.into_iter().collect()));
    }
    // cost 1: 邻键替换
    for i in 0..chars.len() {
        for n in neighbors(chars[i]) {
            let mut v = chars.clone();
            v[i] = *n;
            out.push((1, v.into_iter().collect()));
        }
    }
    // cost 2: 多按一键 -> 删一个
    for i in 0..chars.len() {
        let mut v = chars.clone();
        v.remove(i);
        out.push((2, v.into_iter().collect()));
    }
    // cost 3: 漏按一键 -> 插一个
    for i in 0..=chars.len() {
        for c in INSERT_CHARS {
            let mut v = chars.clone();
            v.insert(i, *c);
            out.push((3, v.into_iter().collect()));
        }
    }

    out.sort_by_key(|a| a.0);
    let mut seen: Vec<String> = Vec::new();
    let mut deduped: Vec<(u8, String)> = Vec::new();
    for (cost, v) in out {
        if v != input && !seen.contains(&v) {
            seen.push(v.clone());
            deduped.push((cost, v));
        }
        if deduped.len() >= 40 {
            break;
        }
    }
    deduped
}

/// 完整分析：候选 + 是否纠错 + 命中拼音。
///
/// 关键取舍：**词典精确命中优先**（最可信）。只有当输入不是任何词条的精确键
/// （即候选只能来自整句拼接/前缀补全）时，才尝试纠错兜底，从而避免把
/// 正确输入误判成错拼。
pub fn analyze(input: &str, limit: usize) -> Match {
    // 击键先按方案解码成全拼（顺序不能反：`;` / 数字要先参与解码）
    let compact = canonical(input);
    if compact.is_empty() {
        return Match::default();
    }
    let eng = engine();

    let mut exact: Vec<String> = Vec::new();
    eng.dict().lookup_into(&compact, &mut exact);

    let mut direct = filter_blocked(&compact, candidates_with(eng, &compact, limit));
    // 大词库补充（jieba/Rime 生成的 lex.bin）：精确优先，其次前缀
    if direct.len() < limit {
        for w in biglex::exact(&compact, 2) {
            if !direct.contains(&w) {
                direct.push(w);
            }
        }
    }
    if direct.len() < limit {
        for w in biglex::prefix(&compact, 2) {
            if !direct.contains(&w) {
                direct.push(w);
            }
        }
    }
    direct.truncate(limit);
    // 个人词库（名单/通讯录导入）优先：姓名是强个人信号
    let personal = crate::store::user_words_for(&compact, 2);
    if !personal.is_empty() {
        let mut merged = personal;
        for w in direct {
            if !merged.contains(&w) {
                merged.push(w);
            }
        }
        merged.truncate(limit);
        direct = merged;
    }
    rerank_with_context(&mut direct);
    if !exact.is_empty() {
        return Match {
            matched: compact,
            corrected: false,
            remembered: false,
            candidates: direct,
        };
    }

    // 纠错学习命中：用户以前就是这么打错的
    if let Some(word) = learned_for(&compact) {
        let mut c = vec![word];
        for w in direct {
            push_unique(&mut c, w, limit);
        }
        return Match {
            matched: compact,
            corrected: true,
            remembered: true,
            candidates: c,
        };
    }

    // 简拼（声母串）
    let ini = filter_blocked(&compact, initials::candidates(&compact, limit));
    if !ini.is_empty() {
        return Match {
            matched: compact,
            corrected: false,
            remembered: false,
            candidates: ini,
        };
    }

    // 模糊音 -> 击键纠错：把纠正后的整词放最前，其余（整句拼接等）排后
    let try_variant = |v: &str| -> Option<Vec<String>> {
        let hit = filter_blocked(&compact, lookup_cheap(eng, v, 3));
        if hit.is_empty() {
            None
        } else {
            Some(hit)
        }
    };

    if FUZZY_ENABLED.load(Ordering::Relaxed) {
        for v in fuzzy_variants(&compact) {
            if let Some(mut hit) = try_variant(&v) {
                for w in direct {
                    push_unique(&mut hit, w, limit);
                }
                return Match {
                    matched: v,
                    corrected: true,
                    remembered: false,
                    candidates: hit,
                };
            }
        }
    }
    if CORRECTION_ENABLED.load(Ordering::Relaxed) {
        for (_cost, v) in keystroke_variants(&compact) {
            if let Some(mut hit) = try_variant(&v) {
                for w in direct {
                    push_unique(&mut hit, w, limit);
                }
                return Match {
                    matched: v,
                    corrected: true,
                    remembered: false,
                    candidates: hit,
                };
            }
        }
    }

    Match {
        matched: compact,
        corrected: false,
        remembered: false,
        candidates: direct,
    }
}

// ---------------- 联想 / 学习 / 删除 ----------------

/// 联想：给定上一个词预测下一个词。
pub fn predict_next(prev: &str, limit: usize) -> Vec<String> {
    let p = prev.trim();
    if p.is_empty() || limit == 0 {
        return Vec::new();
    }
    #[cfg(feature = "trigrams")]
    {
        let (prev_prev, _) = context_words();
        let cands = engine().dict().predict_next_words_context(
            Some(prev_prev.as_str()).filter(|s| !s.is_empty()),
            p,
            limit,
        );
        if !cands.is_empty() {
            return cands.into_iter().map(|(w, _c)| w).collect();
        }
    }
    engine()
        .dict()
        .predict_next_words(p, limit)
        .into_iter()
        .map(|(w, _c)| w)
        .collect()
}

/// 记录用户选词（3 连选自动 pin），返回该拼音的最新候选序。
pub fn record_pick(pinyin: &str, word: &str, limit: usize) -> Vec<String> {
    let compact = canonical(pinyin);
    let word = word.trim();
    if compact.is_empty() || word.is_empty() {
        return Vec::new();
    }
    engine().dict().record_pick(&compact, word);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.remove(&compact);
    }
    t9::clear_cache();
    candidates_with(engine(), &compact, limit)
}

/// 置顶：把某个候选固定在该拼音的首位（用户主动 pin）。
pub fn pin(pinyin: &str, word: &str, limit: usize) -> Vec<String> {
    let compact = canonical(pinyin);
    let word = word.trim();
    if compact.is_empty() || word.is_empty() {
        return Vec::new();
    }
    engine().dict().pin(&compact, word);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.remove(&compact);
    }
    candidates_with(engine(), &compact, limit)
}

/// 已删词黑名单（供设置页恢复）。
pub fn blocked_words() -> Vec<(String, String)> {
    export_blocked()
}

/// 恢复某个被删掉的词（从黑名单移除）。
pub fn unblock(pinyin: &str, word: &str) -> usize {
    let (p, w) = (canonical(pinyin), word.trim().to_string());
    let mut n = 0;
    {
        let mut b = blocked().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = b.get_mut(&p) {
            let before = v.len();
            v.retain(|x| x != &w);
            n = before - v.len();
            if v.is_empty() {
                b.remove(&p);
            }
        }
    }
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.clear();
    }
    t9::clear_cache();
    n
}

/// 删词：该拼音串下永久不再推荐这个词（并清掉它的置顶/学习记录）。
pub fn forget(pinyin: &str, word: &str, limit: usize) -> Vec<String> {
    let compact = canonical(pinyin);
    let word = word.trim();
    if compact.is_empty() || word.is_empty() {
        return Vec::new();
    }
    engine().dict().forget(&compact);
    t9::clear_cache();
    {
        let mut b = blocked().lock().unwrap_or_else(|e| e.into_inner());
        let v = b.entry(compact.clone()).or_default();
        if !v.iter().any(|w| w == word) {
            v.push(word.to_string());
        }
    }
    {
        let mut l = learned().lock().unwrap_or_else(|e| e.into_inner());
        l.retain(|k, (w, _)| !(w == word && k == &compact));
    }
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.remove(&compact);
        c.clear();
    }
    candidates_with(engine(), &compact, limit)
}

/// 用户词库视图：(拼音, 词, 选词次数)。pin 的 count 记为 0（表示已固定首位）。
pub fn learned_words() -> Vec<(String, String, u32)> {
    let snap = export_l0();
    let mut out: Vec<(String, String, u32)> = Vec::new();
    for (p, w) in snap.pins {
        out.push((p, w, 0));
    }
    for (p, w, c) in snap.pick_counts {
        if !out.iter().any(|(pp, ww, _)| pp == &p && ww == &w) {
            out.push((p, w, c));
        }
    }
    out.sort();
    out
}

/// 清空学习记录（置顶与选词计数），保留收藏。
pub fn clear_learned() -> usize {
    let n = learned_words().len();
    import_l0(Vec::new(), Vec::new());
    t9::clear_cache();
    n
}

pub fn export_l0() -> L0Snapshot {
    engine().dict().export_l0()
}

pub fn import_l0(pins: Vec<(String, String)>, pick_counts: Vec<(String, String, u32)>) -> usize {
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.clear();
    }
    engine().dict().import_l0(L0Snapshot { pins, pick_counts })
}

/// 词库规模 + 简拼索引规模（设置页展示）。
pub fn lexicon_info() -> (usize, usize) {
    (lexicon_size(), initials::size())
}

/// 大词库条数（可选层）。
pub fn biglex_size() -> usize {
    biglex::size()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> PinyinEngine {
        PinyinEngine::new()
    }

    /// 串行 + 复位全局状态，避免用例互相污染。
    fn gated() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::test_lock();
        import_learned(Vec::new());
        import_blocked(Vec::new());
        set_context("");
        set_options(true, true, 0);
        guard
    }

    #[test]
    fn engine_loads_real_dict() {
        assert!(lexicon_size() > 100_000);
    }

    #[test]
    fn exact_words_rank_first() {
        let e = fresh();
        assert_eq!(
            candidates_with(&e, "nihao", 8).first().map(String::as_str),
            Some("你好")
        );
        assert!(candidates_with(&e, "zhongguo", 8).contains(&"中国".to_string()));
        assert!(candidates_with(&e, "woaini", 8).contains(&"我爱你".to_string()));
    }

    #[test]
    fn sentence_composition_works() {
        let c = candidates_with(&fresh(), "jintiankaihui", 8);
        assert!(c.iter().any(|w| w.contains("今天")), "got {c:?}");
    }

    #[test]
    fn prefix_completion_for_partial_input() {
        assert!(!candidates_with(&fresh(), "nih", 8).is_empty());
    }

    #[test]
    fn analyze_prefers_direct_hits() {
        let _g = gated();
        let m = analyze("nihao", 8);
        assert!(!m.corrected && !m.remembered);
        assert_eq!(m.matched, "nihao");
        assert_eq!(m.candidates.first().map(String::as_str), Some("你好"));
    }

    #[test]
    fn fuzzy_zh_z_is_corrected() {
        let _g = gated();
        let m = analyze("zongguo", 8);
        assert!(m.corrected, "expected correction, got {m:?}");
        assert_eq!(m.matched, "zhongguo");
        assert!(m.candidates.contains(&"中国".to_string()));
    }

    #[test]
    fn keystroke_typos_are_recovered() {
        let _g = gated();
        for typo in ["nihap", "nhiao", "nihaoo"] {
            let m = analyze(typo, 8);
            assert!(m.corrected, "{typo} should be corrected, got {m:?}");
            assert!(
                m.candidates.iter().any(|w| w.starts_with("你好")),
                "{typo} -> {m:?}"
            );
        }
        assert_eq!(analyze("nihap", 8).matched, "nihao");
        assert_eq!(analyze("nhiao", 8).matched, "nihao");
    }

    #[test]
    fn correction_can_be_disabled() {
        let _g = gated();
        set_options(true, false, 0);
        let m = analyze("nihap", 8);
        assert!(
            m.candidates.iter().all(|w| !w.starts_with("你好")),
            "correction should be off, got {m:?}"
        );
        set_options(true, true, 0);
    }

    #[test]
    fn learned_correction_wins() {
        let _g = gated();
        remember("nihap", "你好");
        let m = analyze("nihap", 8);
        assert!(m.remembered, "expected remembered, got {m:?}");
        assert_eq!(m.candidates.first().map(String::as_str), Some("你好"));
        let items = export_learned();
        assert!(items.iter().any(|(t, w, _)| t == "nihap" && w == "你好"));
    }

    #[test]
    fn initials_shorthand() {
        let _g = gated();
        let m = analyze("nh", 8);
        assert!(!m.corrected);
        assert!(m.candidates.contains(&"你好".to_string()), "got {m:?}");
    }

    #[test]
    fn long_sentence_still_has_candidates() {
        let _g = gated();
        // 36 字母 > 引擎 top_k_compositions 的 MAX_LEN(30)：必须靠分块组合出候选
        let long = "jintianxiawuwomenyiqiqukanyidianying";
        assert!(long.len() > 30, "测试用例必须超过引擎上限");
        let m = analyze(long, 8);
        assert!(!m.candidates.is_empty(), "长句不应无候选: {m:?}");
        assert!(
            m.candidates.iter().any(|w| w.contains("今天")),
            "长句首候选应含常见词: {m:?}"
        );
        // 分块拼接不应吐半句
        assert!(m.candidates.iter().all(|w| w.chars().count() >= 4), "{m:?}");
        // 长句自动标点：句中补逗号、句末补句号
        let first = &m.candidates[0];
        assert!(first.ends_with('。'), "长句应带句号: {first}");
        assert!(first.contains('，'), "长句应在小句处补逗号: {first}");
    }

    #[test]
    fn junk_input_yields_nothing() {
        let _g = gated();
        let m = analyze("qqqqqqqqqq", 8);
        assert!(m.candidates.is_empty(), "got {m:?}");
    }

    #[test]
    fn user_learning_pins_after_three_picks() {
        let e = fresh();
        let before = candidates_with(&e, "ni", 8);
        assert_eq!(before.first().map(String::as_str), Some("你"));
        let target = before
            .iter()
            .find(|w| w.as_str() != "你")
            .cloned()
            .expect("need another candidate");
        for _ in 0..3 {
            e.dict().record_pick("ni", &target);
        }
        assert_eq!(
            candidates_with(&e, "ni", 8).first(),
            Some(&target),
            "L0 pin should promote the pick"
        );
    }

    #[test]
    fn shuangpin_scheme_is_applied() {
        let _g = gated();
        set_options(true, true, shuangpin::SCHEME_FLYPY);
        let m = analyze("nihc", 8);
        assert_eq!(m.matched, "nihao");
        assert!(m.candidates.contains(&"你好".to_string()), "got {m:?}");
        let m2 = analyze("vsgo", 8);
        assert_eq!(m2.matched, "zhongguo");
        assert!(m2.candidates.contains(&"中国".to_string()));
        set_options(true, true, 0);
    }

    #[test]
    fn context_reranks_top_candidates() {
        let _g = gated();
        set_context("我");
        set_context("喜欢");
        let c = analyze("he", 8).candidates;
        assert!(!c.is_empty(), "候选不应为空");
        // 不变式：前 5 个里 bigram 得分最高者必须排在首位（重排实现只动前 5）
        let n = c.len().min(5);
        if n >= 2 {
            let dict = engine().dict();
            let best = c[..n]
                .iter()
                .max_by(|a, b| {
                    dict.bigram_boost(Some("喜欢"), a)
                        .partial_cmp(&dict.bigram_boost(Some("喜欢"), b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned()
                .unwrap();
            assert_eq!(c[0], best, "bigram 最优候选应在首位: {c:?}");
        }
    }

    #[test]
    fn more_candidates_extends_list() {
        let _g = gated();
        let few = analyze("ni", 8).candidates.len();
        let many = more_candidates("ni", 24).len();
        assert!(
            many >= few,
            "more_candidates should not shrink: {few} -> {many}"
        );
        assert!(many > 8);
    }

    #[test]
    fn t9_pipeline_works() {
        let _g = gated();
        let c = t9_candidates("64426", 8);
        assert!(c.contains(&"你好".to_string()), "got {c:?}");
    }

    #[test]
    fn prediction_and_limits() {
        assert!(!predict_next("我", 5).is_empty());
        assert!(predict_next("", 5).is_empty());
        assert!(candidates_with(&fresh(), "nihao", 2).len() <= 2);
    }
}
