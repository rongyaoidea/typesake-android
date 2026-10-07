package com.typesake.app.kb

/**
 * 键盘配色（纯 int，便于单测）。
 *
 * 默认皮肤 = **珊瑚橙 + 奶油色 + 玻璃**（Claude 风格）：
 * 奶油底、珊瑚强调色、半透明玻璃键帽与带描边的浮起面板。
 * 其余 4 套为可选配色（翠绿/暖阳/玫瑰/海洋）。
 */
data class KbColors(
    val bg: Int,
    val key: Int,
    val keyPressed: Int,
    val keyText: Int,
    val actionKey: Int,
    val actionText: Int,
    val accent: Int,
    val accentText: Int,
    val barBg: Int,
    val barText: Int,
    val hint: Int,
    /** 玻璃皮肤的键帽/面板描边色（0 表示不使用） */
    val glassBorder: Int = 0,
    /** 是否玻璃皮肤（服务据此开启系统级背景模糊） */
    val glass: Boolean = false,
)

enum class KbPalette { CORAL, GREEN, SUNSET, ROSE, OCEAN }

object KbThemes {

    const val THEME_SYSTEM = 0
    const val THEME_LIGHT = 1
    const val THEME_DARK = 2

    /** 陶土橙（主强调） */
    const val CORAL = 0xFFD97757.toInt()
    /** 暖米白（背景 / 深色底上的文字） */
    const val CREAM = 0xFFFAF9F5.toInt()
    /**
     * 中灰（次要文本、分割线、弱化元素）。
     *
     * 浅色皮肤上的次要文字要 ≥4.5:1：写在浅灰键帽/奶油候选条上都过线，
     * 深色皮肤另配一个更亮的灰（见 [KbThemes.coralDark]）。
     */
    const val WARM_GRAY = 0xFF63615A.toInt()
    /** 浅色皮肤次要文字 */
    const val LIGHT_HINT = 0xFF5C5A52.toInt()
    /** 深色皮肤次要文字 */
    const val DARK_HINT = 0xFF96A3B2.toInt()
    /** 浅灰（细微背景、输入框、卡片底） */
    const val LIGHT_GRAY = 0xFFE8E6DC.toInt()
    /** 纯白（深色强调色上的文字） */
    const val WHITE = 0xFFFFFFFF.toInt()
    /** 尘蓝（信息提示/链接/次要按钮） */
    const val DUST_BLUE = 0xFF6A9BCC.toInt()
    /** 鼠尾草绿（成功/有机元素） */
    const val SAGE = 0xFF788C5D.toInt()

    fun paletteOf(id: Int): KbPalette = KbPalette.entries.getOrElse(id) { KbPalette.CORAL }

    /** 纯函数：从 Configuration.uiMode 判断夜间（UI_MODE_NIGHT_MASK=0x30, NIGHT_YES=0x20）。 */
    fun isNight(uiMode: Int): Boolean = (uiMode and 0x30) == 0x20

    // ---------------- 默认皮肤：珊瑚橙 + 奶油 + 玻璃 ----------------

    private fun coralLight(): KbColors = KbColors(
        bg = 0xB3ECEADF.toInt(),           // 暖米白玻璃
        key = 0xE6E8E6DC.toInt(),          // 浅灰键帽（输入框/卡片底同色系）
        keyPressed = 0xCCF3E1D8.toInt(),   // 按下：陶土橙浅晕
        keyText = 0xFF33312E.toInt(),
        actionKey = 0x99DCD9CC.toInt(),
        actionText = 0xFF3A3631.toInt(),
        accent = CORAL,
        accentText = accentTextOf(CORAL),
        barBg = 0xCCFAF9F5.toInt(),
        barText = 0xFF4A2C1E.toInt(),
        hint = WARM_GRAY,
        glassBorder = 0x66FFFFFF.toInt(),
        glass = true,
    )

    private fun coralDark(): KbColors = KbColors(
        bg = 0xB31F1D1A.toInt(),
        key = 0xE6332E29.toInt(),
        keyPressed = 0xCC4A4038.toInt(),
        keyText = CREAM,                   // 暖米白用于深色底上的文字
        actionKey = 0x992A2622.toInt(),
        actionText = 0xFFE8E2DA.toInt(),
        accent = CORAL,
        accentText = accentTextOf(CORAL),
        barBg = 0xCC26221F.toInt(),
        barText = 0xFFF1E4DA.toInt(),
        hint = DARK_HINT,
        glassBorder = 0x33FFFFFF.toInt(),
        glass = true,
    )

    // ---------------- 可选配色 ----------------

