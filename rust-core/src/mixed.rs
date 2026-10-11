//! 混简拼：长句里允许“全拼 + 缩写”混合输入。
//!
//! 目标：`wjtxiangqugongsi` → 我今天想去公司
//!
//! 两阶段：
//! 1. k-best 切分 DP：全拼音节按覆盖字母数加分，缩写段（连续声母串 / 单字母）不加分，
//!    只保留覆盖最大的切分（能全拼就全拼，缩写只用来补全拼不出来的字母），同分切分全部保留。
//! 2. 逐条构建再排序：连续全拼段合并成一个 chunk 交给引擎 Viterbi 组合；缩写段用简拼索引
//!    （lex 声母串 + 拼音词典）选词。选词与排序都用「词频 + 常用词奖励 + bigram(上词, 本词)」。
//!
//! 护栏：至少一个单字母缩写、至少一个全拼段、缩写段占比 ≤60%、全拼段占比 ≥30%、
//! 段数 ≤24；宁缺勿滥。

use crate::engine::{lookup_cheap, split_syllables};
use crate::{gramidx, initials};
use inputx_pinyin::PinyinEngine;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 每个位置保留的切分状态数（k-best）
const BEAM: usize = 8;
/// 切分段数上限
const MAX_SEGS: usize = 24;
/// 常用词奖励：让「今天」这类常用词压过略高频的生僻同音缩写
const COMMON_BONUS: f64 = 10_000.0;
/// 超长全拼 chunk 切块上限（引擎单次 Viterbi 上限 30 字节，留余量）
const MAX_CHUNK: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Full(usize),
    Run(usize),
    Abbr,
}

impl Kind {
    fn width(self) -> usize {
        match self {
            Kind::Full(l) | Kind::Run(l) => l,
            Kind::Abbr => 1,
        }
    }

    /// 切分签名用（区分同长度的 Full/Run）
    fn code(self) -> u64 {
        match self {
            Kind::Full(l) => 1 + l as u64,
            Kind::Run(l) => 16 + l as u64,
            Kind::Abbr => 32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Step {
    score: i32,
    sig: u64,
    start: usize,
    kind: Kind,
    prev: usize,
}

/// 首字母 -> 候选单字缓存
type CharCache = Mutex<HashMap<char, Vec<(String, u64)>>>;
/// 声母串 -> 候选词缓存
type RunCache = Mutex<HashMap<String, Vec<(String, u64)>>>;

/// 三处缓存上限：都以用户输入为 key 无界增长，长会话只涨不跌。
/// 到量整清（与 `engine::CACHE_CAP` 同策略，便宜且够用）。
const WORD_FREQ_CAP: usize = 2048;
const CHAR_CACHE_CAP: usize = 64;
const RUN_CACHE_CAP: usize = 256;

fn common_bonus(word: &str) -> f64 {
    if initials::is_common(word) {
        COMMON_BONUS
    } else {
        0.0
    }
}

/// 词的声母串（与生成器同一算法：逐字首选读音取首字母）
fn word_initials(word: &str) -> Option<String> {
    let py = crate::userdic::name_to_pinyin(word)?;
    let segs = inputx_pinyin::segment(&py);
    let first = segs.first()?;
    if first.syllables.len() != word.chars().count() {
        return None;
    }
    Some(
        first
            .syllables
            .iter()
            .filter_map(|x| x.chars().next())
            .collect(),
    )
}

/// 词典词频（缓存；不在词典里的词返回 0）
fn word_freq(eng: &PinyinEngine, word: &str) -> u64 {
    fn cache() -> &'static Mutex<HashMap<String, u64>> {
        static C: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
        C.get_or_init(|| Mutex::new(HashMap::new()))
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = c.get(word) {
            return *f;
        }
    }
    let mut freq = 0u64;
    if let Some(py) = crate::userdic::name_to_pinyin(word) {
        let mut hits: Vec<(String, u64)> = Vec::new();
        eng.dict().lookup_with_freq_into(&py, &mut hits);
        for (w, f) in &hits {
            if w == word {
                freq = *f;
                break;
            }
        }
    }
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= WORD_FREQ_CAP {
            c.clear();
        }
        c.insert(word.to_string(), freq);
    }
    freq
}

/// 首字母 -> 最高频单字（缓存；一次 prefix 扫描约 1–2ms）
fn chars_for_initial(eng: &PinyinEngine, letter: char, k: usize) -> Vec<(String, u64)> {
    fn cache() -> &'static CharCache {
        static C: OnceLock<CharCache> = OnceLock::new();
        C.get_or_init(|| Mutex::new(HashMap::new()))
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = c.get(&letter) {
            return hit.iter().take(k).cloned().collect();
        }
    }
    let mut hits: Vec<(u64, String)> = Vec::new();
    eng.dict()
        .prefix_for_each(&letter.to_string(), |_code, word, freq| {
            if word.chars().count() == 1 {
                hits.push((freq, word.to_string()));
            }
        });
    hits.sort_by_key(|x| std::cmp::Reverse(x.0));
    let list: Vec<(String, u64)> = hits.into_iter().take(8).map(|(f, w)| (w, f)).collect();
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= CHAR_CACHE_CAP {
            c.clear();
        }
        c.insert(letter, list.clone());
    }
    list.into_iter().take(k).collect()
}

