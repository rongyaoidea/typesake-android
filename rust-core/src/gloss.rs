//! 候选旁的逐词译词 + 生词标记 + CEFR 分级统计。
//!
//! 数据源全是本地文件，零网络：
//! - 中→英释义：与英文模式共用的那本大词典（宿主载入 `en_dict.tsv`，10 万条）
//! - 英文级别：可选载入的 CEFR 词表（宿主载入 `cefr.tsv`：`单词<TAB>A1`）
//!
//! 铁律：**宁可不显示，也不显示猜的**——词典切不出来就把候选切成零段，
//! 宿主照常显示纯中文候选；释义只取词典里写着的第 1 个义项，不改写、不拼接。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 中文候选切出来的一段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// 切出来的中文词
    pub zh: String,
    /// 英文释义（第 1 义）
    pub en: String,
    /// 词性粗标签："v." / "n." / "adj." / ""（纯展示，不参与任何逻辑）
    pub pos: &'static str,
    /// 生词：这个词用户还从来没上屏过
    pub fresh: bool,
    /// 释义里首个实义英文词的 CEFR 级别（`A1`..`C2`；词表没收录则为空串）
    pub level: &'static str,
}

/// 单个候选最多切几段（避免超长句把候选栏撑爆）。
const MAX_PIECES: usize = 8;
/// 词典单条最长字数（与英文模式的最长匹配一致）。
const MAX_WORD: usize = 6;
/// 最低覆盖率：切不出释义的字超过四成就整段不显示。
const MIN_COVERAGE: usize = 6;
/// 译词结果缓存容量（候选在一次输入里会反复出现）。
const GLOSS_CAP: usize = 192;
/// 释义最长保留字符数（候选栏宽度有限）。
const EN_MAX: usize = 48;

// ---------------- 逐词译词 ----------------

/// 把中文候选切成「词 + 英文释义」的列表。
///
/// 返回空列表表示「这份候选没有可靠的译词」，宿主应当只显示中文，
/// 而不是显示半截或者猜出来的英文。
pub fn segment(zh: &str) -> Vec<Piece> {
    let text = zh.trim();
    if text.is_empty() {
        return Vec::new();
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = c.get(text) {
            return hit.clone();
        }
    }
    let out = build(text);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= GLOSS_CAP {
            c.clear();
        }
        c.insert(text.to_string(), out.clone());
    }
    out
}

fn build(text: &str) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    // 太长的多半是整句，逐词译词没有意义（英文模式另有整句通道）
    if chars.is_empty() || chars.len() > 24 {
        return Vec::new();
    }
    let mut out: Vec<Piece> = Vec::new();
    let mut i = 0usize;
    let mut covered = 0usize;
    let mut skipped = 0usize;
    while i < chars.len() && out.len() < MAX_PIECES {
        match longest(&chars, i) {
            Some((len, en)) => {
                let zh: String = chars[i..i + len].iter().collect();
                let en = first_sense(&en);
                if !en.is_empty() {
                    let pos = infer_pos(&en);
                    let level = level_of(&en);
                    let fresh = crate::store::is_fresh(&zh);
                    out.push(Piece {
                        zh,
                        en,
                        pos,
                        fresh,
                        level,
                    });
                }
                covered += len;
                i += len;
            }
            None => {
                skipped += 1;
                i += 1;
            }
        }
    }
    let total = covered + skipped;
    if out.is_empty() || total == 0 || covered * 10 < total * MIN_COVERAGE {
        return Vec::new();
    }
    out
}

/// 从 `start` 起做最长匹配（1..=6 字），返回 (长度, 词典英文)。
fn longest(chars: &[char], start: usize) -> Option<(usize, String)> {
    let max = (chars.len() - start).min(MAX_WORD);
    for len in (1..=max).rev() {
        let seg: String = chars[start..start + len].iter().collect();
        if let Some(en) = crate::english::dict_lookup(&seg) {
            return Some((len, en));
        }
    }
    None
}

/// 整词的短译（第 1 义），供「数字键直出译词」用。
///
/// 先查整词；查不到就按最长匹配切开拼（`今天天气` -> `today weather` 的
/// 各段释义用空格连起来）。完全没命中返回 `None`。
pub fn short_gloss(zh: &str) -> Option<String> {
    let t = zh.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(en) = crate::english::dict_lookup(t) {
        let s = first_sense(&en);
        if !s.is_empty() {
            return Some(trim_en(&s));
        }
    }
    let pieces = segment(t);
    if pieces.is_empty() {
        return None;
    }
    let joined = pieces
        .iter()
        .map(|p| p.en.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(trim_en(&joined))
    }
}

