package com.typesake.app

import android.content.Context

/** 输入法设置（SharedPreferences）。 */
class TypesakePrefs(context: Context) {

    private val sp = context.getSharedPreferences("typesake_prefs", Context.MODE_PRIVATE)

    /** 0=跟随系统 1=浅色 2=深色 */
    var themeMode: Int
        get() = sp.getInt(KEY_THEME, 0)
        set(v) = sp.edit().putInt(KEY_THEME, v.coerceIn(0, 2)).apply()

    /** 按键高度（dp），影响整个键盘高度 */
    var keyHeightDp: Int
        get() = sp.getInt(KEY_KEY_HEIGHT, 46)
        set(v) = sp.edit().putInt(KEY_KEY_HEIGHT, v.coerceIn(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT)).apply()

    var haptics: Boolean
        get() = sp.getBoolean(KEY_HAPTICS, true)
        set(v) = sp.edit().putBoolean(KEY_HAPTICS, v).apply()

    var sound: Boolean
        get() = sp.getBoolean(KEY_SOUND, false)
        set(v) = sp.edit().putBoolean(KEY_SOUND, v).apply()

    var keyPreview: Boolean
        get() = sp.getBoolean(KEY_PREVIEW, true)
        set(v) = sp.edit().putBoolean(KEY_PREVIEW, v).apply()

    /** 单手模式 0=关 1=左手 2=右手 */
    var oneHand: Int
        get() = sp.getInt(KEY_ONE_HAND, 0)
        set(v) = sp.edit().putInt(KEY_ONE_HAND, v.coerceIn(0, 2)).apply()

    /** 配色（0 珊瑚/默认玻璃皮肤 1 翠绿 2 暖阳 3 玫瑰 4 海洋） */
    var palette: Int
        get() = sp.getInt(KEY_PALETTE, 0)
        set(v) = sp.edit().putInt(KEY_PALETTE, v.coerceIn(0, 4)).apply()

    /** 模糊音（z/zh、n/l、an/ang…） */
    var fuzzy: Boolean
        get() = sp.getBoolean(KEY_FUZZY, true)
        set(v) = sp.edit().putBoolean(KEY_FUZZY, v).apply()

    /** 击键纠错（邻键/漏键/多键/换位） */
    var correction: Boolean
        get() = sp.getBoolean(KEY_CORRECTION, true)
        set(v) = sp.edit().putBoolean(KEY_CORRECTION, v).apply()

    /** 输入方案（0 全拼 1 小鹤 … 8 大千注音，可用清单见 `TypesakeCore.schemes()`） */
    var shuangpin: Int
        get() = sp.getInt(KEY_SHUANGPIN, 0)
        set(v) = sp.edit().putInt(KEY_SHUANGPIN, v.coerceIn(0, MAX_SCHEME)).apply()

    /** 候选旁逐词译词 */
    var gloss: Boolean
        get() = sp.getBoolean(KEY_GLOSS, true)
        set(v) = sp.edit().putBoolean(KEY_GLOSS, v).apply()

    /** 生词橙标（该词还从没上屏过时标出来） */
    var freshMark: Boolean
        get() = sp.getBoolean(KEY_FRESH, true)
        set(v) = sp.edit().putBoolean(KEY_FRESH, v).apply()

    /** 快捷输入（v 算式 / i 中文数字 / u 码点） */
    var shortcut: Boolean
        get() = sp.getBoolean(KEY_SHORTCUT, true)
        set(v) = sp.edit().putBoolean(KEY_SHORTCUT, v).apply()

    /** 英文模式拼写纠正 */
    var englishFix: Boolean
        get() = sp.getBoolean(KEY_EN_FIX, true)
        set(v) = sp.edit().putBoolean(KEY_EN_FIX, v).apply()

    /** 这些应用里不给英文候选（换行分隔的包名；空 = 所有应用都给） */
    var blockedApps: String
        get() = sp.getString(KEY_BLOCK_APPS, "") ?: ""
        set(v) = sp.edit().putString(KEY_BLOCK_APPS, v).apply()

    /** 当前应用是否被「不给英文候选」拦住了 */
    fun englishBlockedFor(pkg: String?): Boolean {
        if (pkg.isNullOrEmpty()) return false
        return blockedApps.lineSequence().any { it.trim() == pkg }
    }

    /** 键盘布局（false 全键盘 / true 九键） */
    var t9Layout: Boolean
        get() = sp.getBoolean(KEY_T9, false)
        set(v) = sp.edit().putBoolean(KEY_T9, v).apply()

    /** 输出字形（0 简体 1 繁体） */
    var script: Int
        get() = sp.getInt(KEY_SCRIPT, 0)
        set(v) = sp.edit().putInt(KEY_SCRIPT, v.coerceIn(0, 1)).apply()

    /** 自定义符号页第一行（空 = 用默认） */
    var customSymbols: String
        get() = sp.getString(KEY_SYMBOLS, "") ?: ""
        set(v) = sp.edit().putString(KEY_SYMBOLS, v.take(40)).apply()

    /** 候选竖排 */
    var verticalCandidates: Boolean
        get() = sp.getBoolean(KEY_VERTICAL, false)
        set(v) = sp.edit().putBoolean(KEY_VERTICAL, v).apply()

