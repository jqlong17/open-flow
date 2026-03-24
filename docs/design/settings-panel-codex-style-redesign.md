# Open Flow Settings Panel Redesign

目标：把现有设置面板从“功能完整的原生表单”提升到“更像一个现代工作台”的观感，整体借鉴用户给出的参考图风格，但保持 Open Flow 现有的信息架构与 macOS 原生交互优势。

## 1. 设计目标

本次改版不追求炫技，而是追求三件事：

- 更强的视觉秩序：让用户一眼看出导航、主区、当前任务和次级信息的层次
- 更好的高级感：减弱表单堆叠感，增加卡片化、留白和预览驱动
- 更适合 Open Flow：突出“语音输入工作流”而不是通用设置页

参考方向来自截图中的这些气质：

- 左侧柔和淡色导航
- 右侧大圆角主工作区
- 大标题 + 预览卡 + 参数卡的组合
- 浅描边、轻阴影、低饱和中性色
- 默认安静，交互时再强调

## 2. 对现有界面的判断

当前实现文件在 [settings-app/Sources/OpenFlowSettings/ContentView.swift](/Users/ruska/project/open-flow/settings-app/Sources/OpenFlowSettings/ContentView.swift)。

现状优点：

- 信息完整，功能覆盖已经比较全
- Section 组织清晰
- 原生 SwiftUI 组件稳定
- Header、底栏、日志区等功能性区域都已具备

现状问题：

- `TabView + 默认 tabItem` 更像系统偏好设置，缺少品牌感
- 所有 `SettingsSection` 视觉权重接近，用户难以快速判断主次
- 主要页面是“表单堆叠”，缺少结果预览或状态总览
- Provider / Model / Test / Logs 分页割裂，缺少跨模块的总体状态感
- 底部 Save 条与页面主体有点脱节，不像同一个视觉系统

## 3. 新的整体方向

整体采用“两栏工作台”布局：

- 左栏：固定导航，淡蓝灰背景，带少量品牌感
- 右栏：大圆角白色内容区，顶部是页标题和摘要，主体是卡片化内容

推荐风格关键词：

- Calm
- Native
- Airy
- Rounded
- Tool-like

推荐避免：

- 过重阴影
- 过深对比度
- 太多彩色按钮同时出现
- 把每个设置项都做成独立强视觉块

## 4. 信息架构重组建议

现有 Tab 可以保留，但视觉层级要升级。建议重构为以下导航结构：

### 4.1 左侧导航

- General
- Recognition
- Models
- Permissions
- Diagnostics

对应关系：

- `General`：Hotkey、Trigger Mode、Text Processing、Personal Vocabulary 入口
- `Recognition`：Provider、Groq 配置、本地/云端说明
- `Models`：本地模型状态、预设、下载、路径、下载输出
- `Permissions`：Accessibility、Input Monitoring、Microphone
- `Diagnostics`：Hotkey Test、Logs

这样比现在更符合用户心智：

- 先看日常使用相关设置
- 再看识别方案
- 再看模型资产
- 然后是权限
- 最后是测试和日志

## 5. 页面级布局方案

### 5.1 整体窗口

建议采用：

- 左侧导航宽度：`220-240`
- 右侧主区最小宽度：`760+`
- 整体窗口圆角：大于当前，偏 `24-28`
- 外围背景：冷白灰带一点蓝

布局关系：

```text
┌──────────────────────────────────────────────────────┐
│ soft sidebar │ large rounded content surface         │
│              │ ┌ page header                       ┐ │
│ nav items    │ ├ preview / summary card           ┤ │
│ status chip  │ ├ primary settings card            ┤ │
│ app actions  │ ├ secondary settings card          ┤ │
│              │ └ sticky save/status bar           ┘ │
└──────────────────────────────────────────────────────┘
```

### 5.2 顶部页眉

每个页面不只是显示标题，还要有“当前页的一句解释”。

示例：

- `Recognition`
  - 标题：`Speech Recognition`
  - 副标题：`Choose between fully local transcription and Groq Whisper, then tune the provider behavior.`

- `Models`
  - 标题：`Local Models`
  - 副标题：`Manage SenseVoice presets, download status, and storage path.`

