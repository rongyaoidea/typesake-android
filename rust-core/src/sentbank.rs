//! 句库（Tatoeba 中英对齐句对，可选）：整句命中 + 双字重叠打分检索。
//!
//! 文件格式（由 `tools/gen_sentbank.py` 生成）：
//! ```text
//! "TSSB" | ver u8 | flags u8 | rsv u16 | sent_count u32 | gram_count u32
//! sent_idx:  count × (zh_off u32, zh_len u16, en_off u32, en_len u16)
//! gram_idx:  gram_count × (g_off u32, g_len u16, first_posting u32, n_posting u16)
//! postings:  u32 句 id 数组（按 gram 顺序连续存放）
//! zh_blob | en_blob | gram_blob
//! ```
//! 检索：整句精确（二分，sent_idx 按 zh 排序）+ 输入双字集合与句子的重叠打分（倒排）。

use std::sync::{Mutex, OnceLock};

const MAGIC: &[u8; 4] = b"TSSB";
const HEADER: usize = 16;
const SENT_REC: usize = 12;
const GRAM_REC: usize = 12;
const MAX_POSTINGS_PER_GRAM: usize = 60;

struct Bank {
    data: Vec<u8>,
    sent_count: usize,
    gram_count: usize,
    sent_idx: usize,
    gram_idx: usize,
    postings: usize,
    zh_blob: usize,
    en_blob: usize,
    gram_blob: usize,
}

fn slot() -> &'static Mutex<Option<Bank>> {
    static B: OnceLock<Mutex<Option<Bank>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(None))
}

fn u16_at(d: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([d[p], d[p + 1]])
}

fn u32_at(d: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

pub fn load_bytes(data: Vec<u8>) -> usize {
    if data.len() < HEADER || &data[0..4] != MAGIC {
        return 0;
    }
    let sent_count = u32_at(&data, 8) as usize;
    let gram_count = u32_at(&data, 12) as usize;
    let sent_idx = HEADER;
    let gram_idx = sent_idx + sent_count * SENT_REC;
    let postings = gram_idx + gram_count * GRAM_REC;
    let postings_len = gram_count * MAX_POSTINGS_PER_GRAM * 4;
    let zh_blob = postings + postings_len;
    // zh_blob 长度由 sent_idx 推得
    let mut zh_len = 0usize;
    for i in 0..sent_count {
        let p = sent_idx + i * SENT_REC;
        zh_len = zh_len.max(u32_at(&data, p) as usize + u16_at(&data, p + 4) as usize);
    }
    let en_blob = zh_blob + zh_len;
    let mut en_len = 0usize;
    for i in 0..sent_count {
        let p = sent_idx + i * SENT_REC;
        en_len = en_len.max(u32_at(&data, p + 6) as usize + u16_at(&data, p + 10) as usize);
    }
    let gram_blob = en_blob + en_len;
    let mut g_len = 0usize;
    for i in 0..gram_count {
        let p = gram_idx + i * GRAM_REC;
        g_len = g_len.max(u32_at(&data, p) as usize + u16_at(&data, p + 4) as usize);
    }
    if gram_blob + g_len > data.len() {
        return 0;
    }
    // 上面三轮长度扫描都按索引读 `data`（u16_at/u32_at 是直接下标），
    // 文件被截断就会 panic —— 这个函数从 FFI 的 initStorage/importBackup 能走到，
    // 必须先确认每张表的区间都在缓冲区内（biglex.rs / gramidx.rs 也是这么守的）。
    let zh_end = zh_blob.saturating_add(zh_len);
    let en_end = en_blob.saturating_add(en_len);
    let gram_end = gram_blob.saturating_add(g_len);
    if sent_idx.saturating_add(sent_count.saturating_mul(SENT_REC)) > data.len()
        || gram_idx.saturating_add(gram_count.saturating_mul(GRAM_REC)) > data.len()
        || postings.saturating_add(postings_len) > data.len()
        || zh_blob > data.len()
        || zh_end > en_blob
        || en_blob > data.len()
        || en_end > gram_blob
        || gram_blob > data.len()
        || gram_end > data.len()
    {
        return 0;
    }
    let bank = Bank {
        data,
        sent_count,
        gram_count,
        sent_idx,
        gram_idx,
        postings,
        zh_blob,
        en_blob,
        gram_blob,
    };
    let n = bank.sent_count;
    {
        let mut g = slot().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(bank);
    }
    n
}

pub fn load(path: &str) -> usize {
    match std::fs::read(path) {
        Ok(d) => load_bytes(d),
        Err(_) => 0,
    }
}

pub fn size() -> usize {
    let g = slot().lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().map(|b| b.sent_count).unwrap_or(0)
}

fn bigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::new();
    for w in chars.windows(2) {
        let g: String = w.iter().collect();
        if !out.contains(&g) {
            out.push(g);
        }
    }
    out
}

