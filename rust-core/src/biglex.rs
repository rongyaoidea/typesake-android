//! 大词库（可选）：把开源词库（jieba/Rime 等）离线生成的紧凑二进制读进内存，
//! 给候选做"精确 + 前缀"补充。格式由 `src/bin/lexgen.rs` 生成：
//!
//! ```text
//! magic "TSLX" | ver u8 | flags u8 | reserved u16 | count u32
//! index: count × (key_off u32, val_off u32, key_len u16, val_len u16)
//! key_blob (pinyin, 无分隔) | val_blob (词, 无分隔)
//! ```
//! 索引按 key 排序，运行时二分查找；只保留一份文件字节 + 索引切片，不逐条分配 String。

use std::sync::{Mutex, OnceLock};

const MAGIC: &[u8; 4] = b"TSLX";
const HEADER: usize = 12;
const REC: usize = 12;

struct Lex {
    data: Vec<u8>,
    count: usize,
    key_base: usize,
    val_base: usize,
}

fn slot() -> &'static Mutex<Option<Lex>> {
    static L: OnceLock<Mutex<Option<Lex>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(None))
}

fn u16_at(d: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([d[p], d[p + 1]])
}

fn u32_at(d: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

/// 从内存载入（便于测试）。
///
/// 对不可信文件逐条校验 `(off,len)`：任一条越界 / 索引越界即拒装（返回 0），
/// 避免之后每次 `exact/prefix` 在 [`Lex::key`] 切片时 panic（持久 DoS）。
pub fn load_bytes(data: Vec<u8>) -> usize {
    if data.len() < HEADER || &data[0..4] != MAGIC {
        return 0;
    }
    let count = u32_at(&data, 8) as usize;
    let index_end = HEADER.saturating_add(count.saturating_mul(REC));
    if index_end > data.len() || index_end < HEADER {
        return 0;
    }
    // 两个 blob 的起点：键区紧接索引，值区起点 = 键区末尾（由记录里的最大 key_off+len 推导）
    let key_base = index_end;
    let mut key_end = key_base;
    for i in 0..count {
        let p = HEADER + i * REC;
        let off = u32_at(&data, p) as usize;
        let len = u16_at(&data, p + 8) as usize;
        key_end = key_end.max(key_base.saturating_add(off).saturating_add(len));
    }
    if key_end > data.len() {
        return 0;
    }
    let val_base = key_end;
    // 逐条校验：key 落在键区内，val 落在文件内（含 UTF-8 由调用方懒校验）。
    for i in 0..count {
        let p = HEADER + i * REC;
        let k_off = u32_at(&data, p) as usize;
        let k_len = u16_at(&data, p + 8) as usize;
        let v_off = u32_at(&data, p + 4) as usize;
        let v_len = u16_at(&data, p + 10) as usize;
        let ks = key_base.saturating_add(k_off);
        let ke = ks.saturating_add(k_len);
        let vs = val_base.saturating_add(v_off);
        let ve = vs.saturating_add(v_len);
        if ke > key_end || vs < val_base || ve > data.len() || ke < ks || ve < vs {
            return 0;
        }
    }
    let lex = Lex {
        val_base: key_end,
        key_base,
        count,
        data,
    };
    let n = lex.count;
    {
        let mut g = slot().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(lex);
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
    g.as_ref().map(|l| l.count).unwrap_or(0)
}

impl Lex {
    fn key(&self, i: usize) -> &[u8] {
        let p = HEADER + i * REC;
        // load 时已逐条校验，这里用 `get` 兜底：坏数据最多 miss，绝不 panic。
        let off = u32_at(&self.data, p) as usize;
        let len = u16_at(&self.data, p + 8) as usize;
        let s = self.key_base.saturating_add(off);
        self.data.get(s..s.saturating_add(len)).unwrap_or(&[])
    }

    fn val(&self, i: usize) -> Option<&str> {
        let p = HEADER + i * REC;
        let off = u32_at(&self.data, p + 4) as usize;
        let len = u16_at(&self.data, p + 10) as usize;
        let s = self.val_base + off;
        std::str::from_utf8(self.data.get(s..s + len)?).ok()
    }

    /// 首个 >= key 的索引
    fn lower_bound(&self, key: &[u8]) -> usize {
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.key(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

/// 精确命中（同一拼音可能对应多个词）。
pub fn exact(pinyin: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let g = slot().lock().unwrap_or_else(|e| e.into_inner());
    let Some(l) = g.as_ref() else { return out };
    let key = pinyin.as_bytes();
    let mut i = l.lower_bound(key);
    while i < l.count && out.len() < limit {
        if l.key(i) != key {
            break;
        }
        if let Some(v) = l.val(i) {
            if !out.iter().any(|x: &String| x == v) {
                out.push(v.to_string());
            }
        }
        i += 1;
    }
    out
}

/// 前缀补全（输入未打完时给整词）。
pub fn prefix(pinyin: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    if pinyin.len() < 3 {
        return out;
    }
    let g = slot().lock().unwrap_or_else(|e| e.into_inner());
    let Some(l) = g.as_ref() else { return out };
    let key = pinyin.as_bytes();
    let mut i = l.lower_bound(key);
    let mut scanned = 0usize;
    while i < l.count && out.len() < limit && scanned < 400 {
        let k = l.key(i);
        if !k.starts_with(key) {
            break;
        }
        if let Some(v) = l.val(i) {
            if !out.iter().any(|x: &String| x == v) {
                out.push(v.to_string());
            }
        }
        i += 1;
        scanned += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut keys: Vec<(&str, &str)> = entries.to_vec();
        keys.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(b.1)));
        let mut kb = Vec::new();
        let mut vb = Vec::new();
        let mut idx = Vec::new();
        for (k, v) in &keys {
            let ko = kb.len() as u32;
            kb.extend_from_slice(k.as_bytes());
            let vo = vb.len() as u32;
            vb.extend_from_slice(v.as_bytes());
            idx.extend_from_slice(&ko.to_le_bytes());
            idx.extend_from_slice(&vo.to_le_bytes());
            idx.extend_from_slice(&(k.len() as u16).to_le_bytes());
            idx.extend_from_slice(&(v.len() as u16).to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        out.extend_from_slice(&idx);
        out.extend_from_slice(&kb);
        out.extend_from_slice(&vb);
        out
    }

    #[test]
    fn exact_and_prefix_work() {
        let data = build(&[
            ("zhongguo", "中国"),
            ("zhongguoren", "中国人"),
            ("zhongwen", "中文"),
            ("nihao", "你好"),
        ]);
        assert_eq!(load_bytes(data), 4);
        assert_eq!(exact("nihao", 5), vec!["你好".to_string()]);
        let pre = prefix("zhongguo", 5);
        assert!(pre.contains(&"中国".to_string()) && pre.contains(&"中国人".to_string()));
        assert!(exact("", 5).is_empty());
        assert!(prefix("ni", 5).is_empty()); // 太短不查前缀
    }

    #[test]
    fn bad_input_is_safe() {
        assert_eq!(load_bytes(Vec::new()), 0);
        assert_eq!(load_bytes(b"NOPE".to_vec()), 0);
        assert!(exact("x", 3).is_empty());
    }
}
