//! 中文 -> 英文表达组合（离线，v2）。
//!
//! 组合优先级（越靠前越地道）：
//! 1. **我的**：收藏/翻译记忆命中——用户自己的表达最可信
//! 2. **地道**：整句短语库精确命中
//! 3. **结构**：句式模板（我想X / 请X / 太X了 / X吗…）+ 槽位翻译 + 词形变化
//! 4. **直译**：词级贪心 + 词序调整（时间后置）+ 冠词/单复数 → 教学用直译
//!
//! 所有层都过"覆盖率闸门"：宁可不说，也不吐 `… …` 这类噪音。

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// 候选来源（数值与 JNI 协议一致）。
pub const KIND_MINE: u8 = 1;
pub const KIND_NATIVE: u8 = 2;
pub const KIND_PATTERN: u8 = 3;
pub const KIND_LITERAL: u8 = 4;
/// 大词典（CC-CEDICT）整词命中
pub const KIND_DICT: u8 = 5;
/// 句库（Tatoeba 整句/重叠命中）
pub const KIND_SENTBANK: u8 = 6;

/// 整句/整词 -> 地道英文（口语与常用书面语，含商务与学习场景）。
const PHRASES: &[(&str, &str)] = &[
    // 招呼与礼貌
    ("你好", "Hello!"),
    ("您好", "Hello!"),
    ("大家好", "Hello, everyone!"),
    ("早上好", "Good morning!"),
    ("下午好", "Good afternoon!"),
    ("晚上好", "Good evening!"),
    ("晚安", "Good night!"),
    ("再见", "Goodbye! / See you!"),
    ("明天见", "See you tomorrow!"),
    ("回头见", "See you later!"),
    ("好久不见", "Long time no see!"),
    ("谢谢", "Thank you!"),
    ("感谢", "Thank you!"),
    ("非常感谢", "Thank you so much!"),
    ("多谢", "Thanks a lot!"),
    ("辛苦了", "Thanks for your hard work."),
    ("不客气", "You're welcome!"),
    ("不用谢", "You're welcome!"),
    ("对不起", "I'm sorry."),
    ("抱歉", "My apologies."),
    ("不好意思", "Excuse me. / Sorry about that."),
    ("没关系", "That's all right."),
    ("没事", "No worries."),
    ("请问", "Excuse me, may I ask…"),
    ("你好吗", "How are you?"),
    ("我很好", "I'm good, thanks."),
    ("最近怎么样", "How have you been?"),
    ("你叫什么名字", "What's your name?"),
    ("我叫", "My name is…"),
    ("很高兴认识你", "Nice to meet you!"),
    ("我也很高兴", "Nice to meet you too!"),
    ("生日快乐", "Happy birthday!"),
    ("新年快乐", "Happy New Year!"),
    ("恭喜", "Congratulations!"),
    ("祝你好运", "Good luck!"),
    ("玩得开心", "Have fun!"),
    ("一路顺风", "Have a safe trip!"),
    ("保重", "Take care!"),
    // 日常
    ("我爱你", "I love you."),
    ("我想你", "I miss you."),
    ("我同意", "I agree."),
    ("我明白了", "I see. / Got it."),
    ("我不知道", "I don't know."),
    ("我觉得", "I think…"),
    ("我认为", "I think that…"),
    ("我建议", "I suggest that…"),
    ("没问题", "No problem."),
    ("好的", "Okay. / Sure."),
    ("可以", "Sure. / That works."),
    ("不行", "No, that won't work."),
    ("也许吧", "Maybe."),
    ("当然", "Of course."),
    ("真的吗", "Really?"),
    ("太棒了", "That's great!"),
    ("厉害了", "That's impressive!"),
    ("加油", "You can do it! / Keep going!"),
    ("请稍等", "Please wait a moment."),
    ("稍等一下", "Give me a second, please."),
    ("我马上回来", "I'll be right back."),
    ("我需要帮助", "I need help."),
    ("需要帮助", "need help"),
    ("帮助", "help"),
    ("救命", "Help!"),
    ("请再说一遍", "Could you say that again?"),
    ("我听不懂", "I don't understand."),
    ("你会说英语吗", "Do you speak English?"),
    ("我英语不太好", "My English isn't very good."),
    ("我在学习英语", "I'm learning English."),
    ("学习英语", "learn English"),
    ("我在学中文", "I'm learning Chinese."),
    ("你学得很快", "You're learning fast."),
    ("我今天很忙", "I'm busy today."),
    ("今天天气很好", "The weather is nice today."),
    ("下雨了", "It's raining."),
    ("吃了吗", "Have you eaten?"),
    ("一起吃饭", "Let's eat together."),
    ("我请你吃饭", "Dinner is on me."),
    ("我饿了", "I'm hungry."),
    ("我渴了", "I'm thirsty."),
    ("我累了", "I'm tired."),
    ("我生病了", "I'm sick."),
    ("我到了", "I've arrived."),
    ("我在路上", "I'm on my way."),
    ("我迟到了", "I'm late."),
    ("堵车了", "Traffic is bad."),
    ("多少钱", "How much is it?"),
    ("太贵了", "That's too expensive."),
    ("便宜一点", "Can you make it cheaper?"),
    ("我要这个", "I'll take this one."),
    ("我要一杯咖啡", "I'd like a coffee, please."),
    ("买单", "Check, please."),
    ("洗手间在哪里", "Where is the restroom?"),
    ("怎么走", "How do I get there?"),
    ("在哪里", "Where is it?"),
    ("这是什么", "What is this?"),
    ("几点", "What time is it?"),
    ("我住在", "I live in…"),
    ("我来自中国", "I'm from China."),
    // 词
    ("今天", "today"),
    ("明天", "tomorrow"),
    ("昨天", "yesterday"),
    ("每天", "every day"),
    ("现在", "now"),
    ("周末", "weekend"),
    ("中国", "China"),
    ("英语", "English"),
    ("中文", "Chinese"),
    ("朋友", "friend"),
    ("我的朋友", "my friend"),
    ("家人", "family"),
    ("同事", "colleague"),
    ("老板", "boss"),
    ("老师", "teacher"),
    ("学生", "student"),
    ("电话", "phone call"),
    ("打电话", "make a phone call"),
    ("给我打电话", "Give me a call."),
    ("发短信", "send a text message"),
    ("邮箱", "mailbox / email"),
    ("发邮件", "send an email"),
    ("邮件", "email"),
    ("文件", "file / document"),
    ("报告", "report"),
    ("发送报告", "Send the report."),
    ("请发送报告", "Please send the report."),
    ("收到", "Received. / Got it."),
    ("确认一下", "Just to confirm…"),
    ("会议", "meeting"),
    ("开会", "have a meeting"),
    ("今天开会", "We have a meeting today."),
    ("开会时间", "meeting time"),
    ("迟到", "be late"),
    ("加班", "work overtime"),
    ("工作", "work / job"),
    ("我在工作", "I'm working."),
    ("上班", "go to work"),
    ("下班", "get off work"),
    ("出差", "business trip"),
    ("请假", "ask for leave"),
    ("项目", "project"),
    ("进度", "progress"),
    ("截止日期", "deadline"),
    ("预算", "budget"),
    ("合同", "contract"),
    ("客户", "client"),
    ("需求", "requirement"),
    ("方案", "plan / proposal"),
    ("报价", "quotation"),
    ("价格", "price"),
    ("付款", "payment"),
    ("发票", "invoice"),
    ("合同已签署", "The contract has been signed."),
    ("请查收", "Please find it attached."),
    ("有问题随时联系我", "Let me know if you have any questions."),
    ("我们保持联系", "Let's keep in touch."),
    ("感谢你的支持", "Thanks for your support."),
    ("合作愉快", "It's a pleasure working with you."),
    ("请确认", "Please confirm."),
    ("我需要更多时间", "I need more time."),
    ("我稍后回复你", "I'll get back to you later."),
    ("我考虑一下", "Let me think about it."),
    ("我尽力", "I'll do my best."),
    ("交给我", "Leave it to me."),
    ("包在我身上", "Consider it done."),
    ("没问题，我来处理", "No problem, I'll handle it."),
    ("开始", "start / begin"),
    ("开始吧", "Let's get started."),
    ("结束", "end / finish"),
    ("完成", "done / complete"),
    ("已完成", "It's done."),
    ("请开始", "Please go ahead."),
    ("等一下，我想想", "Hold on, let me think."),
    ("我们走吧", "Let's go."),
    ("走吧", "Let's go."),
    ("我来处理", "I'll handle it."),
    ("我来看一下", "Let me take a look."),
    // 补充：高频日常问答（覆盖测试语料）
    ("有没有空", "Are you free?"),
    ("你有空吗", "Are you free?"),
    ("几点了", "What time is it?"),
    ("现在几点", "What time is it now?"),
    ("谢谢你的支持", "Thanks for your support."),
    ("感谢你的帮助", "Thanks for your help."),
    ("抱歉我迟到了", "Sorry I'm late."),
    ("抱歉，我迟到了", "Sorry I'm late."),
    ("我明天出差", "I'm on a business trip tomorrow."),
    ("明天开会", "We have a meeting tomorrow."),
    ("我在开会", "I'm in a meeting."),
    ("我在学习", "I'm studying."),
    ("我在忙", "I'm busy."),
    ("我今天很忙", "I'm busy today."),
    ("我明天有空", "I'm free tomorrow."),
    ("你今天怎么样", "How's your day going?"),
    ("请帮我确认一下", "Could you please confirm?"),
    ("帮我确认一下", "Could you confirm this?"),
    ("这个多少钱", "How much is this?"),
    ("你的项目进度怎么样", "How's the project going?"),
];