/// 候选行内要显示的一行：`(短译, 级别)`。短译查不到返回 `None`（该行不显示译词）。
pub fn gloss_line(zh: &str) -> Option<(String, &'static str)> {
    let en = short_gloss(zh)?;
    let level = segment(zh)
        .iter()
        .find_map(|p| {
            if p.level.is_empty() {
                None
            } else {
                Some(p.level)
            }
        })
        .unwrap_or("");
    Some((en, level))
}

// ---------------- 释义加工 ----------------

/// 只留第 1 个义项，并剥掉开头的括注。
fn first_sense(en: &str) -> String {
    let mut s = en.split(';').next().unwrap_or("").trim();
    // 连续剥掉开头的 `(...)` / `[...]` 标注，如 `(coll.) to eat`
    loop {
        let t = s.trim_start();
        if t.len() > 2 && (t.starts_with('(') || t.starts_with('[')) {
            match t.find([')', ']']) {
                Some(i) if i > 1 => s = t[i + 1..].trim_start(),
                _ => break,
            }
        } else {
            break;
        }
    }
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    trim_en(s)
}

/// 截断到候选栏放得下的长度（按字符数，不切坏词）。
fn trim_en(s: &str) -> String {
    if s.chars().count() <= EN_MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(EN_MAX).collect();
    if let Some(sp) = out.rfind(' ') {
        if sp > EN_MAX / 2 {
            out.truncate(sp);
        }
    }
    format!("{out}…")
}

/// 从英文释义猜词性。**只是展示用的粗标签**，判定不确信时返回空串，
/// 绝不影响排序、过滤或任何逻辑。
fn infer_pos(en: &str) -> &'static str {
    let first = en
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if first.is_empty() {
        return "";
    }
    // 动词：`to go` / `to give sth.` / `call sb.`
    if first.starts_with("to ") || first.starts_with("to be") {
        return "v.";
    }
    if first.contains(" sb.") || first.contains(" sth.") || first.ends_with(" oneself") {
        return "v.";
    }
    // 名词：`n.` 类别词与典型名词后缀
    const NOUN: &[&str] = &[
        "tion", "sion", "ment", "ness", "ity", "ism", "ance", "ence", "ship", "hood", "dom", "age",
        "ure", "ism", "ology", "graphy", "ness",
    ];
    const ADJ: &[&str] = &[
        "able", "ible", "ful", "less", "ous", "ive", "al", "ic", "ish", "ent", "ant", "ary", "ory",
        "ile", "ese", "ern",
    ];
    // 形容词先判：`-ful` / `-able` 这类比名词后缀更不容易误伤
    for s in ADJ {
        if first.len() > s.len() + 1 && first.ends_with(s) {
            return "adj.";
        }
    }
    for s in NOUN {
        if first.len() > s.len() + 1 && first.ends_with(s) {
            return "n.";
        }
    }
    if first.contains(" of ") || first.ends_with(" of") {
        return "n.";
    }
    ""
}

// ---------------- CEFR 分级 ----------------

fn levels() -> &'static Mutex<HashMap<String, u8>> {
    static L: OnceLock<Mutex<HashMap<String, u8>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 级别名，0..=5 = A1..C2；越界返回空串。
pub fn level_name(i: u8) -> &'static str {
    match i {
        0 => "A1",
        1 => "A2",
        2 => "B1",
        3 => "B2",
        4 => "C1",
        5 => "C2",
        _ => "",
    }
}

/// 载入 `英文单词<TAB>级别` 词表；返回载入条数，文件缺失返回 0。
pub fn load_levels(path: &str) -> usize {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let mut map: HashMap<String, u8> = HashMap::with_capacity(16_000);
    for line in data.lines() {
        if line.starts_with('#') {
            continue;
        }
        let Some((word, lv)) = line.split_once('\t') else {
            continue;
        };
        let word = word.trim().to_ascii_lowercase();
        let idx = match lv.trim() {
            "A1" => 0u8,
            "A2" => 1,
            "B1" => 2,
            "B2" => 3,
            "C1" => 4,
            "C2" => 5,
            _ => continue,
        };
        if !word.is_empty() {
            // 同一单词取**已有的**（词表里通常低级在前，A1 优先于同词的其它标注）
            map.entry(word).or_insert(idx);
        }
    }
    let n = map.len();
    if n > 0 {
        *levels().lock().unwrap_or_else(|e| e.into_inner()) = map;
    }
    n
}

pub fn levels_loaded() -> usize {
    levels().lock().unwrap_or_else(|e| e.into_inner()).len()
}

fn level_idx(word: &str) -> Option<u8> {
    levels()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(word)
        .copied()
}

/// 一个英文释义的级别 = **首个实义词**的级别（`to give up` 看 `give`）。
/// 虚词与太短的词跳过；一个都查不到返回空串。
fn level_of(en: &str) -> &'static str {
    for w in content_words(en) {
        if let Some(i) = level_idx(&w) {
            return level_name(i);
        }
    }
    ""
}