这一步很像参考图里的 `Appearance` 标题区，是页面气质的关键。

## 6. 关键借鉴点如何落到 Open Flow

### 6.1 借鉴点一：预览卡取代纯说明文

参考图最值得学的是顶部预览卡。

在 Open Flow 里建议改造成“状态预览卡”，而不是主题预览：

#### Recognition 页顶部预览卡

显示：

- 当前 Provider
- 当前模型
- 是否本地处理
- 是否已配置云端 Key
- 预期体验标签：`Privacy First` / `Cloud Faster Setup`

可以做成左右对比式或单卡式摘要。

#### Models 页顶部预览卡

显示：

- 当前使用预设
- 模型是否已下载
- 模型所在路径
- 模型大小摘要
- 一个主要 CTA：`Download` / `Re-download`

这会比现在用户进入后先看到一串行项目更高级。

### 6.2 借鉴点二：设置项卡片化

建议把当前 `SettingsSection` 从“标题 + 灰底内容”升级成两层结构：

- 外层：页面中的分组卡
- 内层：具体设置行

每张卡内采用“行式设置”：

- 左边：标签 + 一句说明
- 右边：控件

适合的行组件：

- segmented control
- toggle
- compact picker
- inline button
- path pill
- progress row

### 6.3 借鉴点三：降低默认控件噪音

当前有些区域的 `Divider` 和原生表单块叠得比较密，建议改成：

- 少用粗显性的 `Divider`
- 更多依赖留白和轻边框分隔
- 每个卡片内部只在必要处用极浅分隔线

目标是让页面更“呼吸”，不是更“切碎”。

## 7. 视觉语言规范

### 7.1 色彩

建议用一套更克制的配色，而不是直接依赖系统默认蓝：

- Sidebar background: `#EAF1FB` 附近的冷浅蓝
- Main background: `#F7F8FA`
- Content surface: `#FFFFFF`
- Border: `#E7EAF0`
- Primary text: `#111418`
- Secondary text: `#667085`
- Accent blue: `#3A94F6`
- Success: 偏柔和绿色，不要荧光绿
- Warning: 偏杏黄，不要纯橙

原则：

- 90% 用中性色
- 强调蓝只用于当前态、主要 CTA、开关激活、下载进度

### 7.2 圆角

统一圆角会显著提升完整度：

- 外层主容器：`24-28`
- 卡片：`18-20`
- 小胶囊控件：`12-14`
- 输入框与按钮：统一至少 `10-12`

### 7.3 阴影

影子要轻：

- 主内容区：非常轻的容器阴影
- 卡片：更多依赖描边，不靠阴影

### 7.4 字体层级

建议建立固定层级：

- Page Title：`28-32 semibold`
- Card Title：`17-19 semibold`
- Row Label：`14-15 medium`
- Row Description：`12-13 regular`
- Inline value：`12-13 medium/regular`
- Monospace blocks：仅日志、路径、下载输出使用

## 8. 组件级改造建议

### 8.1 Sidebar

建议新增一个自定义 Sidebar，而不是直接依赖 `TabView.tabItem`。

应该包含：

- 顶部返回或应用状态入口
- 导航项图标 + 文案
- 当前项采用浅底高亮
- 左栏底部放 daemon 状态与快速动作

导航项状态：

- 默认：透明
- hover：极浅高亮
- active：浅灰蓝底 + 更高字重

### 8.2 Page Header

统一为：

- 大标题
- 一句说明
- 右侧轻量动作，如 `Import`, `Reveal Config`, `Refresh`, `Open App`

### 8.3 Summary Card

每页顶部加入一张“摘要卡”。

例如 `General` 页可以展示：

- 当前热键
- Trigger 模式
- 中文转换状态
- 个性词表纠错是否开启

这张卡的意义是帮助用户“先看全局，再改细节”。

### 8.4 Setting Rows

推荐统一成以下格式：

```text
Label
Supportive one-line description                    [control]
```

而不是只有：

```text
Label                                    [control]
```

这能让页面看起来明显更“产品化”。

### 8.5 状态标签

用柔和的状态 pill 替代现在部分生硬的 `Label`：

- `Ready`
- `Downloading`
- `Not Downloaded`
- `Granted`
- `Missing`