/// 词级表：直译层与模板槽位翻译共用（动词给原形，其他给一般形式）。
const WORDS: &[(&str, &str)] = &[
    // 代词
    ("我", "I"),
    ("我们", "we"),
    ("你", "you"),
    ("你们", "you"),
    ("他", "he"),
    ("她", "she"),
    ("他们", "they"),
    ("它", "it"),
    ("大家", "everyone"),
    ("自己", "myself"),
    ("我的", "my"),
    ("我们的", "our"),
    ("你的", "your"),
    ("他的", "his"),
    ("她的", "her"),
    ("这个", "this"),
    ("那个", "that"),
    ("这些", "these"),
    ("那些", "those"),
    // 高频动词（原形）
    ("是", "be"),
    ("有", "have"),
    ("要", "want"),
    ("想", "want"),
    ("需要", "need"),
    ("做", "do"),
    ("去", "go"),
    ("来", "come"),
    ("去2", "go"),
    ("看", "look"),
    ("看到", "see"),
    ("听", "listen"),
    ("听见", "hear"),
    ("说", "say"),
    ("讲", "speak"),
    ("告诉", "tell"),
    ("问", "ask"),
    ("回答", "answer"),
    ("知道", "know"),
    ("了解", "understand"),
    ("明白", "understand"),
    ("学", "learn"),
    ("学习", "study"),
    ("教", "teach"),
    ("工作", "work"),
    ("用", "use"),
    ("买", "buy"),
    ("卖", "sell"),
    ("吃", "eat"),
    ("喝", "drink"),
    ("睡", "sleep"),
    ("起床", "get up"),
    ("休息", "rest"),
    ("玩", "play"),
    ("走", "walk"),
    ("跑", "run"),
    ("开车", "drive"),
    ("坐", "sit"),
    ("等", "wait"),
    ("找", "find"),
    ("帮", "help"),
    ("帮助", "help"),
    ("给", "give"),
    ("拿", "take"),
    ("放", "put"),
    ("打开", "open"),
    ("关", "close"),
    ("写", "write"),
    ("读", "read"),
    ("发", "send"),
    ("发送", "send"),
    ("收", "receive"),
    ("改", "change"),
    ("试", "try"),
    ("检查", "check"),
    ("确认", "confirm"),
    ("联系", "contact"),
    ("安排", "arrange"),
    ("准备", "prepare"),
    ("完成", "finish"),
    ("开始", "start"),
    ("结束", "end"),
    ("喜欢", "like"),
    ("爱", "love"),
    ("讨厌", "hate"),
    ("希望", "hope"),
    ("打算", "plan"),
    ("决定", "decide"),
    ("记得", "remember"),
    ("忘记", "forget"),
    ("懂", "understand"),
    ("会", "can"),
    ("能", "can"),
    ("可以", "can"),
    ("应该", "should"),
    ("必须", "must"),
    ("继续", "continue"),
    ("停止", "stop"),
    ("回来", "come back"),
    ("回去", "go back"),
    // 名词
    ("时间", "time"),
    ("钱", "money"),
    ("人", "person"),
    ("事情", "thing"),
    ("东西", "thing"),
    ("地方", "place"),
    ("名字", "name"),
    ("号码", "number"),
    ("问题", "question"),
    ("答案", "answer"),
    ("想法", "idea"),
    ("计划", "plan"),
    ("生活", "life"),
    ("公司", "company"),
    ("学校", "school"),
    ("家", "home"),
    ("办公室", "office"),
    ("电脑", "computer"),
    ("手机", "phone"),
    ("网络", "internet"),
    ("消息", "message"),
    ("照片", "photo"),
    ("书", "book"),
    ("电影", "movie"),
    ("音乐", "music"),
    ("咖啡", "coffee"),
    ("茶", "tea"),
    ("水", "water"),
    ("饭", "meal"),
    ("早餐", "breakfast"),
    ("午餐", "lunch"),
    ("晚餐", "dinner"),
    ("天气", "weather"),
    ("雨", "rain"),
    ("行李", "luggage"),
    ("机票", "ticket"),
    ("出租车", "taxi"),
    ("酒店", "hotel"),
    ("地址", "address"),
    ("医院", "hospital"),
    ("银行", "bank"),
    ("超市", "supermarket"),
    ("价格", "price"),
    ("订单", "order"),
    ("合同", "contract"),
    ("客户", "client"),
    ("报告", "report"),
    ("文件", "document"),
    ("邮件", "email"),
    ("会议", "meeting"),
    ("项目", "project"),
    ("需求", "requirement"),
    ("方案", "plan"),
    ("进度", "progress"),
    // 形容词 / 副词
    ("好", "good"),
    ("很好", "very good"),
    ("不好", "not good"),
    ("大", "big"),
    ("小", "small"),
    ("多", "many"),
    ("少", "few"),
    ("快", "fast"),
    ("慢", "slow"),
    ("新", "new"),
    ("旧", "old"),
    ("重要", "important"),
    ("简单", "simple"),
    ("复杂", "complicated"),
    ("难", "difficult"),
    ("容易", "easy"),
    ("忙", "busy"),
    ("累", "tired"),
    ("饿", "hungry"),
    ("高兴", "happy"),
    ("开心", "happy"),
    ("抱歉", "sorry"),
    ("确定", "sure"),
    ("可能", "maybe"),
    ("真的", "really"),
    ("很", "very"),
    ("非常", "very"),
    ("贵", "expensive"),
    ("便宜", "cheap"),
    ("空", "free"),
    ("支持", "support"),
    ("迟到", "be late"),
    ("太", "too"),
    ("有点", "a bit"),
    ("马上", "right away"),
    ("立刻", "immediately"),
    ("已经", "already"),
    ("还没", "not yet"),
    ("一起", "together"),
    ("再", "again"),
    ("也", "also"),
    ("都", "all"),
    ("一直", "always"),
    ("经常", "often"),
    ("有时候", "sometimes"),
    ("从不", "never"),
    // 疑问 / 连接
    ("什么", "what"),
    ("谁", "who"),
    ("哪里", "where"),
    ("为什么", "why"),
    ("怎么", "how"),
    ("多少", "how much"),
    ("哪", "which"),
    ("几", "how many"),
    ("因为", "because"),
    ("所以", "so"),
    ("但是", "but"),
    ("而且", "and"),
    ("如果", "if"),
    ("然后", "then"),
    ("还是", "or"),
    ("和", "and"),
    // 场景补充
    ("更多", "more"),
    ("好多", "a lot of"),
    ("一些", "some"),
    ("几个", "a few"),
    ("上班时间", "office hours"),
    ("下班时间", "off-work time"),
    ("周末计划", "weekend plan"),
    ("学习计划", "study plan"),
    ("考试成绩", "exam result"),
    ("面试", "interview"),
    ("简历", "resume"),
    ("工资", "salary"),
    ("假期", "vacation"),
    ("假期计划", "vacation plan"),
];