    /** 翻页键（0 逗号句号=二三候选 1 逗号句号=翻页） */
    var pageKeys: Int
        get() = sp.getInt(KEY_PAGE_KEYS, 0)
        set(v) = sp.edit().putInt(KEY_PAGE_KEYS, v.coerceIn(0, 1)).apply()

    /** 记住上次的中/英状态 */
    var englishMode: Boolean
        get() = sp.getBoolean(KEY_EN, false)
        set(v) = sp.edit().putBoolean(KEY_EN, v).apply()

    /** 英文上屏后自动补一个空格 */
    var englishAutoSpace: Boolean
        get() = sp.getBoolean(KEY_EN_SPACE, true)
        set(v) = sp.edit().putBoolean(KEY_EN_SPACE, v).apply()

    /** 单手键盘档位（百分比，75/80/85） */
    var oneHandScale: Int
        get() = sp.getInt(KEY_ONE_SCALE, 80)
        set(v) = sp.edit().putInt(KEY_ONE_SCALE, v.coerceIn(70, 90)).apply()

    /** 隐藏数字行（靠上滑出数字/符号） */
    var hideNumberRow: Boolean
        get() = sp.getBoolean(KEY_HIDE_NUM, false)
        set(v) = sp.edit().putBoolean(KEY_HIDE_NUM, v).apply()

    /** 候选是否显示 1/2/3 序号 */
    var showCandidateIndex: Boolean
        get() = sp.getBoolean(KEY_CAND_INDEX, true)
        set(v) = sp.edit().putBoolean(KEY_CAND_INDEX, v).apply()

    /** 震动强度（0 关 1 弱 2 中 3 强） */
    var hapticLevel: Int
        get() = sp.getInt(KEY_HAPTIC_LEVEL, 1)
        set(v) = sp.edit().putInt(KEY_HAPTIC_LEVEL, v.coerceIn(0, 3)).apply()

    /** 跟随壁纸取色（Android 12+） */
    var dynamicColor: Boolean
        get() = sp.getBoolean(KEY_DYNAMIC, false)
        set(v) = sp.edit().putBoolean(KEY_DYNAMIC, v).apply()

    /** 剪贴板历史（仅本次会话内存，不落盘） */
    var clipboardHistory: Boolean
        get() = sp.getBoolean(KEY_CLIPBOARD, true)
        set(v) = sp.edit().putBoolean(KEY_CLIPBOARD, v).apply()

    /** 让输入法能监听设置变化（改完立即生效，无需重新聚焦输入框） */
    fun registerListener(l: android.content.SharedPreferences.OnSharedPreferenceChangeListener) =
        sp.registerOnSharedPreferenceChangeListener(l)

    fun unregisterListener(l: android.content.SharedPreferences.OnSharedPreferenceChangeListener) =
        sp.unregisterOnSharedPreferenceChangeListener(l)

    companion object {
        const val MIN_KEY_HEIGHT = 34
        const val MAX_KEY_HEIGHT = 62

        /** 方案编号上限（0 全拼 … 8 大千注音），与 Rust `shuangpin::schemes()` 对齐 */
        const val MAX_SCHEME = 8

        /** 大千注音：击键里含 `,` `.` `;` `/` `-` 与 0-9，与选词/翻页键冲突，要单独处理 */
        const val SCHEME_ZHUYIN = 8

        /** 旧版本可能存过越界值 -> 读取时兜底 */
        fun sanitizeKeyHeight(v: Int): Int = v.coerceIn(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT)

        private const val KEY_THEME = "theme_mode"
        private const val KEY_KEY_HEIGHT = "key_height_dp"
        private const val KEY_HAPTICS = "haptics"
        private const val KEY_SOUND = "sound"
        private const val KEY_PREVIEW = "key_preview"
        private const val KEY_ONE_HAND = "one_hand"
        private const val KEY_CLIPBOARD = "clipboard_history"
        private const val KEY_PALETTE = "palette"
        private const val KEY_FUZZY = "fuzzy"
        private const val KEY_CORRECTION = "correction"
        private const val KEY_SHUANGPIN = "shuangpin"
        private const val KEY_T9 = "t9_layout"
        private const val KEY_SCRIPT = "script"
        private const val KEY_SYMBOLS = "custom_symbols"
        private const val KEY_VERTICAL = "vertical_candidates"
        private const val KEY_PAGE_KEYS = "page_keys"
        private const val KEY_EN = "english_mode"
        private const val KEY_EN_SPACE = "english_auto_space"
        private const val KEY_ONE_SCALE = "one_hand_scale"
        private const val KEY_HIDE_NUM = "hide_number_row"
        private const val KEY_CAND_INDEX = "candidate_index"
        private const val KEY_HAPTIC_LEVEL = "haptic_level"
        private const val KEY_DYNAMIC = "dynamic_color"
        private const val KEY_GLOSS = "gloss_inline"
        private const val KEY_FRESH = "fresh_mark"
        private const val KEY_SHORTCUT = "shortcut_input"
        private const val KEY_EN_FIX = "english_fix"
        private const val KEY_BLOCK_APPS = "blocked_english_apps"
    }
}
