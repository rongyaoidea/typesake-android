package com.typesake.app

import com.typesake.app.kb.InputRouter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class InputRouterTest {

    @Test
    fun entersShortcut_shapes() {
        assertTrue(InputRouter.entersShortcut('v', '1'))
        assertTrue(InputRouter.entersShortcut('v', '('))
        assertTrue(InputRouter.entersShortcut('i', '5'))
        assertTrue(InputRouter.entersShortcut('i', '-'))
        assertTrue(InputRouter.entersShortcut('u', '4'))
        assertFalse(InputRouter.entersShortcut('v', 'x'))
        assertFalse(InputRouter.entersShortcut('x', '1'))
    }

    @Test
    fun shortcutShapeOk_prefixes() {
        assertTrue(InputRouter.shortcutShapeOk("v"))
        assertTrue(InputRouter.shortcutShapeOk("v1+2"))
        assertTrue(InputRouter.shortcutShapeOk("i123"))
        assertTrue(InputRouter.shortcutShapeOk("im123.45"))
        assertTrue(InputRouter.shortcutShapeOk("u4e00"))
        assertFalse(InputRouter.shortcutShapeOk(""))
        assertFalse(InputRouter.shortcutShapeOk("hello"))
        assertFalse(InputRouter.shortcutShapeOk("v1+2x"))
        assertFalse(InputRouter.shortcutShapeOk("i12a"))
    }

    @Test
    fun zhuyin_routing() {
        // 注音方案 id=8（与 TypesakePrefs.SCHEME_ZHUYIN 一致）
        assertTrue(InputRouter.zhuyinInput("5", 8, 8, false, false, false, false, true))
        assertTrue(InputRouter.zhuyinInput("-", 8, 8, false, false, false, false, true))
        assertTrue(InputRouter.zhuyinInput(".", 8, 8, false, false, false, true, false))
        // 非注音方案不抢
        assertFalse(InputRouter.zhuyinInput("5", 0, 8, false, false, false, false, true))
        // 隐私/英文/T9 不抢
        assertFalse(InputRouter.zhuyinInput("5", 8, 8, true, false, false, false, true))
        assertFalse(InputRouter.zhuyinInput("5", 8, 8, false, true, false, false, true))
        assertFalse(InputRouter.zhuyinInput("5", 8, 8, false, false, true, false, true))
    }

    @Test
    fun kindDict_label() {
        assertEquals("词典", TypesakeCore.EnglishOption(TypesakeCore.KIND_DICT, "China").kindLabel)
        assertEquals("我的", TypesakeCore.EnglishOption(TypesakeCore.KIND_MINE, "x").kindLabel)
        assertEquals("句库", TypesakeCore.EnglishOption(TypesakeCore.KIND_SENTBANK, "x").kindLabel)
    }
}