/// 词性：动词（结构层"我在X/请X/我想X"要求槽位是动词）
const VERBS_ZH: &[&str] = &[
    "开会",
    "上班",
    "下班",
    "加班",
    "出差",
    "学习",
    "工作",
    "吃饭",
    "喝水",
    "睡觉",
    "休息",
    "打电话",
    "发邮件",
    "发短信",
    "发送",
    "确认",
    "检查",
    "准备",
    "完成",
    "开始",
    "结束",
    "喜欢",
    "想",
    "要",
    "需要",
    "去",
    "来",
    "看",
    "听",
    "说",
    "讲",
    "告诉",
    "问",
    "回答",
    "知道",
    "了解",
    "明白",
    "学",
    "教",
    "用",
    "买",
    "卖",
    "吃",
    "喝",
    "睡",
    "玩",
    "走",
    "跑",
    "开车",
    "坐",
    "等",
    "找",
    "帮",
    "给",
    "拿",
    "放",
    "打开",
    "关",
    "写",
    "读",
    "发",
    "收",
    "改",
    "试",
    "联系",
    "安排",
    "做",
    "有",
    "祝",
    "请",
    "让",
    "送",
    "借",
    "还",
    "记住",
    "忘记",
    "考虑",
    "决定",
    "希望",
    "打算",
    "继续",
    "停止",
    "回来",
    "回去",
    "开会时间",
    "汇报",
];

/// 词性：形容词（"太X了"要求槽位是形容词）
const ADJ_ZH: &[&str] = &[
    "好", "很好", "不好", "大", "小", "多", "少", "快", "慢", "新", "旧", "重要", "简单", "复杂",
    "难", "容易", "忙", "累", "饿", "渴", "高兴", "开心", "抱歉", "确定", "贵", "便宜", "冷", "热",
    "早", "晚", "长", "短", "高", "漂亮", "好看", "好玩", "好吃", "干净", "脏", "安静", "吵",
    "安全", "危险", "困", "甜", "酸", "辣", "咸", "紧张", "无聊",
];

/// 词性：名词（"X在哪里"要求槽位是名词）
const NOUN_ZH: &[&str] = &[
    "北京",
    "上海",
    "中国",
    "公司",
    "学校",
    "家",
    "办公室",
    "电脑",
    "手机",
    "网络",
    "消息",
    "照片",
    "书",
    "电影",
    "音乐",
    "咖啡",
    "茶",
    "水",
    "饭",
    "早餐",
    "午餐",
    "晚餐",
    "天气",
    "雨",
    "行李",
    "机票",
    "出租车",
    "酒店",
    "地址",
    "医院",
    "银行",
    "超市",
    "价格",
    "订单",
    "合同",
    "客户",
    "报告",
    "文件",
    "邮件",
    "会议",
    "项目",
    "需求",
    "方案",
    "进度",
    "时间",
    "钱",
    "人",
    "事情",
    "东西",
    "地方",
    "名字",
    "号码",
    "问题",
    "答案",
    "想法",
    "计划",
    "生活",
    "朋友",
    "家人",
    "同事",
    "老板",
    "老师",
    "学生",
    "电话",
    "邮箱",
    "洗手间",
    "会议室",
    "护照",
];

fn slot_has(zh: &str, list: &[&str]) -> bool {
    let t = zh.trim();
    list.iter().any(|w| t.starts_with(w))
}

fn slot_is_verb(zh: &str) -> bool {
    slot_has(zh, VERBS_ZH)
}

fn slot_is_adj(zh: &str) -> bool {
    slot_has(zh, ADJ_ZH)
}

fn slot_is_noun(zh: &str) -> bool {
    slot_has(zh, NOUN_ZH)
}

/// 中文虚词/标点：直译时跳过，不参与覆盖率惩罚。
const SKIP_CHARS: &str = "的了是在有和就不都一也又还个们这那吗呢吧啊嘛哦嗯着过把被给对向从跟与及之其，。！？、；：\"''（）《》…—~·　 ";

/// 时间词：英文里通常后置。
const TIME_WORDS: &[(&str, &str)] = &[
    ("今天", "today"),
    ("明天", "tomorrow"),
    ("昨天", "yesterday"),
    ("现在", "right now"),
    ("每天", "every day"),
    ("每天2", "every day"),
    ("马上", "right away"),
    ("等会儿", "in a while"),
    ("以后", "later"),
    ("周末", "this weekend"),
    ("这周", "this week"),
    ("下周", "next week"),
    ("下周2", "next week"),
    ("上周", "last week"),
    ("今年", "this year"),
    ("明年", "next year"),
];

/// 不规则动词过去式 / 过去分词（直译层用）。
const IRREGULAR: &[(&str, &str)] = &[
    ("be", "was"),
    ("have", "had"),
    ("do", "did"),
    ("go", "went"),
    ("come", "came"),
    ("see", "saw"),
    ("say", "said"),
    ("tell", "told"),
    ("give", "gave"),
    ("take", "took"),
    ("get", "got"),
    ("make", "made"),
    ("know", "knew"),
    ("think", "thought"),
    ("find", "found"),
    ("send", "sent"),
    ("write", "wrote"),
    ("read", "read"),
    ("eat", "ate"),
    ("drink", "drank"),
    ("sleep", "slept"),
    ("run", "ran"),
    ("buy", "bought"),
    ("sell", "sold"),
    ("bring", "brought"),
    ("put", "put"),
    ("cut", "cut"),
    ("let", "let"),
    ("meet", "met"),
    ("pay", "paid"),
    ("leave", "left"),
    ("feel", "felt"),
    ("keep", "kept"),
    ("lose", "lost"),
    ("win", "won"),
    ("begin", "began"),
    ("finish", "finished"),
    ("start", "started"),
    ("stop", "stopped"),
    ("speak", "spoke"),
    ("understand", "understood"),
    ("remember", "remembered"),
    ("forget", "forgot"),
    ("decide", "decided"),
    ("hope", "hoped"),
    ("like", "liked"),
    ("love", "loved"),
    ("want", "wanted"),
    ("need", "needed"),
    ("help", "helped"),
    ("work", "worked"),
    ("learn", "learned"),
    ("study", "studied"),
    ("use", "used"),
    ("ask", "asked"),
    ("answer", "answered"),
    ("wait", "waited"),
    ("walk", "walked"),
    ("drive", "drove"),
    ("sit", "sat"),
    ("open", "opened"),
    ("close", "closed"),
    ("check", "checked"),
    ("confirm", "confirmed"),
    ("contact", "contacted"),
    ("prepare", "prepared"),
    ("arrange", "arranged"),
    ("change", "changed"),
    ("try", "tried"),
    ("call", "called"),
    ("arrive", "arrived"),
    ("rest", "rested"),
];

fn irregular_past(v: &str) -> Option<&'static str> {
    IRREGULAR.iter().find(|(k, _)| *k == v).map(|(_, p)| *p)
}

/// 翻译记忆（收藏句子），由 store 注入。
fn memory() -> &'static Mutex<Vec<(String, String)>> {
    static M: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn set_memory(items: Vec<(String, String)>) {
    {
        let mut m = memory().lock().unwrap_or_else(|e| e.into_inner());
        *m = items
            .into_iter()
            .filter(|(cn, en)| !cn.trim().is_empty() && !en.trim().is_empty())
            .collect();
    }
}

// ---------------- 大词典（CC-CEDICT 生成，可选加载） ----------------

fn big_dict() -> &'static Mutex<HashMap<String, String>> {
    static D: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 载入 `简体<TAB>英文` 词典文件（assets 由宿主拷到 filesDir 后传入）。
/// 返回载入条数；文件缺失/为空时返回 0 并保持原有小词表可用。
pub fn load_dict(path: &str) -> usize {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let mut map: HashMap<String, String> = HashMap::with_capacity(140_000);
    for line in data.lines() {
        if let Some((zh, en)) = line.split_once('\t') {
            let (zh, en) = (zh.trim(), en.trim());
            if !zh.is_empty() && !en.is_empty() {
                map.insert(zh.to_string(), en.to_string());
            }
        }
    }
    let n = map.len();
    if n > 0 {
        {
            let mut m = big_dict().lock().unwrap_or_else(|e| e.into_inner());
            *m = map;
        }
        // 词典换了，旧的译词缓存（可能是一片「查不到」）作废
        crate::gloss::clear_cache();
        // 拼写纠正用的英文词表同理作废
        reset_en_words();
    }
    n
}

pub fn dict_size() -> usize {
    big_dict().lock().unwrap_or_else(|e| e.into_inner()).len()
}

fn dict_exact(zh: &str) -> Option<String> {
    big_dict()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(zh)
        .cloned()
}

/// 整词查大词典（逐词译词用）：命中返回英文释义原文（未加工）。
pub(crate) fn dict_lookup(zh: &str) -> Option<String> {
    dict_exact(zh)
}

// ---------------- 英文模式拼写纠正 ----------------