状态 pill 要低饱和、轻背景、深一点的文字，不要每个都用实心色。

### 8.6 路径显示

模型路径和配置路径不要像普通文字。

建议做成：

- 胶囊样式 path pill
- 内含图标和截断路径
- hover 时可复制

### 8.7 日志和下载输出

日志区现在是纯 `TextEditor`，功能够，但视觉偏粗糙。

建议：

- 放进单独卡片
- 顶部带标题与操作栏
- 代码/日志区背景使用非常浅的中性灰
- 圆角略大一点

## 9. 各页面具体改造草案

### 9.1 General

页面结构建议：

1. Summary Card
2. Input Behavior Card
3. Text Processing Card
4. Personal Vocabulary Card

重点：

- 热键和 Trigger Mode 放一张主卡里
- 中文转换单独一张卡
- Personal Vocabulary 改成更像“功能入口卡”，而不是普通设置项

### 9.2 Recognition

页面结构建议：

1. Provider Summary Card
2. Provider Choice Card
3. Local Provider Detail Card 或 Groq Detail Card

重点：

- 顶部先清楚说明“local vs cloud”
- Provider 切换不只是一条 segmented，要配一段说明和能力标签
- 本地模式突出隐私
- Groq 模式突出配置简易性与模型选择

### 9.3 Models

页面结构建议：

1. Model Status Hero Card
2. Preset and Download Card
3. Storage Path Card
4. Download Output Card

重点：

- 下载按钮做成主 CTA
- 当前预设和状态视觉上更强
- 路径、大小、下载状态不要混在同一层

### 9.4 Permissions

页面结构建议：

1. Permission Overview Card
2. 三个权限项作为单独 row
3. 底部加一个 restart hint

重点：

- 每个权限项做成“检查项”，更接近系统 onboarding
- 缺失权限时，行动按钮更明确

### 9.5 Diagnostics

页面结构建议：

1. Hotkey Test Card
2. Event Log Card
3. Daemon Log Card

重点：

- Test 和 Logs 放在同一信息语境中
- 现在单独分页的日志可并入 Diagnostics，减少割裂感

## 10. 与现有代码结构的映射

建议保留 `ContentView.swift` 作为主入口，但新增几层轻量组件：

- `SettingsShell`
- `SettingsSidebar`
- `SettingsPageHeader`
- `SettingsSummaryCard`
- `SettingsRow`
- `StatusPill`
- `PathPill`

可继续保留现有 `SettingsSection`，但建议逐步退场，改成更扁平的组件体系。

推荐文件拆分：

```text
settings-app/Sources/OpenFlowSettings/
  ContentView.swift
  Components/
    SettingsShell.swift
    SettingsSidebar.swift
    SettingsPageHeader.swift
    SettingsSummaryCard.swift
    SettingsRow.swift
    StatusPill.swift
    PathPill.swift
```

## 11. 分阶段落地建议

### Phase 1：结构换骨，不大动功能

先做：

- 自定义 Sidebar 替代默认 TabView 外观
- 右侧大圆角内容容器
- Page Header
- 统一卡片风格

这一阶段就能让气质接近参考图 60%。

### Phase 2：加入摘要卡和状态视觉

再做：

- Recognition Summary Card
- Models Hero Card
- Status Pill
- Path Pill

这一阶段能把“好看”变成“更像专业工具”。

### Phase 3：细节打磨

最后再做：

- 更细的间距系统
- 更统一的控件高度
- 日志和下载输出美化
- 动画与过渡

## 12. 设计结论

最值得借鉴的不是参考图的表面颜色，而是这三个结构性思路：

- 用 Sidebar + 大内容区建立明确层级
- 用摘要卡让设置页先展示“当前状态”再展示“可编辑字段”
- 用统一的圆角、浅边框、低噪音控件语言建立高级感

对 Open Flow 来说，最适合的落地方式不是完全照抄，而是把这种风格转译成一个“更柔和、更有工作台感的 macOS 设置面板”。

---

如果进入实现阶段，建议先从 `Recognition` 和 `Models` 两页开始，因为这两页最能拉开观感差距，也最接近参考图的“预览 + 参数编辑”模式。
