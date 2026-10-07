//! 九键（T9）：数字串 -> 拼音展开 -> 候选。
//!
//! 每个数字键展开为若干字母（按拼音常用度排序），笛卡尔积受预算约束
//! （≤3 位全展开 36 种，4 位 24 种，5 位以上 12 种），逐个做"精确+前缀"廉价查询，
//! 结果去重后按展开优先级合并。缓存同一数字串，避免连击重复计算。

use crate::engine;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 键位 -> 字母（按拼音用字频率排序）
fn letters_of(digit: char) -> &'static [char] {
    match digit {
        '2' => &['a', 'b', 'c'],
        '3' => &['e', 'd', 'f'],
        '4' => &['g', 'h', 'i'],
        '5' => &['j', 'k', 'l'],
        '6' => &['n', 'm', 'o'],
        '7' => &['s', 'p', 'r', 'q'],
        '8' => &['t', 'u', 'v'],
        '9' => &['w', 'y', 'x', 'z'],
        _ => &[],
    }
}

/// 数字串 -> 拼音展开（不做剪枝，仅供小规模/测试使用）。
pub fn expand(digits: &str, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![String::new()];
    for ch in digits.chars() {
        let letters = letters_of(ch);
        if letters.is_empty() {
            return Vec::new();
        }
        let mut next: Vec<String> = Vec::with_capacity(out.len() * letters.len());
        for prefix in &out {
            for l in letters {
                if next.len() >= cap {
                    break;
                }
                let mut s = prefix.clone();
                s.push(*l);
                next.push(s);
            }
        }
        out = next;
        if out.is_empty() {
            break;
        }
    }
    out
}

/// 带词典前缀剪枝的展开：只保留"还能长出真实词条"的中间串。
/// 这样 "64426" 会保留 nihao（你好）路径，而不是被前 12 个无意义组合占满。
fn expand_pruned(digits: &str, branch_cap: usize) -> Vec<String> {
    let dict = engine::engine().dict();
    let mut cur: Vec<String> = vec![String::new()];
    for ch in digits.chars() {
        let letters = letters_of(ch);
        if letters.is_empty() {
            return Vec::new();
        }
        let mut next: Vec<String> = Vec::new();
        for prefix in &cur {
            for l in letters {
                let mut s = prefix.clone();
                s.push(*l);
                if dict.prefix_exists(&s) {
                    next.push(s);
                }
            }
        }
        next.sort();
        next.dedup();
        if next.len() > branch_cap {
            next.truncate(branch_cap);
        }
        cur = next;
        if cur.is_empty() {
            break;
        }
    }
    cur
}

fn cache() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static C: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

const CACHE_CAP: usize = 48;

/// 九键候选：合并各展开的廉价查询结果。
pub fn candidates(digits: &str, limit: usize) -> Vec<String> {
    let key: String = digits.chars().filter(|c| c.is_ascii_digit()).collect();
    if key.is_empty() || limit == 0 {
        return Vec::new();
    }
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = c.get(&key) {
            let mut v = hit.clone();
            v.truncate(limit);
            return v;
        }
    }
    let eng = engine::engine();
    // (0=精确命中该拼音, 1=前缀补全) 优先，其次保持展开顺序
    let mut scored: Vec<(u8, usize, String)> = Vec::new();
    for (i, exp) in expand_pruned(&key, 400).into_iter().enumerate() {
        if exp.is_empty() {
            continue;
        }
        // 一次查 2 条就够判「精确命中」：`lookup_cheap` 先做精确查找，精确结果排在第 0 位，
        // 原来为了拿这个排序额外查一遍 limit=1，每个展开拼音都白跑一次 FST
        let hits = engine::lookup_cheap(eng, &exp, 2);
        for w in &hits {
            if scored.iter().any(|(_, _, x)| x == w) {
                continue;
            }
            let rank = if hits.first() == Some(w) { 0u8 } else { 1u8 };
            scored.push((rank, i, w.clone()));
            if scored.len() >= limit * 4 {
                break;
            }
        }
        if scored.len() >= limit * 4 {
            break;
        }
    }
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut out: Vec<String> = Vec::new();
    for (_, _, w) in scored {
        if !out.contains(&w) {
            out.push(w);
            if out.len() >= limit {
                break;
            }
        }
    }
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= CACHE_CAP {
            c.clear();
        }
        c.insert(key, out.clone());
    }
    out
}

/// 清缓存（学习/删词后调用）。
pub fn clear_cache() {
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_respects_cap_and_order() {
        let two = expand("64", 64);
        assert_eq!(two.len(), 9); // 3×3
        assert!(two.contains(&"ni".to_string()));
        let four = expand("6444", 24);
        assert_eq!(four.len(), 24);
        assert!(expand("11", 8).is_empty());
    }

    #[test]
    fn pruned_expansion_keeps_real_path() {
        let exps = expand_pruned("64426", 400);
        assert!(exps.contains(&"nihao".to_string()), "got {exps:?}");
    }

    #[test]
    fn t9_finds_words() {
        // 64 = ni, 426 = hao/han…，944 = xie? 用 64426 (ni + hao)
        let c = candidates("64426", 8);
        assert!(c.contains(&"你好".to_string()), "got {c:?}");
    }

    #[test]
    fn t9_single_run_is_stable() {
        let a = candidates("64426", 5);
        let b = candidates("64426", 5);
        assert_eq!(a, b);
        clear_cache();
        assert!(candidates("", 5).is_empty());
    }
}