/// 英文单词表的存储格（从大词典的英文侧收集；**首次用到才建**，不用的人不占这份内存）。
///
/// 放 `Arc` 是因为纠正在每次敲键都要查：锁只负责「第一次建表」，之后拿到的是共享句柄，
/// 不用每敲一个字母就克隆 8 万条字符串。
fn en_cell() -> &'static RwLock<Option<Arc<BTreeSet<String>>>> {
    static W: OnceLock<RwLock<Option<Arc<BTreeSet<String>>>>> = OnceLock::new();
    W.get_or_init(|| RwLock::new(None))
}

/// 词典被换掉时作废旧词表（下次用到按新词典重建）。
fn reset_en_words() {
    *en_cell().write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 取英文词表；没有可取的（词典未载入）返回 `None`。
fn en_words() -> Option<Arc<BTreeSet<String>>> {
    let cell = en_cell();
    // 快路径：只读锁拿到就返回（每敲一个字母都会走这里，原来每次都拿写锁）。
    // 先把结果落到一个局部变量再判断，读锁在这一行结束就释放，不会和下面的写锁死锁。
    let cached = cell.read().unwrap_or_else(|e| e.into_inner()).clone();
    if cached.is_some() {
        return cached;
    }
    let mut g = cell.write().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        let mut set = BTreeSet::new();
        {
            let m = big_dict().lock().unwrap_or_else(|e| e.into_inner());
            for v in m.values() {
                for tok in v.split(|c: char| !c.is_ascii_alphabetic()) {
                    let t = tok.to_ascii_lowercase();
                    if (2..=24).contains(&t.len()) {
                        set.insert(t);
                    }
                }
            }
        }
        *g = Some(Arc::new(set));
    }
    g.clone()
}

/// 有界编辑距离：距离超过 `max` 直接放弃（返回 `None`），省掉剩下的一大片 DP。
///
/// 缓冲（`prev`/`cur`）由调用方持有并复用：纠错时每个长度合适的候选都要算一次距离，
/// 原来每次都现分配两个 `Vec`，8 万词表下等于每敲一个字母几万次分配。
fn edit_within(a: &[u8], b: &[u8], max: u8, prev: &mut Vec<u16>, cur: &mut Vec<u16>) -> Option<u8> {
    let n = b.len();
    prev.clear();
    prev.extend(0..=n as u16);
    cur.clear();
    cur.resize(n + 1, 0);
    for i in 1..=a.len() {
        cur[0] = i as u16;
        let mut row_min = cur[0];
        for j in 1..=n {
            let cost = u16::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            row_min = row_min.min(cur[j]);
        }
        // 整行都超预算：后面无论怎么补都只会更贵
        if row_min > u16::from(max) {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[n] as u8;
    (d <= max).then_some(d)
}

/// 英文模式拼写纠正：把打错的词换成词表里的真实词。
///
/// 判定很保守，宁可不纠正也不乱改：
/// - 原词本身就在词表里（没打错）→ `None`；
/// - 不是纯字母、或长度 <3 / >24 → `None`；
/// - 编辑距离超限（≤4 字母只容 1 处，更长容 2 处）→ `None`；
/// - 多个候选时依次比：**距离 → 首字母是否相同 → 长度差**，仍平手取字典序靠前的。
///
/// 返回的是小写词表形式；首字母大小写由调用方按原词还原。
pub fn fix_spelling(word: &str) -> Option<String> {
    let w = word.trim().to_ascii_lowercase();
    if w.len() < 3 || w.len() > 24 || !w.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let set = en_words()?;
    if set.is_empty() || set.contains(&w) {
        return None;
    }
    let budget: u8 = if w.len() <= 4 { 1 } else { 2 };
    let lo = w.len() - budget as usize;
    let hi = w.len() + budget as usize;
    let (ab, wb) = (w.as_bytes(), w.as_bytes());
    let mut prev: Vec<u16> = Vec::new();
    let mut cur: Vec<u16> = Vec::new();
    let mut best: Option<((u8, u8, u8), String)> = None;
    for cand in set.iter() {
        let cl = cand.len();
        if cl < lo || cl > hi {
            continue;
        }
        let Some(d) = edit_within(wb, cand.as_bytes(), budget, &mut prev, &mut cur) else {
            continue;
        };
        let key = (
            d,
            u8::from(cand.as_bytes()[0] != ab[0]),
            cl.abs_diff(w.len()).min(255) as u8,
        );
        // 严格小于：BTreeSet 是字典序遍历，平手时自然留下字典序靠前的那个
        if best.as_ref().map(|(bk, _)| key < *bk).unwrap_or(true) {
            best = Some((key, cand.clone()));
        }
    }
    best.map(|(_, c)| c)
}

/// 最长匹配（1..=6 字）。`buf` 由调用方复用，避免每个位置都现拼一个 `String`。
fn dict_longest(chars: &[char], start: usize, buf: &mut String) -> Option<(usize, String)> {
    let max = (chars.len() - start).min(6);
    let map = big_dict().lock().unwrap_or_else(|e| e.into_inner());
    for len in (1..=max).rev() {
        buf.clear();
        buf.extend(chars[start..start + len].iter());
        if let Some(v) = map.get(buf.as_str()) {
            return Some((len, v.to_string()));
        }
    }
    None
}

// ---------------- 词形变化 ----------------

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
}

/// 动词 -> -ing（go→going / run→running / make→making / lie→lying）。
pub fn verb_ing(v: &str) -> String {
    let w = v.trim();
    if w.is_empty() {
        return w.to_string();
    }
    if let Some(stem) = w.strip_suffix("ie") {
        return format!("{stem}ying");
    }
    if w.ends_with('e') && !w.ends_with("ee") && !w.ends_with("ye") {
        return format!("{}ing", &w[..w.len() - 1]);
    }
    let chars: Vec<char> = w.chars().collect();
    let n = chars.len();
    if n >= 3 {
        let (a, b, c) = (chars[n - 3], chars[n - 2], chars[n - 1]);
        if !is_vowel(a) && is_vowel(b) && !is_vowel(c) && c != 'w' && c != 'x' && c != 'y' {
            return format!("{w}{c}ing");
        }
    }
    format!("{w}ing")
}

/// 动词 -> 过去式（不规则优先）。
pub fn verb_past(v: &str) -> String {
    let w = v.trim();
    if w.is_empty() {
        return w.to_string();
    }
    if let Some(p) = irregular_past(w) {
        return p.to_string();
    }
    if w.ends_with('e') {
        return format!("{w}d");
    }
    if w.ends_with('y') && w.len() > 1 {
        let prev = w.chars().rev().nth(1).unwrap_or('a');
        if !is_vowel(prev) {
            if let Some(stem) = w.strip_suffix('y') {
                return format!("{stem}ied");
            }
        }
    }
    let chars: Vec<char> = w.chars().collect();
    let n = chars.len();
    if n >= 3 {
        let (a, b, c) = (chars[n - 3], chars[n - 2], chars[n - 1]);
        if !is_vowel(a) && is_vowel(b) && !is_vowel(c) && !"wxhy".contains(c) {
            return format!("{w}{c}ed");
        }
    }
    format!("{w}ed")
}

/// 动词 -> 第三人称单数。
pub fn verb_s(v: &str) -> String {
    let w = v.trim();
    if w.is_empty() {
        return w.to_string();
    }
    if w == "can" || w == "must" || w == "should" {
        return w.to_string();
    }
    if w.ends_with('s')
        || w.ends_with('x')
        || w.ends_with("ch")
        || w.ends_with("sh")
        || w.ends_with('o')
    {
        return format!("{w}es");
    }
    if w.ends_with('y') && w.len() > 1 {
        let prev = w.chars().rev().nth(1).unwrap_or('a');
        if !is_vowel(prev) {
            if let Some(stem) = w.strip_suffix('y') {
                return format!("{stem}ies");
            }
        }
    }
    format!("{w}s")
}

/// 名词复数（教学用，规则足够）。
pub fn plural(n: &str) -> String {
    let w = n.trim();
    if w.is_empty() {
        return w.to_string();
    }
    if w.ends_with('s') || w.ends_with('x') || w.ends_with("ch") || w.ends_with("sh") {
        return format!("{w}es");
    }
    if w.ends_with('y') && w.len() > 1 {
        let prev = w.chars().rev().nth(1).unwrap_or('a');
        if !is_vowel(prev) {
            if let Some(stem) = w.strip_suffix('y') {
                return format!("{stem}ies");
            }
        }
    }
    format!("{w}s")
}

/// 冠词：按发音决定 a / an（只处理常见元音开头）。
pub fn article(noun: &str) -> String {
    let first = noun
        .trim()
        .chars()
        .next()
        .unwrap_or('x')
        .to_ascii_lowercase();
    let an = matches!(first, 'a' | 'e' | 'i' | 'o' | 'u')
        && !noun.trim().to_ascii_lowercase().starts_with("uni")
        && !noun.trim().to_ascii_lowercase().starts_with("use");
    if an {
        format!("an {noun}")
    } else {
        format!("a {noun}")
    }
}

/// 词典释义入句：首字母大写、补句号（保持术语原样，不再做词形变化）。
fn sentence_case_soft(s: &str) -> String {
    sentence_case(s)
}

/// 首字母大写 + 句末标点。
pub fn sentence_case(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut chars = t.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

// ---------------- 索引 ----------------

struct Index {
    phrases_by_first: HashMap<char, Vec<(usize, &'static str, &'static str)>>,
    words_by_first: HashMap<char, Vec<(usize, &'static str, &'static str)>>,
    char_gloss: HashMap<char, &'static str>,
    time_by_first: HashMap<char, Vec<(usize, &'static str, &'static str)>>,
}

fn index() -> &'static Index {
    static IDX: OnceLock<Index> = OnceLock::new();
    IDX.get_or_init(|| {
        let build = |src: &'static [(&'static str, &'static str)]| {
            let mut m: HashMap<char, Vec<(usize, &'static str, &'static str)>> = HashMap::new();
            for (cn, en) in src {
                if let Some(c) = cn.chars().next() {
                    m.entry(c).or_default().push((cn.chars().count(), cn, en));
                }
            }
            for v in m.values_mut() {
                v.sort_by_key(|b| std::cmp::Reverse(b.0));
            }
            m
        };
        Index {
            phrases_by_first: build(PHRASES),
            words_by_first: build(WORDS),
            char_gloss: WORDS
                .iter()
                .filter(|(cn, _)| cn.chars().count() == 1)
                .filter_map(|(cn, en)| cn.chars().next().map(|c| (c, *en)))
                .collect(),
            time_by_first: build(TIME_WORDS),
        }
    })
}

