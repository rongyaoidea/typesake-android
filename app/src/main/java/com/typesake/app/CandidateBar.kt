package com.typesake.app

/**
 * 候选条里「更多」入口的纯逻辑（无 Android 依赖，便于 JVM 单测）。
 *
 * 引擎的 `analyze` 一次最多给 8 条，所以「候选条没被占满」时挂了「更多」也只是
 * 弹出一模一样的词；翻页模式一次就取了 24 条，再挂弹层纯属重复。这两条判定放在这里，
 * 渲染层只管按结果画按钮。
 */
object CandidateBar {

    /** 横排一屏 8 条，竖排 4 条。 */
    const val PER_PAGE_H = 8
    const val PER_PAGE_V = 4

    fun perPage(vertical: Boolean): Int = if (vertical) PER_PAGE_V else PER_PAGE_H

    /**
     * 候选条被一屏占满、且不在翻页模式时才给「更多」。
     *
     * @param visibleCount 当前这屏实际画出来的候选数
     * @param vertical 候选是否竖排
     * @param paged 逗号句号当翻页键的模式（此时一次已取满 24 条，用「下页/上页」）
     */
    fun showMore(visibleCount: Int, vertical: Boolean, paged: Boolean): Boolean =
        !paged && visibleCount >= perPage(vertical)

    /**
     * 弹层里只列候选条上还没出现过的词。
     *
     * 引擎的 `moreCandidates` 是把候选池从头重算一遍，前面几条必然和条上重复；
     * 原样弹出来会让人以为按钮没生效。
     */
    fun extra(all: List<String>, shown: Collection<String>): List<String> {
        if (shown.isEmpty()) return all
        val seen = HashSet(shown)
        return all.filterNot { seen.contains(it) }
    }

    /**
     * 翻页起点夹取：页码永远对齐整页，最后一页不满也只到最后一个整页的起点。
     *
     * @param wanted 想去的起点（通常是当前起点 ± 每页条数）
     */
    fun pageStart(total: Int, perPage: Int, wanted: Int): Int {
        if (total <= 0 || perPage <= 0) return 0
        val last = ((total - 1) / perPage) * perPage
        return wanted.coerceIn(0, last)
    }
}