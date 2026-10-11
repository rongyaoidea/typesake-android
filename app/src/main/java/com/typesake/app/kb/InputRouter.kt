package com.typesake.app.kb

/**
 * 输入路由纯函数（无 Android 依赖，可单测）。
 *
 * 从 `TypesakeImeService` 提取：快捷输入形状守卫 + 注音键分流。
 * Service 只做状态机与 InputConnection，判定逻辑全在这里（补单测见 InputRouterTest）。
 */
object InputRouter {

    /** 第一键后缀能不能坐实快捷输入形状（`im` 单独在 onLetter 里放行）。 */
    fun entersShortcut(prefix: Char, next: Char): Boolean = when (prefix) {
        'v' -> next.isDigit() || next == '(' || next == '-' || next == '.'
        'i' -> next.isDigit() || next == '-' || next == '.'
        'u' -> next.isDigit()
        else -> false
    }

    /** 整条缓冲是否仍然是某个快捷输入的合法前缀；不是就退出、交还给拼音。 */
    fun shortcutShapeOk(s: String): Boolean {
        if (s.isEmpty() || s[0] !in "viu") return false
        val rest = s.substring(1)
        if (rest.isEmpty()) return true
        return when (s[0]) {
            'v' -> rest.all { it.isDigit() || it in "()+-*/.^%" }
            'u' -> rest.all { it.isLetterOrDigit() }
            else -> if (s.startsWith("im")) s.substring(2).all { it.isDigit() || it == '.' }
            else rest.all { it.isDigit() || it == '-' || it == '.' }
        }
    }

    /**
     * 大千注音键分流（纯判定，不碰 prefs/Service 状态）。
     *
     * @param scheme 当前方案 id（`TypesakePrefs.SCHEME_ZHUYIN` 才走注音）
     * @param zhuyinScheme 注音方案 id（调用方传入，避免 kb 包依赖 prefs 常量）
     */
    fun zhuyinInput(
        text: String,
        scheme: Int,
        zhuyinScheme: Int,
        t9Mode: Boolean,
        englishMode: Boolean,
        privateField: Boolean,
        composing: Boolean,
        lettersLayer: Boolean,
    ): Boolean {
        if (scheme != zhuyinScheme) return false
        if (t9Mode || englishMode || privateField) return false
        if (text.length != 1) return false
        val ch = text[0]
        if (ch.isDigit() || ch == '-') return true
        if (ch !in ",.;:/") return false
        if (composing) return true
        return lettersLayer
    }
}