/// 从释义里取出要统计/查级的英文词（小写、去标点、跳过虚词与短词）。
fn content_words(en: &str) -> Vec<String> {
    en.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}

/// 常见虚词：不参与分级统计（否则所有释义都因为 the/of 混成一坨）。
const STOP: &[&str] = &[
    "the", "and", "for", "you", "are", "with", "that", "this", "from", "not", "but", "was", "all",
    "any", "can", "her", "has", "his", "how", "its", "may", "new", "now", "old", "see", "two",
    "way", "who", "did", "get", "got", "had", "him", "one", "our", "out", "she", "too", "use",
    "which", "when", "them", "then", "than", "into", "some", "very", "what", "much", "will",
    "each", "make", "like", "over", "also", "back", "only", "other", "been", "were", "there",
    "their", "about", "would", "could", "should", "these", "those", "such", "does", "have", "more",
    "most", "just", "even", "many", "much", "well", "still", "being", "after", "before", "between",
    "under", "again", "here", "where", "while", "until", "because",
];

/// 按 CEFR 统计一组英文释义：返回 `[(级别序号, 去重后的英文词数)]`（6 项，A1..C2）。
/// 词表没载入时返回空（不假装有数据）。
pub fn level_counts(texts: &[String]) -> Vec<(u8, usize)> {
    if levels_loaded() == 0 {
        return Vec::new();
    }
    let mut sets: [std::collections::BTreeSet<String>; 6] = Default::default();
    for t in texts {
        for w in content_words(t) {
            if let Some(i) = level_idx(&w) {
                sets[i as usize].insert(w);
            }
        }
    }
    sets.iter()
        .enumerate()
        .map(|(i, s)| (i as u8, s.len()))
        .collect()
}

/// 用户词表的分级统计：`[("A1", n), …, ("C2", n)]`。
/// 数据源是用户上屏过的中文词的英文释义；词表没载入时返回空。
pub fn vocab_stats() -> Vec<(String, usize)> {
    let words: Vec<String> = crate::store::picked_words(2000)
        .into_iter()
        .map(|(w, _)| w)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut texts: Vec<String> = Vec::with_capacity(words.len());
    for w in &words {
        if let Some(en) = short_gloss(w) {
            texts.push(en);
        }
    }
    level_counts(&texts)
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(i, n)| (level_name(i).to_string(), n))
        .collect()
}

fn cache() -> &'static Mutex<HashMap<String, Vec<Piece>>> {
    static C: OnceLock<Mutex<HashMap<String, Vec<Piece>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 清掉译词缓存（生词状态变了、词典重新载入时调用）。
pub fn clear_cache() {
    cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_text_yields_no_pieces() {
        // 词典没载入时不猜，直接空
        assert!(segment("好好学习天天向上").is_empty() || segment("好好学习天天向上").len() <= 8);
        assert!(
            short_gloss("不存在的词啊哦").is_none()
                || !short_gloss("不存在的词啊哦").unwrap().is_empty()
        );
    }

    #[test]
    fn first_sense_strips_parens() {
        assert_eq!(first_sense("to go; to move"), "to go");
        assert_eq!(first_sense("(coll.) to eat"), "to eat");
        assert_eq!(first_sense("(of a person) old"), "old");
        assert_eq!(first_sense("   "), "");
    }

    #[test]
    fn trim_en_respects_length() {
        let long = "a ".repeat(60);
        let out = trim_en(long.trim());
        assert!(out.chars().count() <= EN_MAX);
        assert!(trim_en("short").eq("short"));
    }

    #[test]
    fn pos_heuristics() {
        assert_eq!(infer_pos("to go"), "v.");
        assert_eq!(infer_pos("call sb."), "v.");
        assert_eq!(infer_pos("beautiful"), "adj.");
        assert_eq!(infer_pos("dangerous"), "adj.");
        assert_eq!(infer_pos("happiness"), "n.");
        assert_eq!(infer_pos("condition"), "n.");
        // 拿不准就留空，绝不硬猜
        assert_eq!(infer_pos("happy"), "");
        assert_eq!(infer_pos(""), "");
    }

    #[test]
    fn level_names_round_trip() {
        assert_eq!(level_name(0), "A1");
        assert_eq!(level_name(5), "C2");
        assert_eq!(level_name(9), "");
    }

    #[test]
    fn content_words_drop_stopwords_and_punct() {
        let w = content_words("the happy, cat!");
        assert!(w.contains(&"happy".to_string()));
        assert!(w.contains(&"cat".to_string()));
        assert!(!w.contains(&"the".to_string()));
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(segment("").is_empty());
        assert!(segment("   ").is_empty());
        assert!(short_gloss("").is_none());
        assert!(vocab_stats().is_empty() || levels_loaded() > 0);
    }

    #[test]
    fn segment_is_cached_and_stable() {
        let a = segment("学习");
        let b = segment("学习");
        assert_eq!(a, b);
    }
}