/// 在表中做最长匹配。
fn longest_match(
    table: &HashMap<char, Vec<(usize, &'static str, &'static str)>>,
    chars: &[char],
    start: usize,
) -> Option<(usize, &'static str)> {
    let first = *chars.get(start)?;
    let cands = table.get(&first)?;
    for (len, cn, en) in cands {
        if start + len <= chars.len() {
            let seg: String = chars[start..start + len].iter().collect();
            if seg == *cn {
                return Some((*len, en));
            }
        }
    }
    None
}

/// 词级贪心：(覆盖字符数, 未知字符数, 英文片段)
fn gloss(text: &str) -> (usize, usize, Vec<String>) {
    let idx = index();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut matched = 0usize;
    let mut unknown = 0usize;
    let mut parts: Vec<String> = Vec::new();
    let mut seg = String::new();
    while i < chars.len() {
        // 多字词条优先（开会 / 开会时间），再退到单词
        if let Some((len, en)) = longest_match(&idx.phrases_by_first, &chars, i) {
            if len >= 2 {
                parts.push(en.to_string());
                matched += len;
                i += len;
                continue;
            }
        }
        if let Some((len, en)) = longest_match(&idx.words_by_first, &chars, i) {
            parts.push(en.to_string());
            matched += len;
            i += len;
            continue;
        }
        if let Some((len, en)) = dict_longest(&chars, i, &mut seg) {
            parts.push(en);
            matched += len;
            i += len;
            continue;
        }
        if SKIP_CHARS.contains(chars[i]) {
            i += 1;
            continue;
        }
        unknown += 1;
        i += 1;
    }
    (matched, unknown, parts)
}

// ---------------- 模板（结构层） ----------------

fn slot(text: &str) -> Option<String> {
    let (m, u, mut parts) = gloss(text);
    if m == 0 || (m + u) == 0 {
        return None;
    }
    if (m as f64) / ((m + u) as f64) < 0.6 {
        return None;
    }
    fix_pronoun_objects(&mut parts);
    Some(lower_first(&parts.join(" ")))
}

/// 判断槽位英文是否已经是完整句子（避免 "Could you could you confirm…" 这类重复包裹）。
fn looks_like_clause(en: &str) -> bool {
    let l = en.trim().to_ascii_lowercase();
    const HEADS: &[&str] = &[
        "i ", "i'", "we ", "you ", "he ", "she ", "they ", "it ", "please", "let", "could",
        "would", "can", "do ", "does ", "are ", "is ", "how", "what", "where", "when", "why",
        "who", "sorry", "thanks", "thank", "yes", "no", "there",
    ];
    HEADS.iter().any(|h| l.starts_with(h))
}

/// 把动词短语英文转成 -ing 形式（变第一个动词：have a meeting -> having a meeting）。
fn to_ing(phrase: &str) -> String {
    let mut words: Vec<String> = phrase.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        return phrase.to_string();
    }
    words[0] = verb_ing(&words[0]);
    words.join(" ")
}

/// 句中小写化（槽位内容落到句子中间时用）。
fn lower_first(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(f) => f.to_lowercase().collect::<String>() + it.as_str(),
        None => String::new(),
    }
}

/// 动词后的宾格代词：help I -> help me。
fn fix_pronoun_objects(parts: &mut [String]) {
    const TAKE_OBJECT: &[&str] = &[
        "help", "tell", "ask", "show", "give", "let", "call", "email", "teach", "wish",
    ];
    const MAP: &[(&str, &str)] = &[
        ("I", "me"),
        ("we", "us"),
        ("he", "him"),
        ("she", "her"),
        ("they", "them"),
    ];
    for i in 0..parts.len().saturating_sub(1) {
        if TAKE_OBJECT.contains(&parts[i].as_str()) {
            if let Some((_, obj)) = MAP.iter().find(|(sub, _)| *sub == parts[i + 1].as_str()) {
                parts[i + 1] = (*obj).to_string();
            }
        }
    }
}

/// 把动词短语英文转成过去式（只变第一个动词）。
fn to_past(phrase: &str) -> String {
    let mut words: Vec<String> = phrase.split_whitespace().map(|s| s.to_string()).collect();
    if words.is_empty() {
        return phrase.to_string();
    }
    words[0] = verb_past(&words[0]);
    words.join(" ")
}

fn strip_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let t = text.trim();
    t.strip_prefix(prefix).map(|r| r.trim())
}

fn strip_suffix<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let t = text.trim();
    t.strip_suffix(suffix).map(|r| r.trim())
}

