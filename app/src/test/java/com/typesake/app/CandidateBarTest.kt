package com.typesake.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CandidateBarTest {

    @Test
    fun perPageDependsOnOrientation() {
        assertEquals(8, CandidateBar.perPage(vertical = false))
        assertEquals(4, CandidateBar.perPage(vertical = true))
    }

    @Test
    fun moreOnlyWhenBarIsFull() {
        assertFalse(CandidateBar.showMore(visibleCount = 3, vertical = false, paged = false))
        assertTrue(CandidateBar.showMore(visibleCount = 8, vertical = false, paged = false))
        // 竖排一屏 4 条，占满就该给入口
        assertFalse(CandidateBar.showMore(visibleCount = 3, vertical = true, paged = false))
        assertTrue(CandidateBar.showMore(visibleCount = 4, vertical = true, paged = false))
    }

    @Test
    fun pagedModeReliesOnPageChipsInstead() {
        // 翻页模式一次取 24 条，再挂「更多」只会重复
        assertFalse(CandidateBar.showMore(visibleCount = 8, vertical = false, paged = true))
    }

    @Test
    fun extraDropsCandidatesAlreadyOnTheBar() {
        val all = listOf("你好", "尼豪", "泥猴", "拟好")
        assertEquals(listOf("泥猴", "拟好"), CandidateBar.extra(all, listOf("你好", "尼豪")))
    }

    @Test
    fun extraKeepsOrderAndDeduplicatesNothingElse() {
        assertEquals(listOf("a", "b"), CandidateBar.extra(listOf("a", "b"), emptyList()))
        // 条上只有一个候选时，只去掉那一个
        assertEquals(listOf("b"), CandidateBar.extra(listOf("a", "b"), listOf("a")))
    }

    @Test
    fun pageStartClampsToWholePages() {
        // 24 条 / 每页 8：起点只能是 0 / 8 / 16
        assertEquals(0, CandidateBar.pageStart(total = 24, perPage = 8, wanted = -8))
        assertEquals(8, CandidateBar.pageStart(total = 24, perPage = 8, wanted = 8))
        assertEquals(16, CandidateBar.pageStart(total = 24, perPage = 8, wanted = 16))
        assertEquals(16, CandidateBar.pageStart(total = 24, perPage = 8, wanted = 999))
    }

    @Test
    fun pageStartHandlesPartialLastPage() {
        // 10 条 / 每页 8：最后一页从第 2 条开始，起点仍是 8，不能是 2 也不能是 9
        assertEquals(8, CandidateBar.pageStart(total = 10, perPage = 8, wanted = 8))
        assertEquals(0, CandidateBar.pageStart(total = 3, perPage = 8, wanted = 8))
        assertEquals(0, CandidateBar.pageStart(total = 0, perPage = 8, wanted = 8))
        assertEquals(0, CandidateBar.pageStart(total = 8, perPage = 0, wanted = 8))
    }

    @Test
    fun pageStartIsStableForVerticalMode() {
        // 竖排一屏 4 条：6 条候选时起点只能是 0 / 4
        assertEquals(4, CandidateBar.pageStart(total = 6, perPage = 4, wanted = 4))
        assertEquals(4, CandidateBar.pageStart(total = 6, perPage = 4, wanted = 99))
    }
}