/// 声母串候选（带词频）：拼音词典简拼索引 + lex 声母串索引（词频反查），按词频+常用词排序
fn run_candidates(eng: &PinyinEngine, run: &str, k: usize) -> Vec<(String, u64)> {
    fn cache() -> &'static RunCache {
        static C: OnceLock<RunCache> = OnceLock::new();
        C.get_or_init(|| Mutex::new(HashMap::new()))
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = c.get(run) {
            return hit.iter().take(k).cloned().collect();
        }
    }
    let want = run.chars().count();
    let mut out: Vec<(String, u64)> = Vec::new();
    for (w, f) in initials::exact_with_freq(run, 32) {
        if w.chars().count() == want && !out.iter().any(|(x, _)| x == &w) {
            out.push((w, f));
        }
    }
    for w in gramidx::lookup(run, 32) {
        if w.chars().count() != want || out.iter().any(|(x, _)| x == &w) {
            continue;
        }
        if word_initials(&w).as_deref() != Some(run) {
            continue; // 错桶（多音字）过滤
        }
        let f = word_freq(eng, &w);
        if f > 0 {
            out.push((w, f));
        }
    }
    out.sort_by(|a, b| {
        let sa = a.1 as f64 + common_bonus(&a.0);
        let sb = b.1 as f64 + common_bonus(&b.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(16);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= RUN_CACHE_CAP {
            c.clear();
        }
        c.insert(run.to_string(), out.clone());
    }
    out
}

/// 声母串是否存在候选词（DP 剪枝用，不做完整解析）
fn run_has_word(run: &str) -> bool {
    !gramidx::lookup(run, 1).is_empty() || !initials::exact_with_freq(run, 1).is_empty()
}

/// 词打分：词频 + 常用词奖励 + bigram(上词, 本词)
fn word_score(eng: &PinyinEngine, word: &str, freq: u64, prev: Option<&str>) -> f64 {
    freq as f64 + common_bonus(word) + eng.dict().bigram_boost(prev, word)
}

fn pick_best(eng: &PinyinEngine, cands: &[(String, u64)], prev: Option<&str>) -> Option<String> {
    let mut best: Option<(f64, &str)> = None;
    for (w, f) in cands {
        let s = word_score(eng, w, *f, prev);
        if best.as_ref().map(|(bs, _)| s > *bs).unwrap_or(true) {
            best = Some((s, w.as_str()));
        }
    }
    best.map(|(_, w)| w.to_string())
}

/// k-best 切分：只返回“全拼覆盖最大”的切分（同分全部返回）
fn parse_chains(eng: &PinyinEngine, input: &str) -> Vec<Vec<(usize, Kind)>> {
    let n = input.len();
    if n == 0 {
        return Vec::new();
    }
    let mut dp: Vec<Vec<Step>> = vec![Vec::new(); n + 1];
    dp[0].push(Step {
        score: 0,
        sig: 0,
        start: 0,
        kind: Kind::Full(0),
        prev: 0,
    });
    for i in 1..=n {
        let mut cands: Vec<Step> = Vec::new();
        // 1) 全拼音节：得分 = 覆盖字母数（“能全拼就全拼”）
        for len in 1..=6.min(i) {
            let s = &input[i - len..i];
            if !s.chars().all(|c| c.is_ascii_lowercase()) || !inputx_pinyin::is_valid_syllable(s) {
                continue;
            }
            for (idx, st) in dp[i - len].iter().enumerate() {
                let kind = Kind::Full(len);
                cands.push(Step {
                    score: st.score + len as i32,
                    sig: st.sig.rotate_left(9) ^ ((i as u64) << 8) ^ kind.code(),
                    start: i - len,
                    kind,
                    prev: idx,
                });
            }
        }
        // 2) 连续缩写整体命中简拼词（不覆盖全拼得分，只作为切分候选）
        for len in 2..=4.min(i) {
            let run = &input[i - len..i];
            if !run.chars().all(|c| c.is_ascii_lowercase()) || !run_has_word(run) {
                continue;
            }
            for (idx, st) in dp[i - len].iter().enumerate() {
                let kind = Kind::Run(len);
                cands.push(Step {
                    score: st.score,
                    sig: st.sig.rotate_left(9) ^ ((i as u64) << 8) ^ kind.code(),
                    start: i - len,
                    kind,
                    prev: idx,
                });
            }
        }
        // 3) 单字母缩写
        let b = input.as_bytes()[i - 1];
        if b.is_ascii_lowercase() && !chars_for_initial(eng, b as char, 1).is_empty() {
            for (idx, st) in dp[i - 1].iter().enumerate() {
                let kind = Kind::Abbr;
                cands.push(Step {
                    score: st.score,
                    sig: st.sig.rotate_left(9) ^ ((i as u64) << 8) ^ kind.code(),
                    start: i - 1,
                    kind,
                    prev: idx,
                });
            }
        }
        cands.sort_by_key(|a| std::cmp::Reverse(a.score));
        let mut sigs: Vec<u64> = Vec::new();
        for c in cands {
            if sigs.contains(&c.sig) {
                continue;
            }
            sigs.push(c.sig);
            dp[i].push(c);
            if dp[i].len() >= BEAM {
                break;
            }
        }
    }
    let Some(max_score) = dp[n].first().map(|s| s.score) else {
        return Vec::new();
    };
    let mut out: Vec<Vec<(usize, Kind)>> = Vec::new();
    for (idx, st) in dp[n].iter().enumerate() {
        if st.score != max_score {
            continue;
        }
        let mut segs: Vec<(usize, Kind)> = Vec::new();
        let (mut pos, mut si) = (n, idx);
        loop {
            let s = dp[pos][si];
            if !matches!(s.kind, Kind::Full(0)) {
                segs.push((s.start, s.kind));
            }
            if s.start == 0 {
                break;
            }
            pos = s.start;
            si = s.prev;
        }
        segs.reverse();
        out.push(segs);
    }
    out
}

/// 全拼 chunk 组合：连续全拼段合并后交给引擎 Viterbi（超长再按音节切块）
fn compose_chunk(eng: &PinyinEngine, chunk: &str) -> Option<Vec<String>> {
    if chunk.len() < 4 {
        return lookup_cheap(eng, chunk, 1)
            .into_iter()
            .next()
            .map(|w| vec![w]);
    }
    if chunk.len() <= 30 {
        return eng
            .dict()
            .best_composition_chain(chunk)
            .map(|(_, _, words)| words);
    }
    let syls = split_syllables(chunk);
    if syls.is_empty() {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_syls = 0usize;
    for s in &syls {
        if !cur.is_empty() && (cur.len() + s.len() > MAX_CHUNK || cur_syls >= 8) {
            out.extend(compose_chunk(eng, &cur)?);
            cur.clear();
            cur_syls = 0;
        }
        cur.push_str(s);
        cur_syls += 1;
    }
    if !cur.is_empty() {
        out.extend(compose_chunk(eng, &cur)?);
    }
    Some(out)
}

/// 按切分构建句子（任一段取不到词就放弃整条切分）。
/// 返回 (是否缩写段, 词)，排序时只评缩写段（全拼 chunk 由引擎 Viterbi 负责）。
fn build(eng: &PinyinEngine, input: &str, chain: &[(usize, Kind)]) -> Option<Vec<(bool, String)>> {
    let mut parts: Vec<(bool, String)> = Vec::new();
    let mut i = 0usize;
    while i < chain.len() {
        match chain[i].1 {
            Kind::Full(_) => {
                let start = chain[i].0;
                let mut end = start + chain[i].1.width();
                while i + 1 < chain.len() && matches!(chain[i + 1].1, Kind::Full(_)) {
                    i += 1;
                    end = chain[i].0 + chain[i].1.width();
                }
                let chunk = &input[start..end];
                parts.extend(compose_chunk(eng, chunk)?.into_iter().map(|w| (false, w)));
                i += 1;
            }
            Kind::Run(len) => {
                let start = chain[i].0;
                let run = &input[start..start + len];
                let cands = run_candidates(eng, run, 16);
                let prev = parts.last().map(|(_, w)| w.as_str());
                let word = pick_best(eng, &cands, prev)?;
                parts.push((true, word));
                i += 1;
            }
            Kind::Abbr => {
                let mut run = String::new();
                let mut j = i;
                while j < chain.len() && matches!(chain[j].1, Kind::Abbr) {
                    run.push(input.as_bytes()[chain[j].0] as char);
                    j += 1;
                }
                // 连续缩写优先整体命中简拼词（wjt -> 3 字词）；整体不成词就放弃，
                // 不退化成「我+就+他」这类高词频但语义错误的全单字序列
                let prev = parts.last().map(|(_, w)| w.as_str());
                let whole = run_candidates(eng, &run, 16);
                if whole.is_empty() {
                    if run.chars().count() > 1 {
                        return None;
                    }
                    let cands = chars_for_initial(eng, run.chars().next()?, 8);
                    let word = pick_best(eng, &cands, prev)?;
                    parts.push((true, word));
                } else {
                    let word = pick_best(eng, &whole, prev)?;
                    parts.push((true, word));
                }
                i = j;
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

/// 排序分：只评缩写段（词频 + 常用词奖励 + 相邻缩写词 bigram）。
/// 全拼 chunk 由引擎 Viterbi 按词频+bigram 自行组合，且各切分的 chunk 内容相同；
/// 若把「缩写词→chunk 首词」的边界 bigram 也算进来，会奖励「忘记+他」这类
/// 只是碰巧与后续 chunk 首词搭配的高频错误缩写（如 他→想）。
fn rank(eng: &PinyinEngine, parts: &[(bool, String)]) -> f64 {
    let mut total = 0.0;
    let mut prev: Option<&str> = None;
    let mut prev_abbr = false;
    for (is_abbr, w) in parts {
        if *is_abbr {
            total += word_freq(eng, w) as f64 + common_bonus(w);
            if prev_abbr {
                if let Some(p) = prev {
                    total += eng.dict().bigram_boost(Some(p), w);
                }
            }
        }
        prev = Some(w.as_str());
        prev_abbr = *is_abbr;
    }
    total
}

/// 混简拼组合：仅在“纯全拼无结果”时作为兜底调用。
pub fn compose(eng: &PinyinEngine, input: &str, limit: usize) -> Vec<String> {
    if input.len() < 4 || limit == 0 || !input.is_ascii() {
        return Vec::new();
    }
    let chains = parse_chains(eng, input);
    let mut best: Option<(f64, String)> = None;
    for chain in &chains {
        let segs = chain.len();
        if segs == 0 || segs > MAX_SEGS {
            continue;
        }
        let abbr = chain
            .iter()
            .filter(|(_, k)| matches!(k, Kind::Abbr))
            .count();
        let full = chain
            .iter()
            .filter(|(_, k)| matches!(k, Kind::Full(_)))
            .count();
        if abbr == 0 || full == 0 {
            continue;
        }
        if abbr * 10 > segs * 6 || full * 10 < segs * 3 {
            continue;
        }
        let Some(parts) = build(eng, input, chain) else {
            continue;
        };
        let joined: String = parts.iter().map(|(_, w)| w.as_str()).collect();
        if joined.is_empty() {
            continue;
        }
        let r = rank(eng, &parts);
        if best.as_ref().map(|(br, _)| r > *br).unwrap_or(true) {
            best = Some((r, joined));
        }
    }
    best.map(|(_, s)| vec![s]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine;

    fn eng() -> &'static PinyinEngine {
        engine::engine()
    }

    #[test]
    fn mixed_shorthand_composes_sentence() {
        let _g = crate::test_lock();
        // CI 会先生成 gram.bin（lex 声母串索引）；缺失时仍可走拼音词典简拼索引
        let base = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../app/src/main/assets/gram.bin"
        );
        let _ = crate::gramidx::load(base);
        let out = compose(eng(), "wjtxiangqugongsi", 3);
        assert_eq!(out, vec!["我今天想去公司".to_string()], "-> {out:?}");
    }

    #[test]
    fn pure_full_pinyin_is_left_to_normal_path() {
        let _g = crate::test_lock();
        assert!(compose(eng(), "jintiankaihui", 3).is_empty());
        assert!(compose(eng(), "nihao", 3).is_empty());
    }

    #[test]
    fn guardrails_block_gibberish() {
        let _g = crate::test_lock();
        assert!(compose(eng(), "qqqq", 3).is_empty());
        assert!(compose(eng(), "abc", 3).is_empty());
    }
}