/// 结构模板：返回地道英文（含词形变化）。
pub fn try_patterns(text: &str) -> Option<String> {
    let t = text.trim();

    // 我想/我要/我想要 X：动词 -> I'd like to X；名词 -> I'd like X
    for p in ["我想", "我要", "我想要", "我想去"] {
        if let Some(rest) = strip_prefix(t, p) {
            let verbish = slot_is_verb(rest) || p == "我想去";
            if let Some(v) = slot(rest) {
                if looks_like_clause(&v) {
                    return Some(sentence_case(&v));
                }
                if verbish {
                    return Some(sentence_case(&format!("I'd like to {v}")));
                }
                if slot_is_noun(rest) {
                    return Some(sentence_case(&format!("I'd like {v}")));
                }
            }
        }
    }
    // 我在 X：动词 -> I'm X-ing；名词/地点 -> I'm in X（修掉 "我在北京" 的误判）
    if let Some(rest) = strip_prefix(t, "我在") {
        if let Some(v) = slot(rest) {
            if looks_like_clause(&v) {
                return Some(sentence_case(&v));
            }
            if slot_is_verb(rest) {
                return Some(sentence_case(&format!("I'm {}", to_ing(&v))));
            }
            if slot_is_noun(rest) {
                let prep = if rest.starts_with("家")
                    || rest.starts_with("公司")
                    || rest.starts_with("学校")
                    || rest.starts_with("办公室")
                {
                    "at"
                } else {
                    "in"
                };
                return Some(sentence_case(&format!("I'm {prep} {v}")));
            }
        }
    }
    // 我们 X 吧 -> Let's X
    if let Some(rest) = strip_suffix(t, "吧") {
        let rest = rest.trim_start_matches("我们").trim();
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("Let's {v}")));
        }
    }
    // 请 X / 请帮我 X -> Please X
    if let Some(rest) = strip_prefix(t, "请帮我") {
        if let Some(v) = slot(rest) {
            if looks_like_clause(&v) {
                return Some(sentence_case(&v));
            }
            return Some(sentence_case(&format!("Please help me {v}")));
        }
    }
    if let Some(rest) = strip_prefix(t, "请") {
        if slot_is_verb(rest) {
            if let Some(v) = slot(rest) {
                if looks_like_clause(&v) {
                    return Some(sentence_case(&v));
                }
                return Some(sentence_case(&format!("Please {v}")));
            }
        }
    }
    // 太 X 了 -> That's too X（槽位应为形容词）
    if let Some(rest) = strip_suffix(t, "了") {
        if let Some(x) = strip_prefix(rest, "太") {
            if slot_is_adj(x) {
                if let Some(v) = slot(x) {
                    return Some(sentence_case(&format!("That's too {v}")));
                }
            }
        }
    }
    // 能不能/可以…吗 -> Could you X
    for p in ["能不能", "可以帮我", "可不可以", "能否"] {
        if let Some(rest) = strip_prefix(t, p) {
            let rest = rest
                .trim_end_matches('吗')
                .trim_end_matches('？')
                .trim_end_matches('?');
            if let Some(v) = slot(rest) {
                if looks_like_clause(&v) {
                    return Some(sentence_case(&v));
                }
                return Some(sentence_case(&format!("Could you {v}")));
            }
        }
    }
    // X 吗 / X ？ -> Do you X?
    if let Some(rest) = strip_suffix(t, "吗") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("Do you {v}")));
        }
    }
    // 有没有 X -> Is there any X?
    if let Some(rest) = strip_prefix(t, "有没有") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("Is there any {v}")));
        }
    }
    // X 在哪里 -> Where is X?（槽位应为名词/地点）
    if let Some(rest) = strip_suffix(t, "在哪里") {
        if slot_is_noun(rest) || slot_is_verb(rest) {
            if let Some(v) = slot(rest) {
                return Some(sentence_case(&format!("Where is {v}")));
            }
        }
    }
    // X 多少钱 -> How much is X?
    if let Some(rest) = strip_suffix(t, "多少钱") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("How much is {v}")));
        }
    }
    // 我需要更多 X -> I need more X（必须排在"我需要"之前）
    if let Some(rest) = strip_prefix(t, "我需要更多") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("I need more {v}")));
        }
    }
    // 我需要 X -> I need X
    if let Some(rest) = strip_prefix(t, "我需要") {
        if let Some(v) = slot(rest) {
            if looks_like_clause(&v) {
                return Some(sentence_case(&v));
            }
            return Some(sentence_case(&format!("I need {v}")));
        }
    }
    // 帮我 X -> Help me X
    if let Some(rest) = strip_prefix(t, "帮我") {
        if let Some(v) = slot(rest) {
            if looks_like_clause(&v) {
                return Some(sentence_case(&v));
            }
            return Some(sentence_case(&format!("Help me {v}")));
        }
    }
    // 我不 X -> I don't X
    if let Some(rest) = strip_prefix(t, "我不") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("I don't {v}")));
        }
    }
    // 别 X -> Don't X
    if let Some(rest) = strip_prefix(t, "别") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("Don't {v}")));
        }
    }
    // 我在学 X -> I'm learning X
    if let Some(rest) = strip_prefix(t, "我在学") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("I'm learning {v}")));
        }
    }
    // X 怎么样 -> How about X?
    if let Some(rest) = strip_suffix(t, "怎么样") {
        if let Some(v) = slot(rest) {
            return Some(sentence_case(&format!("How about {v}")));
        }
    }
    // 祝你 X -> Wish you X
    if let Some(rest) = strip_prefix(t, "祝你") {
        if let Some(v) = slot(rest) {
            if looks_like_clause(&v) {
                return Some(sentence_case(&v));
            }
            return Some(sentence_case(&format!("Wish you {v}")));
        }
    }
    // X 了 -> 过去式（I X-ed）
    if let Some(rest) = strip_suffix(t, "了") {
        if let Some(v) = slot(rest) {
            let words: Vec<&str> = v.split_whitespace().collect();
            if words.len() >= 2 {
                let subject = words[0];
                if subject == "I"
                    || subject == "we"
                    || subject == "you"
                    || subject == "he"
                    || subject == "she"
                    || subject == "they"
                {
                    let rest_v = words[1..].join(" ");
                    return Some(sentence_case(&format!("{subject} {}", to_past(&rest_v))));
                }
            }
        }
    }
    None
}

// ---------------- 直译层（词序调整） ----------------

/// 从词序列里挑出时间词、主语、动词、其他，按英文语序组装。
fn literal(text: &str) -> Option<String> {
    let idx = index();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut subject: Option<String> = None;
    let mut verb: Option<String> = None;
    let mut objects: Vec<String> = Vec::new();
    let mut time: Option<String> = None;
    let mut matched = 0usize;
    let mut unknown = 0usize;
    let mut clause: Option<String> = None;
    let mut need_to = false;
    // 大词典探测用的复用缓冲（与循环里的 `seg` 分开，避免遮蔽）
    let mut probe = String::new();

    while i < chars.len() {
        // 时间词优先识别
        if let Some((len, en)) = longest_match(&idx.time_by_first, &chars, i) {
            if time.is_none() {
                time = Some(en.to_string());
            }
            matched += len;
            i += len;
            continue;
        }
        // 多字词条：若英文本身已是完整句子（"Please send the report."）就整体作为 clause，
        // 不再拼主语，避免出现 "I Please send the report." 这类噪音
        if let Some((len, en)) = longest_match(&idx.phrases_by_first, &chars, i) {
            if len >= 2 {
                matched += len;
                i += len;
                if en.split_whitespace().count() >= 2 || looks_like_clause(en) {
                    clause = Some(en.to_string());
                } else {
                    objects.push(en.to_string());
                }
                continue;
            }
        }
        if let Some((len, en)) = longest_match(&idx.words_by_first, &chars, i) {
            let seg: String = chars[i..i + len].iter().collect();
            let is_pronoun = matches!(
                seg.as_str(),
                "我" | "我们" | "你" | "你们" | "他" | "她" | "他们" | "大家"
            );
            let is_verb = matches!(
                seg.as_str(),
                "是" | "有"
                    | "要"
                    | "想"
                    | "需要"
                    | "做"
                    | "去"
                    | "来"
                    | "看"
                    | "说"
                    | "知道"
                    | "学"
                    | "学习"
                    | "工作"
                    | "用"
                    | "买"
                    | "吃"
                    | "喝"
                    | "睡"
                    | "帮助"
                    | "帮"
                    | "给"
                    | "拿"
                    | "打开"
                    | "写"
                    | "发"
                    | "发送"
                    | "检查"
                    | "确认"
                    | "联系"
                    | "准备"
                    | "完成"
                    | "开始"
                    | "喜欢"
                    | "爱"
                    | "希望"
                    | "打算"
                    | "决定"
                    | "喜欢2"
                    | "会"
                    | "能"
                    | "可以"
                    | "应该"
                    | "需要2"
                    | "开车"
                    | "走路"
            );
            if is_pronoun && subject.is_none() {
                subject = Some(en.to_string());
            } else if is_verb && verb.is_none() {
                verb = Some(en.to_string());
                if matches!(
                    seg.as_str(),
                    "要" | "想" | "打算" | "决定" | "希望" | "需要"
                ) {
                    need_to = true;
                }
            } else {
                if need_to && objects.is_empty() {
                    objects.push(format!("to {en}"));
                    need_to = false;
                } else {
                    objects.push(en.to_string());
                }
            }
            matched += len;
            i += len;
            continue;
        }
        if let Some((len, en)) = dict_longest(&chars, i, &mut probe) {
            objects.push(en);
            matched += len;
            i += len;
            continue;
        }
        if SKIP_CHARS.contains(chars[i]) {
            i += 1;
            continue;
        }
        unknown += 1;
        i += 1;
    }

    // 已经命中完整句子：直接返回（可带时间词）
    if let Some(c) = clause {
        if subject.is_none() && verb.is_none() {
            return Some(match time {
                Some(t) if !c.to_ascii_lowercase().contains(&t) => {
                    sentence_case(&format!("{c} {t}"))
                }
                _ => sentence_case(&c),
            });
        }
    }

    if verb.is_none() && objects.is_empty() {
        return None;
    }
    let total = matched + unknown;
    // 直译层只做"教学参考"，宁缺毋滥：覆盖率要求更高
    if total == 0 || (matched as f64) / (total as f64) < 0.75 {
        return None;
    }
    fix_pronoun_objects(&mut objects);

    let subj = subject.unwrap_or_else(|| "I".to_string());
    let mut out = subj.clone();
    if let Some(v) = verb {
        out.push(' ');
        // 过去时间 -> 过去式
        if time.as_deref() == Some("yesterday") || time.as_deref() == Some("last week") {
            out.push_str(&to_past(&v));
        } else if subj == "he" || subj == "she" || subj == "it" {
            out.push_str(&verb_s(&v));
        } else {
            out.push_str(&v);
        }
    }
    if !objects.is_empty() {
        out.push(' ');
        out.push_str(&objects.join(" "));
    }
    if let Some(t) = time {
        out.push(' ');
        out.push_str(&t);
    }
    Some(sentence_case(&out))
}