    private fun lightBase(p: KbPalette): KbColors {
        val (accent, barBg, barText) = when (p) {
            KbPalette.CORAL -> Triple(CORAL, 0xFFF7EDE6.toInt(), 0xFF4A2C1E.toInt())
            KbPalette.GREEN -> Triple(0xFF1B7A5A.toInt(), 0xFFE7F4EC.toInt(), 0xFF14432F.toInt())
            KbPalette.SUNSET -> Triple(0xFFB4560E.toInt(), 0xFFFDEEE0.toInt(), 0xFF5A2A06.toInt())
            KbPalette.ROSE -> Triple(0xFFB03060.toInt(), 0xFFFDECF2.toInt(), 0xFF55142E.toInt())
            KbPalette.OCEAN -> Triple(0xFF1668A8.toInt(), 0xFFE6F0FA.toInt(), 0xFF0E3C63.toInt())
        }
        return KbColors(
            bg = 0xFFD9DEE8.toInt(),
            key = 0xFFFFFFFF.toInt(),
            keyPressed = 0xFFC6CFDD.toInt(),
            keyText = 0xFF1B2430.toInt(),
            actionKey = 0xFFB9C2D0.toInt(),
            actionText = 0xFF1B2430.toInt(),
            accent = accent,
            accentText = accentTextOf(accent),
            barBg = barBg,
            barText = barText,
            hint = LIGHT_HINT,
        )
    }

    private fun darkBase(p: KbPalette): KbColors {
        val (accent, barBg, barText) = when (p) {
            KbPalette.CORAL -> Triple(CORAL, 0xFF2A221E.toInt(), 0xFFF0DED2.toInt())
            KbPalette.GREEN -> Triple(0xFF2FBF87.toInt(), 0xFF17251F.toInt(), 0xFFBDE9D5.toInt())
            KbPalette.SUNSET -> Triple(0xFFE08A45.toInt(), 0xFF2A1F14.toInt(), 0xFFF2D6B8.toInt())
            KbPalette.ROSE -> Triple(0xFFE06A96.toInt(), 0xFF2A1620.toInt(), 0xFFF5CBDA.toInt())
            KbPalette.OCEAN -> Triple(0xFF4FA3DC.toInt(), 0xFF14202B.toInt(), 0xFFC6DFF2.toInt())
        }
        return KbColors(
            bg = 0xFF12181F.toInt(),
            key = 0xFF2A333F.toInt(),
            keyPressed = 0xFF3B4756.toInt(),
            keyText = 0xFFF2F5F8.toInt(),
            actionKey = 0xFF1D2530.toInt(),
            actionText = 0xFFD7DEE7.toInt(),
            accent = accent,
            accentText = accentTextOf(accent),
            barBg = barBg,
            barText = barText,
            hint = DARK_HINT,
        )
    }

    fun resolve(palette: KbPalette, themeMode: Int, night: Boolean): KbColors {
        val dark = when (themeMode) {
            THEME_LIGHT -> false
            THEME_DARK -> true
            else -> night
        }
        return when (palette) {
            KbPalette.CORAL -> if (dark) coralDark() else coralLight()
            else -> if (dark) darkBase(palette) else lightBase(palette)
        }
    }

    fun resolve(paletteId: Int, themeMode: Int, night: Boolean): KbColors =
        resolve(paletteOf(paletteId), themeMode, night)

    /** 配色预览色（设置页色块 / Compose 主题用）。 */
    fun accentOf(palette: KbPalette, night: Boolean): Int =
        resolve(palette, KbThemes.THEME_SYSTEM, night).accent

    // ---------------- 无障碍对比度（WCAG 2.x） ----------------

    /**
     * 强调色上的文字色：白字过不了 4.5:1 就换深棕。
     *
     * 纯函数所以可单测——珊瑚橙上白字只有 3.12:1，而翠绿/暖阳/玫瑰/海洋白字都在
     * 4.9:1 以上，固定用一种颜色必然有一半皮肤不达标。
     */
    fun accentTextOf(accent: Int): Int =
        if (contrastRatio(WHITE, accent) >= 4.5) WHITE else 0xFF2A1005.toInt()

    private fun channel(v: Int): Double {
        val c = v / 255.0
        return if (c <= 0.03928) c / 12.92 else Math.pow((c + 0.055) / 1.055, 2.4)
    }

    private fun relativeLuminance(color: Int): Double {
        val r = channel((color shr 16) and 0xFF)
        val g = channel((color shr 8) and 0xFF)
        val b = channel(color and 0xFF)
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /**
     * 前景/背景对比度（1:1 ~ 21:1）。
     *
     * 半透明色（键帽/候选条带 alpha）只取其 RGB 当不透明色算：玻璃皮肤真正的背景是
     * 宿主应用，alpha 参与进来只会让断言变得不可复现。
     */
    fun contrastRatio(fg: Int, bg: Int): Double {
        val lf = relativeLuminance(fg)
        val lb = relativeLuminance(bg)
        val hi = maxOf(lf, lb)
        val lo = minOf(lf, lb)
        return (hi + 0.05) / (lo + 0.05)
    }
}