impl Bank {
    fn zh(&self, i: usize) -> Option<&str> {
        let p = self.sent_idx + i * SENT_REC;
        let off = u32_at(&self.data, p) as usize;
        let len = u16_at(&self.data, p + 4) as usize;
        std::str::from_utf8(
            self.data
                .get(self.zh_blob + off..self.zh_blob + off + len)?,
        )
        .ok()
    }

    fn en(&self, i: usize) -> Option<&str> {
        let p = self.sent_idx + i * SENT_REC;
        let off = u32_at(&self.data, p + 6) as usize;
        let len = u16_at(&self.data, p + 10) as usize;
        std::str::from_utf8(
            self.data
                .get(self.en_blob + off..self.en_blob + off + len)?,
        )
        .ok()
    }

    /// 整句精确（sent_idx 按 zh 排序）
    fn exact(&self, zh: &str) -> Option<usize> {
        let key = zh.as_bytes();
        let (mut lo, mut hi) = (0usize, self.sent_count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.zh(mid) {
                Some(v) if v.as_bytes() < key => lo = mid + 1,
                Some(_) => hi = mid,
                None => lo = mid + 1,
            }
        }
        if lo < self.sent_count && self.zh(lo) == Some(zh) {
            Some(lo)
        } else {
            None
        }
    }