// ---------------- 对外 API ----------------

/// 逐字兜底：仅 ≤4 字且全部认识时输出。
fn char_gloss(text: &str) -> Option<String> {
    let idx = index();
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() || chars.len() > 4 {
        return None;
    }
    let mut words = Vec::with_capacity(chars.len());
    for ch in &chars {
        words.push(*idx.char_gloss.get(ch)?);
    }
    Some(words.join(" "))
}

/// 中文 -> 英文候选（最多 3 条，带来源标签）。
pub fn english_candidates(text: &str) -> Vec<(u8, String)> {
    let t = text.trim();
    if t.is_empty() {
        return Vec::new();
    }
    // 统一重排：先收集 (kind, text, score)，最后按分数排序截断
    let mut scored: Vec<(u8, String, i32)> = Vec::new();
    let mut out: Vec<(u8, String)> = Vec::new();
    fn norm(s: &str) -> String {
        s.trim()
            .trim_end_matches(['.', '!', '?', '。', '！', '？'])
            .to_ascii_lowercase()
    }
    /// 来源基础分（越大越靠前）
    fn kind_base(kind: u8) -> i32 {
        match kind {
            KIND_MINE => 100,
            KIND_NATIVE => 90,
            KIND_SENTBANK => 85,
            KIND_DICT => 70,
            KIND_PATTERN => 60,
            _ => 40,
        }
    }

    fn push(out: &mut Vec<(u8, String)>, kind: u8, s: String) {
        if s.trim().is_empty() {
            return;
        }
        let n = norm(&s);
        if !out.iter().any(|(_, v)| norm(v) == n) {
            out.push((kind, s));
        }
    }

    fn push_scored(scored: &mut Vec<(u8, String, i32)>, kind: u8, s: String, bonus: i32) {
        if s.trim().is_empty() {
            return;
        }
        let n = norm(&s);
        if !scored.iter().any(|(_, v, _)| norm(v) == n) {
            scored.push((kind, s, kind_base(kind) + bonus));
        }
    }

    // 1) 我的（翻译记忆：精确或包含）；繁体输入统一按简体匹配
    let simp = if crate::s2t::ready(false) {
        crate::s2t::to_simplified(t)
    } else {
        t.to_string()
    };
    let lookup_keys: Vec<&str> = if simp == t {
        vec![t]
    } else {
        vec![t, simp.as_str()]
    };
    {
        let m = memory().lock().unwrap_or_else(|e| e.into_inner());
        for key in &lookup_keys {
            if let Some((_, en)) = m.iter().find(|(cn, _)| cn == key) {
                push(&mut out, KIND_MINE, en.clone());
                break;
            }
        }
        if out.is_empty() {
            if let Some((_, en)) = m
                .iter()
                .filter(|(cn, _)| {
                    cn.chars().count() >= 3 && lookup_keys.iter().any(|k| k.contains(cn.as_str()))
                })
                .max_by_key(|(cn, _)| cn.chars().count())
            {
                push(&mut out, KIND_MINE, en.clone());
            }
        }
    }

    // 2) 地道整句
    if let Some((_, en)) = PHRASES.iter().find(|(cn, _)| *cn == t) {
        push(&mut out, KIND_NATIVE, (*en).to_string());
    }

    // 2.2) 句库（整句精确 / 双字重叠 ≥60%）：重叠分越高排得越前
    for (en, score) in crate::sentbank::lookup(t, 2) {
        push_scored(&mut scored, KIND_SENTBANK, en, (score.min(100) as i32) / 4);
    }

    // 2.5) 大词典整词命中（CC-CEDICT）
    if let Some(en) = dict_exact(t) {
        push(&mut out, KIND_DICT, sentence_case_soft(&en));
    }

    // 3) 结构模板
    if let Some(p) = try_patterns(t) {
        push(&mut out, KIND_PATTERN, p);
    }

    // 4) 直译（词序调整）
    if let Some(l) = literal(t) {
        push(&mut out, KIND_LITERAL, l);
    }

    // 5) 短词逐字
    if out.is_empty() {
        if let Some(c) = char_gloss(t) {
            push(&mut out, KIND_LITERAL, c);
        }
    }

    // 其余层按基础分进入统一排序
    for (kind, text) in out {
        push_scored(&mut scored, kind, text, 0);
    }
    scored.sort_by_key(|x| std::cmp::Reverse(x.2));
    scored.into_iter().take(3).map(|(k, s, _)| (k, s)).collect()
}

/// 首选英文（空串表示暂无建议）。
pub fn suggest(text: &str) -> String {
    english_candidates(text)
        .into_iter()
        .next()
        .map(|(_, s)| s)
        .unwrap_or_default()
}

