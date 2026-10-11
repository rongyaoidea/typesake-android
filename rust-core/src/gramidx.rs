//! 声母串索引（由 lexgen 从带拼音的大词库生成）：`jt` → 今天。
//!
//! 这是混简拼的关键数据源：拼音词典里「今天」不是词条（只有单字），
//! 只有带拼音的 lex 词库才有多音节词，因此简拼整词必须从这里取。
//!
//! 文件格式（`gram.bin`）：
//! ```text
//! "TSGR" | ver u8 | rsv[3] | count u32
//! idx: count × (g_off u32, g_len u16, v_off u32, v_len u16)   # 按 gram 升序
//! gram_blob | val_blob（值 = 词，多个词用 '\u{1F}' 分隔）
//! ```

use std::sync::{Mutex, OnceLock};

const MAGIC: &[u8; 4] = b"TSGR";
const HEADER: usize = 12;
const REC: usize = 12;
const SEP: char = '\u{1F}';

struct Idx {
    data: Vec<u8>,
    count: usize,
    gram_base: usize,
    val_base: usize,
}

fn slot() -> &'static Mutex<Option<Idx>> {
    static S: OnceLock<Mutex<Option<Idx>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
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
    let count = u32_at(&data, 8) as usize;
    let index_end = HEADER.saturating_add(count.saturating_mul(REC));
    if index_end > data.len() || index_end < HEADER {
        return 0;
    }
    let mut gram_len = 0usize;
    let mut val_len = 0usize;
    for i in 0..count {
        let p = HEADER + i * REC;
        gram_len =
            gram_len.max((u32_at(&data, p) as usize).saturating_add(u16_at(&data, p + 4) as usize));
        val_len = val_len
            .max((u32_at(&data, p + 6) as usize).saturating_add(u16_at(&data, p + 10) as usize));
    }
    let gram_base = index_end;
    let val_base = gram_base.saturating_add(gram_len);
    if val_base.saturating_add(val_len) > data.len() {
        return 0;
    }
    // 逐条校验：gram 落在 gram 区，val 落在 val 区。任一条越界即拒装。
    let gram_end = gram_base.saturating_add(gram_len);
    let val_end = val_base.saturating_add(val_len);
    for i in 0..count {
        let p = HEADER + i * REC;
        let g_off = u32_at(&data, p) as usize;
        let g_len = u16_at(&data, p + 4) as usize;
        let v_off = u32_at(&data, p + 6) as usize;
        let v_len = u16_at(&data, p + 10) as usize;
        let gs = gram_base.saturating_add(g_off);
        let ge = gs.saturating_add(g_len);
        let vs = val_base.saturating_add(v_off);
        let ve = vs.saturating_add(v_len);
        if ge > gram_end || vs < val_base || ve > val_end || ge < gs || ve < vs {
            return 0;
        }
    }
    let idx = Idx {
        data,
        count,
        gram_base,
        val_base,
    };
    let n = idx.count;
    {
        let mut g = slot().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(idx);
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
    g.as_ref().map(|i| i.count).unwrap_or(0)
}

impl Idx {
    fn gram(&self, i: usize) -> &[u8] {
        let p = HEADER + i * REC;
        let off = u32_at(&self.data, p) as usize;
        let len = u16_at(&self.data, p + 4) as usize;
        let s = self.gram_base.saturating_add(off);
        self.data.get(s..s.saturating_add(len)).unwrap_or(&[])
    }

    fn val(&self, i: usize) -> &str {
        let p = HEADER + i * REC;
        let off = u32_at(&self.data, p + 6) as usize;
        let len = u16_at(&self.data, p + 10) as usize;
        let s = self.val_base.saturating_add(off);
        let e = s.saturating_add(len);
        std::str::from_utf8(self.data.get(s..e).unwrap_or(&[])).unwrap_or("")
    }

    fn lower_bound(&self, key: &[u8]) -> usize {
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.gram(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

/// 声母串 -> 词列表（按词频降序，由生成器排好）
pub fn lookup(gram: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    if gram.is_empty() || limit == 0 {
        return out;
    }
    let g = slot().lock().unwrap_or_else(|e| e.into_inner());
    let Some(i) = g.as_ref() else { return out };
    let key = gram.as_bytes();
    let at = i.lower_bound(key);
    if at >= i.count || i.gram(at) != key {
        return out;
    }
    for w in i.val(at).split(SEP) {
        if !w.is_empty() && !out.iter().any(|x: &String| x == w) {
            out.push(w.to_string());
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut sorted: Vec<(&str, &str)> = entries.to_vec();
        sorted.sort_by_key(|x| x.0);
        let (mut gb, mut vb, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        for (g, v) in &sorted {
            let go = gb.len() as u32;
            gb.extend_from_slice(g.as_bytes());
            let vo = vb.len() as u32;
            vb.extend_from_slice(v.as_bytes());
            idx.extend_from_slice(&go.to_le_bytes());
            idx.extend_from_slice(&(g.len() as u16).to_le_bytes());
            idx.extend_from_slice(&vo.to_le_bytes());
            idx.extend_from_slice(&(v.len() as u16).to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.extend_from_slice(&[0u8; 3]);
        out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
        out.extend_from_slice(&idx);
        out.extend_from_slice(&gb);
        out.extend_from_slice(&vb);
        out
    }

    #[test]
    fn lookup_works_and_is_safe() {
        let data = build(&[
            ("jt", "今天\u{1F}交通"),
            ("wjt", "无法"),
            ("gongsi", "公司"),
        ]);
        assert_eq!(load_bytes(data), 3);
        assert_eq!(
            lookup("jt", 5),
            vec!["今天".to_string(), "交通".to_string()]
        );
        assert!(lookup("zzz", 5).is_empty());
        assert!(lookup("", 5).is_empty());
        assert_eq!(load_bytes(Vec::new()), 0);
    }
}
