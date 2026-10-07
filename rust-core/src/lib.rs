//! Typesake Android 离线语言引擎（Rust）。
//!
//! 架构：
//! - [`engine`]：拼音->中文候选（精确 / 整句 Viterbi / 前缀补全）+ 联想预测 + 词频学习（L0）
//! - [`english`]：中文->英文表达（词典贪心匹配 + 覆盖度阈值，永不吐垃圾）
//! - [`store`]：收藏与学习快照的原子持久化
//! - [`ffi`]：JNI 边界（全部 catch_unwind 包裹，panic 不跨 FFI）
//!
//! 全离线、零网络权限。热路径返回 `\u{1F}` 分隔串而非 JSON，避免每键解析开销。

pub mod biglex;
pub mod engine;
pub mod english;
pub mod ffi;
pub mod gloss;
pub mod gramidx;
pub mod initials;
pub mod mixed;
pub mod s2t;
pub mod sentbank;
pub mod shortcut;
pub mod shuangpin;
pub mod store;
pub mod t9;
pub mod userdic;

/// 测试用的全局串行锁：多个模块的用例都会动全局引擎状态（选项/学习/删词），
/// 并行跑会互相污染，统一用这把锁串行化。
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    L.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// 热路径列表分隔符（Unit Separator），比 JSON 便宜且无转义问题。
pub const DELIM: char = '\u{1F}';
/// `join` 的分隔符：预先建好，别在每次返回候选时都 `DELIM.to_string()` 分配一次。
pub const DELIM_STR: &str = "\u{1F}";

/// 把字符串列表拼成分隔串（空列表 -> 空串）。
pub fn join_delim(items: &[String]) -> String {
    items.join(DELIM_STR)
}

/// 拆分分隔串（空串 -> 空列表）。
pub fn split_delim(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(DELIM).map(|x| x.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delim_round_trip() {
        let items = vec!["你好".to_string(), "Hello".to_string()];
        assert_eq!(split_delim(&join_delim(&items)), items);
        assert!(split_delim("").is_empty());
        assert_eq!(join_delim(&[]), "");
    }
}