    fn gram_postings(&self, gram: &str) -> &[u8] {
        let key = gram.as_bytes();
        let (mut lo, mut hi) = (0usize, self.gram_count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let p = self.gram_idx + mid * GRAM_REC;
            let off = u32_at(&self.data, p) as usize;
            let len = u16_at(&self.data, p + 4) as usize;
            let g = self
                .data
                .get(self.gram_blob + off..self.gram_blob + off + len)
                .unwrap_or(&[]);
            if g < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo >= self.gram_count {
            return &[];
        }
        let p = self.gram_idx + lo * GRAM_REC;
        let off = u32_at(&self.data, p) as usize;
        let len = u16_at(&self.data, p + 4) as usize;
        let g = self
            .data
            .get(self.gram_blob + off..self.gram_blob + off + len)
            .unwrap_or(&[]);
        if g != key {
            return &[];
        }
        let first = u32_at(&self.data, p + 6) as usize;
        let n = u16_at(&self.data, p + 10) as usize;
        let start = self.postings + first * 4;
        self.data.get(start..start + n * 4).unwrap_or(&[])
    }
}

/// 整句 + 重叠打分检索，返回 (英文, 分数)，分数越高越贴切。
pub fn lookup(zh: &str, limit: usize) -> Vec<(String, u32)> {
    let text = zh.trim();
    if text.is_empty() || limit == 0 {
        return Vec::new();
    }
    let g = slot().lock().unwrap_or_else(|e| e.into_inner());
    let Some(b) = g.as_ref() else {
        return Vec::new();
    };

    let mut out: Vec<(String, u32)> = Vec::new();
    // 1) 整句精确
    if let Some(i) = b.exact(text) {
        if let Some(en) = b.en(i) {
            out.push((en.to_string(), 1000));
        }
    }
    // 2) 双字重叠打分（倒排）
    let grams = bigrams(text);
    if !grams.is_empty() {
        let mut score: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for gram in &grams {
            let postings = b.gram_postings(gram);
            // 用下标遍历而不是 chunks/chunks_exact：避免新旧 clippy 对 as_chunks 的互斥建议
            #[allow(clippy::needless_range_loop)]
            for k in 0..postings.len() / 4 {
                let b = k * 4;
                let id = u32::from_le_bytes([
                    postings[b],
                    postings[b + 1],
                    postings[b + 2],
                    postings[b + 3],
                ]);
                *score.entry(id).or_insert(0) += 1;
            }
        }
        let total = grams.len() as u32;
        let mut ranked: Vec<(u32, u32)> = score
            .into_iter()
            .filter(|(_, s)| *s * 100 / total.max(1) >= 60) // 覆盖率闸门：至少 60% 双字命中
            .collect();
        ranked.sort_by_key(|x| (std::cmp::Reverse(x.1), x.0));
        for (id, s) in ranked.into_iter().take(limit) {
            if let Some(en) = b.en(id as usize) {
                if !out.iter().any(|(x, _)| x == en) {
                    out.push((en.to_string(), s));
                }
            }
        }
    }
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut sorted: Vec<(&str, &str)> = pairs.to_vec();
        sorted.sort_by_key(|x| x.0);
        let mut zh = Vec::new();
        let mut en = Vec::new();
        let mut idx = Vec::new();
        for (z, e) in &sorted {
            let zo = zh.len() as u32;
            zh.extend_from_slice(z.as_bytes());
            let eo = en.len() as u32;
            en.extend_from_slice(e.as_bytes());
            idx.extend_from_slice(&zo.to_le_bytes());
            idx.extend_from_slice(&(z.len() as u16).to_le_bytes());
            idx.extend_from_slice(&eo.to_le_bytes());
            idx.extend_from_slice(&(e.len() as u16).to_le_bytes());
        }
        // 双字倒排
        let mut map: std::collections::BTreeMap<String, Vec<u32>> =
            std::collections::BTreeMap::new();
        for (i, (z, _)) in sorted.iter().enumerate() {
            for g in bigrams(z) {
                map.entry(g).or_default().push(i as u32);
            }
        }
        let mut gblob = Vec::new();
        let mut gidx = Vec::new();
        let mut postings = Vec::new();
        for (g, ids) in &map {
            let go = gblob.len() as u32;
            gblob.extend_from_slice(g.as_bytes());
            let first = postings.len() as u32;
            let n = ids.len().min(MAX_POSTINGS_PER_GRAM);
            for id in ids.iter().take(n) {
                postings.extend_from_slice(&id.to_le_bytes());
            }
            gidx.extend_from_slice(&go.to_le_bytes());
            gidx.extend_from_slice(&(g.len() as u16).to_le_bytes());
            gidx.extend_from_slice(&first.to_le_bytes());
            gidx.extend_from_slice(&(n as u16).to_le_bytes());
        }
        postings.resize(map.len() * MAX_POSTINGS_PER_GRAM * 4, 0);
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
        out.extend_from_slice(&(map.len() as u32).to_le_bytes());
        out.extend_from_slice(&idx);
        out.extend_from_slice(&gidx);
        out.extend_from_slice(&postings);
        out.extend_from_slice(&zh);
        out.extend_from_slice(&en);
        out.extend_from_slice(&gblob);
        out
    }

    #[test]
    fn exact_and_overlap() {
        let data = build(&[
            ("今天天气很好", "The weather is nice today."),
            ("今天很忙", "I'm busy today."),
            ("你好", "Hello!"),
        ]);
        assert_eq!(load_bytes(data), 3);
        assert_eq!(lookup("你好", 3)[0].0, "Hello!");
        // 非整句：双字重叠命中
        let r = lookup("今天天气", 3);
        assert!(r.iter().any(|(e, _)| e.contains("weather")), "got {r:?}");
        assert!(lookup("", 3).is_empty());
    }

    #[test]
    fn bad_input_safe() {
        assert_eq!(load_bytes(Vec::new()), 0);
        assert_eq!(load_bytes(b"TSSB".to_vec()), 0);
    }
}