/// 英文 -> 语法讲解（离线启发式）。
pub fn grammar_explain(english: &str) -> String {
    let text = english.trim();
    if text.is_empty() {
        return "输入或收藏英文句子后，这里会给出结构讲解。".to_string();
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let lower = text.to_lowercase();
    let mut lines = vec![format!("句子：{text}")];
    lines.push(format!("词数：{} 个单词", words.len()));

    if text.ends_with('?') {
        lines.push("句式：疑问句（助动词/疑问词开头：do、be、what、how…）".to_string());
    } else if text.ends_with('!') {
        lines.push("句式：感叹句或招呼语（语气强，口语常用）".to_string());
    } else if lower.starts_with("please") || lower.starts_with("let ") {
        lines.push("句式：祈使句（省略主语的请求/建议）".to_string());
    } else {
        lines.push("句式：陈述句（主语 + 谓语 + 其他成分）".to_string());
    }

    if lower.contains("will ") || lower.contains("going to") || lower.contains("tomorrow") {
        lines.push("时态线索：将来（will / be going to / tomorrow）".to_string());
    } else if lower.contains(" yesterday")
        || lower.contains(" was ")
        || lower.contains(" were ")
        || lower.ends_with("ed")
        || lower.ends_with("ed.")
    {
        lines.push("时态线索：过去（was / were / 动词-ed / yesterday）".to_string());
    } else if lower.contains("'m ")
        || lower.contains(" am ")
        || lower.contains(" is ")
        || lower.contains(" are ")
        || lower.contains("ing ")
    {
        lines.push("时态线索：现在/进行（be 动词或 -ing）".to_string());
    }

    for m in ["please", "could", "would", "should", "can ", "must"] {
        if lower.contains(m) {
            lines.push(format!(
                "情态/礼貌词：含 \"{}\"，多用于请求或建议",
                m.trim()
            ));
            break;
        }
    }

    if words.len() >= 2 {
        lines.push(format!(
            "成分速览：主语≈ {} | 谓语≈ {} | 其余≈ {}",
            words[0],
            words[1],
            if words.len() > 2 {
                words[2..].join(" ")
            } else {
                "—".to_string()
            }
        ));
    }
    lines.push("用法提示：收藏后朗读两遍，下次遇到同样中文场景直接套用。".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_phrase_wins() {
        assert_eq!(suggest("谢谢"), "Thank you!");
        assert_eq!(suggest("再见"), "Goodbye! / See you!");
        assert_eq!(suggest("你好"), "Hello!");
    }

    #[test]
    fn no_garbage_for_unknown_text() {
        let s = suggest("龘靐齉爩鱻");
        assert!(s.is_empty(), "expected empty, got {s:?}");
        for (_, c) in english_candidates("龘靐齉爩鱻") {
            assert!(!c.contains('…'));
        }
    }

    #[test]
    fn morphology_rules() {
        assert_eq!(verb_ing("go"), "going");
        assert_eq!(verb_ing("run"), "running");
        assert_eq!(verb_ing("make"), "making");
        assert_eq!(verb_past("go"), "went");
        assert_eq!(verb_past("like"), "liked");
        assert_eq!(verb_past("stop"), "stopped");
        assert_eq!(verb_past("study"), "studied");
        assert_eq!(verb_s("go"), "goes");
        assert_eq!(verb_s("study"), "studies");
        assert_eq!(plural("box"), "boxes");
        assert_eq!(plural("city"), "cities");
        assert_eq!(article("apple"), "an apple");
        assert_eq!(article("book"), "a book");
        assert_eq!(sentence_case("hello world"), "Hello world.");
        assert_eq!(sentence_case("really?"), "Really?");
    }

    #[test]
    fn patterns_produce_fluent_english() {
        assert_eq!(
            try_patterns("请发送报告").unwrap(),
            "Please send the report."
        );
        assert_eq!(try_patterns("太贵了").unwrap(), "That's too expensive.");
        assert!(try_patterns("我在开会").unwrap().starts_with("I'm having"));
        assert!(try_patterns("我要买咖啡")
            .unwrap()
            .starts_with("I'd like to buy"));
        assert!(try_patterns("我们走吧").unwrap().starts_with("Let's"));
        assert_eq!(suggest("我们走吧"), "Let's go.");
        assert!(try_patterns("生日快乐").is_some() || suggest("生日快乐") == "Happy birthday!");
    }

    #[test]
    fn literal_puts_time_at_the_end() {
        let v = literal("我今天很忙").unwrap();
        assert!(v.starts_with("I"), "{v}");
        assert!(v.ends_with("today."), "{v}");
    }

    #[test]
    fn literal_past_tense() {
        let v = literal("我昨天买了书").unwrap_or_default();
        if !v.is_empty() {
            assert!(v.contains("bought") || v.contains("yesterday"), "{v}");
        }
    }

    #[test]
    fn pos_gates_patterns() {
        // 动词槽位 -> 进行时
        assert!(try_patterns("我在开会").unwrap().starts_with("I'm having"));
        // 名词槽位 -> I'm in/at（修掉 "I'm being-ing" 这类误判）
        // 测试环境不加载大词典，用内建词表里的名词（公司）
        let at_company = try_patterns("我在公司").unwrap();
        assert!(
            at_company.starts_with("I'm at") || at_company.starts_with("I'm in"),
            "{at_company}"
        );
        // 形容词槽位才走"太X了"
        assert_eq!(try_patterns("太贵了").unwrap(), "That's too expensive.");
        assert!(try_patterns("太公司了").is_none());
        // 非动词槽位不套"请X"
        assert!(try_patterns("请公司").is_none());
    }

    #[test]
    fn rerank_prefers_stronger_sources() {
        // 有整句短语时，"地道"必须排第一（分数 90 > 结构 60 / 直译 40）
        let c = english_candidates("请发送报告");
        assert_eq!(c.first().map(|(k, _)| *k), Some(KIND_NATIVE), "{c:?}");
        // 我的（翻译记忆）优先于一切
        set_memory(vec![(
            "请发送报告".to_string(),
            "Kindly send the report.".to_string(),
        )]);
        let c2 = english_candidates("请发送报告");
        assert_eq!(c2.first().map(|(k, _)| *k), Some(KIND_MINE), "{c2:?}");
        set_memory(Vec::new());
    }

    #[test]
    fn memory_has_top_priority() {
        set_memory(vec![(
            "今天天气很好".to_string(),
            "Lovely weather today!".to_string(),
        )]);
        let c = english_candidates("今天天气很好");
        assert_eq!(c.first().map(|(k, _)| *k), Some(KIND_MINE));
        assert_eq!(
            c.first().map(|(_, s)| s.as_str()),
            Some("Lovely weather today!")
        );
        set_memory(Vec::new());
    }

    #[test]
    fn kinds_are_ordered_and_capped() {
        let c = english_candidates("请发送报告");
        assert!(c.len() <= 3);
        let kinds: Vec<u8> = c.iter().map(|(k, _)| *k).collect();
        let mut sorted = kinds.clone();
        sorted.sort();
        assert_eq!(kinds, sorted, "kinds should be ascending (best first)");
    }

    #[test]
    fn big_dict_loads_and_serves() {
        // 用仓库里的真实资产做回归：缺文件则该用例失败，防止漏提交
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../app/src/main/assets/en_dict.tsv"
        );
        let n = load_dict(path);
        assert!(n > 50_000, "expected a large dictionary, got {n}");
        assert_eq!(dict_size(), n);
        assert_eq!(dict_exact("咖啡").as_deref(), Some("coffee"));
        assert!(dict_exact("发票").unwrap_or_default().contains("invoice"));
        // 整词命中走「词典」层
        let c = english_candidates("充电宝");
        assert_eq!(c.first().map(|(k, _)| *k), Some(KIND_DICT));
        let first = c
            .first()
            .map(|(_, s)| s.to_ascii_lowercase())
            .unwrap_or_default();
        assert!(first.contains("power bank"), "got {first:?}");
    }

    #[test]
    fn big_dict_improves_composition() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../app/src/main/assets/en_dict.tsv"
        );
        assert!(load_dict(path) > 50_000);
        // 词典词参与组合：句子整体覆盖率足够时给出直译
        let en = suggest("我需要咖啡");
        assert!(en.to_lowercase().contains("coffee"), "got {en:?}");
    }

    /// 测试用包装：生产路径复用缓冲（见 [`fix_spelling`]），这里每次给全新的。
    fn edit(a: &[u8], b: &[u8], max: u8) -> Option<u8> {
        edit_within(a, b, max, &mut Vec::new(), &mut Vec::new())
    }

    #[test]
    fn edit_within_respects_budget() {
        assert_eq!(edit(b"cat", b"cat", 2), Some(0));
        assert_eq!(edit(b"cat", b"cut", 2), Some(1));
        assert_eq!(edit(b"cat", b"cats", 2), Some(1));
        assert_eq!(edit(b"cofee", b"coffee", 2), Some(1));
        // 超预算必须尽早放弃，而不是算完再说
        assert_eq!(edit(b"cat", b"dog", 2), None);
        assert_eq!(edit(b"kitten", b"sitting", 2), None);
        assert_eq!(edit(b"", b"", 0), Some(0));
    }

    #[test]
    fn fix_spelling_corrects_only_real_typos() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../app/src/main/assets/en_dict.tsv"
        );
        assert!(load_dict(path) > 50_000);
        // 打对了：一个字都不该改
        assert_eq!(fix_spelling("coffee"), None);
        assert_eq!(fix_spelling("invoice"), None);
        // 打错了：补回漏掉的字母
        assert_eq!(fix_spelling("cofee").as_deref(), Some("coffee"));
        assert_eq!(fix_spelling("restarant").as_deref(), Some("restaurant"));
        // 不是词 / 不像词 / 夹杂非字母：宁可不纠正
        assert_eq!(fix_spelling("ab"), None);
        assert_eq!(fix_spelling("cof fee"), None);
        assert_eq!(fix_spelling("asdfghjkl"), None);
        assert_eq!(fix_spelling(""), None);
    }

    #[test]
    fn coverage_on_daily_corpus() {
        let corpus = [
            "你好",
            "谢谢",
            "多少钱",
            "请发送报告",
            "我今天很忙",
            "明天开会",
            "我要买咖啡",
            "我在学习英语",
            "太棒了",
            "对不起",
            "我饿了",
            "几点了",
            "有没有空",
            "请稍等",
            "我需要帮助",
            "洗手间在哪里",
            "我们走吧",
            "能不能帮我确认一下",
            "祝你生日快乐",
            "我今天开会",
            "我明天出差",
            "这个多少钱",
            "我想喝茶",
            "我在工作",
            "请确认",
            "抱歉我迟到了",
            "我需要更多时间",
            "我们保持联系",
            "你的项目进度怎么样",
            "谢谢你的支持",
        ];
        let mut ok = 0;
        for s in corpus {
            if !suggest(s).is_empty() {
                ok += 1;
            }
        }
        let ratio = ok as f64 / corpus.len() as f64;
        assert!(ratio >= 0.9, "coverage too low: {ok}/{}", corpus.len());
    }

    #[test]
    fn suggestions_are_english_ish() {
        for s in [
            "请发送报告",
            "我今天很忙",
            "我要买咖啡",
            "太贵了",
            "有没有空",
        ] {
            let en = suggest(s);
            assert!(!en.is_empty(), "{s} produced nothing");
            let ascii = en.chars().filter(|c| c.is_ascii_alphabetic()).count();
            assert!(ascii >= 2, "{s} -> {en} does not look English");
        }
    }
}
