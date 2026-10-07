package com.typesake.app

import com.typesake.app.kb.KbPalette
import com.typesake.app.kb.KbThemes
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class KbThemeTest {

    @Test
    fun followsSystemWhenModeIsSystem() {
        assertEquals(
            KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_DARK, false),
            KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_SYSTEM, true),
        )
        assertEquals(
            KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_LIGHT, true),
            KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_SYSTEM, false),
        )
    }

    @Test
    fun explicitModeOverridesSystem() {
        assertNotEquals(
            KbThemes.resolve(KbPalette.ROSE, KbThemes.THEME_LIGHT, true),
            KbThemes.resolve(KbPalette.ROSE, KbThemes.THEME_DARK, true),
        )
    }

    @Test
    fun nightDetection() {
        // UI_MODE_NIGHT_YES = 0x20, UI_MODE_NIGHT_NO = 0x10, TYPE_NORMAL = 0x01
        assertTrue(KbThemes.isNight(0x21))
        assertFalse(KbThemes.isNight(0x11))
    }

    @Test
    fun palettesAreVisuallyDistinct() {
        val accents = KbPalette.entries.map { KbThemes.accentOf(it, night = false) }
        assertEquals(accents.size, accents.toSet().size)
        val darkAccents = KbPalette.entries.map { KbThemes.accentOf(it, night = true) }
        assertEquals(darkAccents.size, darkAccents.toSet().size)
    }

    @Test
    fun defaultPaletteIsCoralGlass() {
        assertEquals(KbPalette.CORAL, KbThemes.paletteOf(0))
        assertEquals(KbPalette.CORAL, KbThemes.paletteOf(99))
        val light = KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_LIGHT, false)
        assertTrue("default skin must be glass", light.glass)
        assertNotEquals(0, light.glassBorder)
        assertEquals(KbThemes.CORAL, light.accent)
        val dark = KbThemes.resolve(KbPalette.CORAL, KbThemes.THEME_DARK, true)
        assertTrue(dark.glass)
        assertEquals(KbThemes.CORAL, dark.accent)
    }

    @Test
    fun nonGlassPalettesStayFlat() {
        for (p in listOf(KbPalette.GREEN, KbPalette.SUNSET, KbPalette.ROSE, KbPalette.OCEAN)) {
            assertFalse(p.name, KbThemes.resolve(p, KbThemes.THEME_LIGHT, false).glass)
        }
    }

    @Test
    fun paletteLookupIsBoundsSafe() {
        assertEquals(KbPalette.CORAL, KbThemes.paletteOf(0))
        assertEquals(KbPalette.OCEAN, KbThemes.paletteOf(4))
        assertEquals(KbPalette.CORAL, KbThemes.paletteOf(-1))
    }

    @Test
    fun contrastRatioMatchesKnownValues() {
        // 黑底白字 21:1、白底黑字 21:1、同色 1:1
        assertEquals(21.0, KbThemes.contrastRatio(0xFF000000.toInt(), 0xFFFFFFFF.toInt()), 0.05)
        assertEquals(1.0, KbThemes.contrastRatio(0xFF123456.toInt(), 0xFF123456.toInt()), 0.001)
        // 半透明只取 RGB：0x80000000 与纯黑等价
        assertEquals(
            21.0,
            KbThemes.contrastRatio(0xFFFFFFFF.toInt(), 0x80000000.toInt()),
            0.05,
        )
    }

    /**
     * 所有皮肤的「文字写在面板上」都必须过 4.5:1。
     *
     * 这条以前没有断言，于是默认皮肤里 `hint(#B0AEA5)` 写在浅灰键帽上只有 1.78:1
     * ——「更多/下页/上页」这些按钮等于看不见，CI 也发现不了。
     */
    @Test
    fun everyPaletteKeepsTextReadable() {
        for (palette in KbPalette.entries) {
            for (dark in listOf(false, true)) {
                val c = KbThemes.resolve(palette, if (dark) KbThemes.THEME_DARK else KbThemes.THEME_LIGHT, dark)
                val where = "${palette.name}/${if (dark) "dark" else "light"}"
                fun check(label: String, fg: Int, bg: Int) {
                    val r = KbThemes.contrastRatio(fg, bg)
                    assertTrue("$where $label contrast=$r", r >= 4.5)
                }
                check("keyText/key", c.keyText, c.key)
                check("actionText/actionKey", c.actionText, c.actionKey)
                check("hint/key", c.hint, c.key)
                check("hint/barBg", c.hint, c.barBg)
                check("barText/barBg", c.barText, c.barBg)
                check("accentText/accent", c.accentText, c.accent)
            }
        }
    }
}
