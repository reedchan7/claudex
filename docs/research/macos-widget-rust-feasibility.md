# macOS 桌面 Widget 展示 AI Agent 用量:Rust 可行性调研

> 调研日期:2026-08-05。调研对象:claudex(Rust CLI,读取各 AI agent 本地 OAuth token,调用未公开 usage API 渲染终端进度条)。
> 问题:能否继续用 Rust 做一个 macOS 桌面 Widget,定时刷新展示指定 agent 的用量?
> 标注约定:【官方】= Apple/crate 官方文档明确说明;【源码】= 直接读 crate 源码确认;【社区】= 社区实践/二手资料;【推断】= 基于证据的推论。

## TL;DR 结论表

| 路线 | Rust 代码占比 | 需要 Xcode/Swift | 需要付费开发者账号($99/年) | 刷新实时性上限 | 工程复杂度 | 结论 |
|---|---|---|---|---|---|---|
| A. WidgetKit 原生(Swift 壳 + Rust core) | 数据层 ~100%,UI 0% | 必须 | App Groups 需要;ad-hoc 可绕开但有坑 | 约 15–60 分钟(系统刷新预算) | 高 | 可行,但 Rust 只能写数据层,UI 必须 SwiftUI |
| B. egui/winit 悬浮窗(伪 widget) | 100% | 不需要 | 不需要 | 任意(秒级~分钟级) | 低–中 | **推荐 MVP** |
| C. Tauri v2 悬浮窗(伪 widget) | 后端 100%,UI 是 Web 前端 | 不需要 | 不需要 | 任意 | 中 | 可行,代价是引入 Web 工具链 |
| D. Tauri + tauri-plugin-widgets(真 WidgetKit) | 逻辑 100%,Swift 由插件从 JSON 生成 | 必须(sidecar Xcode 工程) | ad-hoc 可跑本机;App Groups 需付费 | 同 A 受限 | 中–高 | 想做真 WidgetKit 时的省事选项 |
| E. tray-icon 菜单栏(可叠加在 B/C 上) | 100% | 不需要 | 不需要 | 任意 | 低 | 推荐与 B 并行 |

一句话:**"纯 Rust 做苹果官方桌面 Widget(WidgetKit)"不存在**——WidgetKit 的 UI 只能是 Swift/SwiftUI 代码;但"纯 Rust 做视觉等同的桌面悬浮 widget"完全可行,且工程量和分发摩擦都小一个数量级。

---

## 1. 路线 A:WidgetKit 原生详查

### 1.1 必须有多少 Swift(无法绕开的部分)

一个 macOS WidgetKit widget 的最小组成:

1. **宿主 App target**:哪怕只是一个空壳 `.app`,widget extension 必须打包在 `Contents/PlugIns/*.appex` 里,由系统守护进程(pkd/chronod)从**已安装的 app bundle**中加载【社区,见 [CodexBar widgets.md](https://raw.githubusercontent.com/steipete/CodexBar/main/docs/widgets.md) 的 PlugInKit 排障清单】。
2. **Widget Extension target**:`TimelineProvider` 协议实现(`placeholder`/`getSnapshot`/`getTimeline`)+ SwiftUI `View` + `@main` 的 `WidgetBundle`【官方,见 [TimelineProvider 文档](https://developer.apple.com/documentation/widgetkit/timelineprovider)】。
3. `NSExtensionPointIdentifier = com.apple.widgetkit-extension` 的 Info.plist 与 entitlements。

参考实现 [dependentsign/ClaudeUsageWidget](https://github.com/dependentsign/ClaudeUsageWidget)(纯 Swift,MIT):宿主 App 约百行(配置表单),Extension 里 API 请求+视图约几百行。即"最小 Swift 壳"量级在 **200–500 行 Swift**,其中视图和 TimelineProvider 协议部分是硬约束,无法用 Rust 替代。

### 1.2 Rust 能占多少:staticlib + FFI 有成熟先例

把网络请求、token 管理、数据解析放到 Rust,Swift 只做绑定和渲染,是可行且有官方路径的:

- Mozilla [uniffi 的 Xcode 集成文档](https://mozilla.github.io/uniffi-rs/latest/swift/xcode.html)明确给出流程:cargo 编译 staticlib → `uniffi-bindgen` 生成 Swift 绑定 → 桥接头文件链入 target【官方】。uniffi 支持 async,适合包一层 tokio 运行时做 HTTP。
- 更轻的手写 C ABI / [swift-bridge](https://github.com/chinedufn/swift-bridge) 也可行【社区】。

注意:extension 进程由系统按需唤起、随时杀死,Rust 侧应保持**无状态函数式调用**(传入配置,返回 JSON 快照),不要依赖常驻 tokio runtime 或全局状态【推断】。

### 1.3 widget 与主程序的数据共享

- 标准机制:**App Groups** 共享容器(`containerURL(group)` 写 JSON)或 App Group `UserDefaults` suite。CodexBar 即采用"主 App 刷新管线写紧凑 JSON 快照到 app-group 容器,widget 只读渲染"的架构【社区一手,[CodexBar widgets.md](https://raw.githubusercontent.com/steipete/CodexBar/main/docs/widgets.md)】。
- **App Groups entitlement 需要真实 Team ID(付费 Apple Developer Program)**;tauri-plugin-widgets 的签名对照表明确:只有 ad-hoc 签名时不能用 App Groups,得退化为 `widgetContainer` 方案——主 App(须未沙盒化)直接写 extension 容器目录 `~/Library/Containers/<appex-id>/Data/widget_data.json`【社区一手,[tauri-plugin-widgets macOS setup](https://raw.githubusercontent.com/s00d/tauri-plugin-widgets/main/docs/guide/setup/macos.md)】。
- ClaudeUsageWidget 走了第三条路:extension 沙盒内用 `getpwuid(getuid())` 解析真实 home 目录直接读 `~/.claude/claude-usage-widget.json`【社区一手,见其 README】。这依赖 ad-hoc 重签名后沙盒文件访问不受限的灰色地带,脆弱且随时可能被系统收紧【推断】。

### 1.4 刷新机制与预算(决定"实时"上限的核心约束)

- WidgetKit 的刷新由系统全权调度:provider 返回带 `reloadPolicy` 的 timeline,系统按预算决定是否唤起 extension。**"用户频繁查看的 widget,每日预算典型为 40–70 次刷新,约等于每 15–60 分钟一次"**【官方,[Keeping a widget up to date](https://developer.apple.com/documentation/widgetkit/keeping-a-widget-up-to-date)】。
- 预算按用户实际查看频率动态分配,不查看则更少;无节制的 reload 请求会被系统静默丢弃【社区,[Swift Senpai](https://swiftsenpai.com/development/refreshing-widget/)、[dzionis.by 的预算分析](https://dzionis.by/writing/widgetkit-timeline-budget.html)】。
- 单次 timeline 生成约 30 秒 CPU 上限、单次 timeline 条目数经验上限 ~250【社区实测,[Stack Overflow](https://stackoverflow.com/questions/69520200/limit-to-widgetkit-timeline-entries)】。
- 实践结论:指望 widget 自己"每 5 分钟轮询 API"不可行;正确姿势是**宿主 App 常驻轮询 → 写共享快照 → 调 `WidgetCenter.reloadTimelines` 让 widget 重渲染**。CodexBar 正是这么做的,并且刻意把 widget 快照/重载频率钳制到 **5 分钟下限**,"以免耗尽系统管理的刷新预算"【社区一手,CodexBar widgets.md】。ClaudeUsageWidget 宣称的 "Auto-refresh every 5 minutes" 离开了宿主推送基本不可达【推断】。
- 即:**WidgetKit 路线的"实时"上限约为分钟级(5–15 分钟),秒级刷新想都不要想**。

### 1.5 macOS 桌面放置机制

macOS 14 Sonoma 起,widget 可放到桌面:右键桌面 → "编辑小组件"(Edit Widgets)→ 从图库拖到桌面;也支持通知中心摆放和通过 Continuity 使用 iPhone 的 widget【社区/媒体,[Setapp 指南](https://setapp.com/how-to/widgets-on-mac)、[WWDC23 报道](https://apple.slashdot.org/story/23/06/05/2055258/apple-announces-macos-sonoma-with-desktop-widgets-and-game-mode)】。关键前提:**widget 只从 /Applications 下已安装、签名有效的 .app 中注册出现**(tauri-plugin-widgets 文档:"Addable widgets appear only after you build the app and move the .app to /Applications";CodexBar 的 pkd/chronod 排障流程也印证签名失败会让 widget 在图库中消失)【社区一手】。

### 1.6 签名与分发(不进 App Store)

- Apple 官方支持 [Developer ID + 公证(notarization)在 App Store 外分发](https://help.apple.com/xcode/mac/current/en.lproj/dev033e997ca.html),app 内嵌的 extension 一并签名公证【官方】。公证需要付费开发者账号【社区,[Ask Different](https://apple.stackexchange.com/questions/388554/is-a-paid-apple-developer-account-required-for-notarizing-macos-apps)】。
- 本机自用:ad-hoc 签名(`codesign --sign -`)可以让 widget 出现在图库并工作(tauri-plugin-widgets 签名表:Ad-hoc → Widget visible: Yes, Local only;ClaudeUsageWidget 的安装步骤也是 xcodebuild 后 ad-hoc 重签)【社区一手】。
- 工具链硬依赖:Xcode(或至少 Xcode CLT + xcodegen),无法纯 cargo 构建【社区一手,两个参考项目均如此】。

### 1.7 路线 A 小结

Rust 可以承担 token 管理、HTTP、解析(uniffi/staticlib),但 **widget UI、TimelineProvider、Xcode 工程、签名公证这条 Apple 流水线一行都省不掉**。"用 Rust"的占比约是逻辑层的 80%,但整体工程复杂度由 Apple 侧决定:新增 Xcode 工程、双 target、entitlements、(建议)付费账号、打包脚本,以及长期跟随 macOS/Xcode 升级的维护税。对当前"裸二进制 + install.sh"的分发形态是范式级改变。

---

## 2. 路线 C/D:Tauri v2

### 2.1 悬浮窗能力(伪 widget)

Tauri v2 的窗口 API 逐项核对【官方,[WindowBuilder](https://docs.rs/tauri/latest/tauri/window/struct.WindowBuilder.html)、[Window Customization 指南](https://v2.tauri.app/learn/window-customization/)】:

| 需求 | Tauri 能力 | macOS 备注 |
|---|---|---|
| 无边框 | `decorations(false)` | 支持 |
| 透明背景 | `transparent(true)` | 需 `macos-private-api` feature |
| 桌面层级(沉底) | `always_on_bottom(true)` | 桌面 API,支持 |
| 鼠标穿透 | `set_ignore_cursor_events(true)` | 经 tao → `setIgnoresMouseEvents`【源码,tao-0.35.3】 |
| 全工作区可见 | `visible_on_all_workspaces(true)` | 明确支持 macOS(Windows 不支持) |
| 隐藏 Dock 图标 | `skip_taskbar` **macOS 不支持**;需 `set_activation_policy(Accessory)` | tao 的 `ActivationPolicy::Accessory` → `NSApplicationActivationPolicyAccessory`【源码】 |
| 阴影 | `shadow(false)` | 支持 |

结论:Tauri 做"透明无边框沉底悬浮窗"能力齐全。代价:前端是 Webview(HTML/JS),项目从纯 Rust 变成 Rust + 前端工具链;二进制体积和内存(WKWebView)比 egui 大。

### 2.2 tauri-plugin-widgets 成熟度(真 WidgetKit)

[s00d/tauri-plugin-widgets](https://github.com/s00d/tauri-plugin-widgets)(MIT,活跃维护)用一份 JSON 声明式配置同时生成 Android Glance / Apple WidgetKit / Windows Adaptive Cards / 桌面 webview widget【社区一手,README 与官方文档站】。macOS 侧实情【社区一手,[macOS setup](https://raw.githubusercontent.com/s00d/tauri-plugin-widgets/main/docs/guide/setup/macos.md)】:

- **仍需 sidecar Xcode 工程**:插件用 xcodegen 生成 `Sources/MyWidget.swift`(Swift 代码,基于其 `TauriWidgetProvider`),`beforeBundleCommand` 里编译签名 `.appex` 嵌入 `Contents/PlugIns`。Swift 不用你手写,但 Xcode 工具链、签名、/Applications 安装这些 Apple 约束一项不少。
- ad-hoc 签名可本机跑(`widgetContainer` 传输,宿主不得沙盒化);App Groups 传输要真实 Team ID。
- 排障文档同样提醒 "Apple limits widget refreshes to ~40–70 per day"。

即:该插件把路线 A 的 Swift 样板和 Xcode 配置自动化了,**降低了但不可能消除 WidgetKit 的固有复杂度**;它同时也提供"桌面 webview widget"(纯悬浮窗)作为不碰 Xcode 的降级面。

---

## 3. 路线 B:egui/winit 纯 Rust

### 3.1 窗口能力逐项(均有源码级确认)

- **窗口层级**:winit `WindowLevel::{AlwaysOnBottom, Normal, AlwaysOnTop}`【官方,[docs.rs](https://docs.rs/winit/latest/winit/window/enum.WindowLevel.html)】,文档明确 AlwaysOnBottom "useful for a widget-based app",且不支持列表里只有 iOS/Android/Web/Wayland——macOS 支持。winit 0.30.13 源码:macOS 上 `AlwaysOnBottom → kCGNormalWindowLevel - 1`,`AlwaysOnTop → kCGFloatingWindowLevel`【源码,`window_delegate.rs` set_window_level】。注意它沉在普通窗口之下、桌面图标之上,与真 widget 视觉层级基本一致【推断】。
- **鼠标穿透**:`Window::set_cursor_hittest(false)` → macOS 直接调 `setIgnoresMouseEvents(true)`【源码,winit 0.30.13】。
- **egui/eframe 暴露的开关**:`ViewportBuilder` 有 `transparent`、`decorations`、`window_level`、`mouse_passthrough`、`has_shadow`、`fullsize_content_view` 等字段,且官方注明 macOS 下 transparent 建议配 `has_shadow(false)` 避免残影【官方,[egui ViewportBuilder](https://docs.rs/egui/latest/egui/viewport/struct.ViewportBuilder.html)】。
- **透明的坑**:eframe 默认 glow 后端透明可用;**wgpu 后端在 Mac 上 `transparent` 无效**(issue 已关闭但现象被多人复现)【社区,[egui#2680](https://github.com/emilk/egui/issues/2680)】。也有现成封装 [egui_overlay](https://crates.io/crates/egui_overlay)(透明表面/去边框/输入穿透)【社区】。
- **菜单栏**:[tray-icon](https://crates.io/crates/tray-icon)(tauri-apps 维护,"Create tray icons for desktop applications")提供跨平台状态栏图标+菜单,可与 egui/tao 事件循环共存【官方/源码,本地 registry 0.21.3/0.24.1】。

### 3.2 macOS 上的已知坑

- **Dock 图标**:裸 winit/egui 默认会在 Dock 出现;隐藏需把 app 打成 bundle 并设 `LSUIElement`,或经 objc2 调 `setActivationPolicy(Accessory)`——用 objc2-app-kit 几行可解决【社区/推断,tao 内即有同款实现可参照】。
- **Spaces/全屏**:沉底窗口默认只存在于当前 Space;要"所有桌面可见"需 `NSWindow.collectionBehavior = canJoinAllSpaces`(winit 未直接暴露,需 objc2 补)【社区】。
- **Retina**:winit 的 scale factor 处理在 macOS 上成熟(逻辑/物理像素分离),`ScaleFactorChanged` 事件需响应重绘【官方,winit docs】。
- **打包**:要获得稳定的 bundle id、图标、登录项自启,需要从"裸二进制"升级到 `.app` bundle(cargo-bundle 或手写 Info.plist + ad-hoc 签名),本机自用无需付费账号【社区】。

---

## 4. 实时数据现实性

### 4.1 轮询频率

这些 usage API 均为未公开接口,**没有任何官方 rate limit 文档**;只能看社区实践:

- CodexBar(最成熟同类):可选 1m/2m/5m/15m/30m/手动/自适应;**新装默认自适应,实际决策区间 2–30 分钟**(近期交互 2m、温热 5m、闲置 15m、长闲置 30m,低电量/过热 30m);旧默认 5 分钟;widget 快照与 token 刷新统一钳 5 分钟下限【社区一手,[refresh-loop.md](https://raw.githubusercontent.com/steipete/CodexBar/main/docs/refresh-loop.md)】。
- ClaudeUsageWidget 宣称 5 分钟【社区】。
- Claude OAuth 刷新端点"有严格 rate limit"【社区,[OpenClaw 指南](https://resources.learnopenclaw.ai/openclaw-claude-anthropic-avoid-getting-banned/)】。

结论:【推断】**5 分钟轮询是经过验证的安全基线;活跃时段 2 分钟也可接受;秒级无意义**(用量窗口本身是 5 小时/周粒度,且各家用量数据服务端也有缓存延迟)。悬浮窗/菜单栏路线可以做到这个频率;WidgetKit 路线被系统预算卡在 ~15 分钟。

### 4.2 Token 生命周期(claudex 现状已覆盖大半)

- **Claude**:access token ~8 小时过期,refresh token 长期有效;claudex 已实现过期检测(60s 提前量)与刷新回写【社区一手,[claude-code#31095](https://github.com/anthropics/claude-code/issues/31095)、[#68398](https://github.com/anthropics/claude-code/issues/68398);源码,本仓库 `src/auth.rs`】。**注意 Anthropic 对共享 refresh token 的并发刷新敏感**,常驻 GUI 与 Claude Code 本体同时刷新可能互相顶掉,claudex 现有的"401 才刷新"保守策略应保留【推断】。
- **Codex**:`~/.codex/auth.json` 文件凭据【源码,`src/codex/auth.rs`】;文件可能被 Codex CLI 自己滚动更新,GUI 应每次读取而非缓存【推断】。
- **Kimi**:凭据文件 + refresh 流程,claudex 已实现刷新回写【源码,`src/kimi/auth.rs`】。GLM 为 API key(无过期问题),Grok 走本地凭据【源码,`src/glm`、`src/grok`】。

### 4.3 GUI 常驻进程访问 Keychain 的差异

claudex 当前通过 spawn `/usr/bin/security find-generic-password` 读 "Claude Code-credentials"【源码,`src/auth.rs:131`】。这条路径对 GUI 有个意外的好处:**Keychain 弹窗的 ACL 挂在 Apple 签名的 `security` 工具上**,GUI 继续 shell out 则行为与 CLI 完全一致,无新增摩擦【推断,基于实现机制】。若改为直接用 Security framework FFI(`security-framework` crate),弹窗会指名 GUI 二进制本身:稳定签名身份下一次"始终允许"即可,ad-hoc/频繁重签会导致反复弹窗——CodexBar 专门写了 [keychain-prompts.md](https://raw.githubusercontent.com/steipete/CodexBar/main/docs/keychain-prompts.md) 教用户在 Keychain Access 里把 app 加进条目 ACL,可见这是同类应用的共性痛点【社区一手】。**建议:GUI 保留 shell out `security` 的做法,且 token 刷新回写只在一个进程里做,避免与 Claude Code 竞争。**

---

## 5. 生态参考

| 项目 | 技术栈/架构 | 可借鉴点 |
|---|---|---|
| [steipete/CodexBar](https://github.com/steipete/CodexBar) | Swift 6 菜单栏 App(macOS 14+),60+ provider(含 Claude/Codex/Gemini/Grok/z.ai/Kimi);WidgetKit widget = 主 App 写 JSON 快照到 App Group,widget 只读渲染;自适应 2–30m 刷新;**另发 bundled CLI 输出 JSON,Linux 生态(waybar/GNOME/KDE/tmux 插件)全部消费 CLI 的 JSON** | 架构范本:**数据核心与展示壳分离**;自适应刷新策略表;Keychain 排障文档;widget 快照 5 分钟下限 |
| [dependentsign/ClaudeUsageWidget](https://github.com/dependentsign/ClaudeUsageWidget) | 纯 Swift WidgetKit,xcodebuild + ad-hoc 重签,直接读 `~/.claude` 配置 | WidgetKit 最小实现参考;"5 分钟自刷"的营销话术反证预算约束 |
| [s00d/tauri-plugin-widgets](https://github.com/s00d/tauri-plugin-widgets) | Tauri 插件:JSON → 生成 Swift widget 工程 + 桌面 webview widget | 若选真 WidgetKit,用它自动化 Xcode 样板;ad-hoc 下 `widgetContainer` 传输方案 |
| [tikaboolabs/Plex-Desktop-Server-Widgets](https://github.com/tikaboolabs/Plex-Desktop-Server-Widgets) | "No WidgetKit or developer account required" 的悬浮窗伪 widget | 证明"伪 widget 悬浮窗"是规避 Xcode/签名的主流务实选择 |
| ccusage 系 | 纯本地日志分析,不碰 API | 数据源的补充思路(本地 transcript 推算成本,零 rate limit 风险) |

未见"Rust 写的 macOS AI 用量 widget"成品——这正是空档,也是本调研要回答的问题。

---

## 6. 推荐架构与工作量

### 推荐:数据核心(claudex)+ 薄展示壳,分两期

```
┌─────────────────────────────────────────────┐
│ claudex(现有 CLI,抽出 core lib)            │
│  · 各 provider auth/api(已有)               │
│  · 新增:`claudex usage --json`(快照输出)    │
│  · 新增:`claudex daemon`(定时轮询→写         │
│    ~/.claudex/widget-snapshot.json,原子写)  │
└──────────────┬──────────────────────────────┘
               │ 文件(或 stdout JSON)
     ┌─────────┴──────────┐
     ▼                    ▼
┌─────────────┐    ┌───────────────┐
│ claudex-bar │    │ 任意第三方壳    │
│ egui 悬浮窗  │    │ (SketchyBar/   │
│ + tray-icon │    │  tmux/waybar…) │
└─────────────┘    └───────────────┘
```

- **一期(MVP,纯 Rust,不碰 Xcode)**:claudex 增加 `--json` 输出与可选 daemon 轮询(默认 5 分钟,自适应抄 CodexBar 策略表);新建 `claudex-bar` 二进制(egui + tray-icon):透明无边框沉底悬浮窗 + 菜单栏图标,读快照文件渲染,点击菜单"立即刷新"时直接同步调一次 core。复用 `security` shell-out 读 Keychain,刷新回写只发生在 claudex 侧。
- **二期(可选,真 WidgetKit)**:若用户明确要"系统桌面小组件"形态,引入 tauri-plugin-widgets 或手写最小 Swift 壳(uniffi 绑 claudex-core),宿主 App 复用一期 daemon 写 App Group 快照,widget 只读渲染。接受 Apple 流水线(Xcode、签名、/Applications、预算 15 分钟级)。

选"落盘 JSON + 只读壳"而非"core lib 直接链接进 GUI"的理由:进程隔离(token 竞争、崩溃隔离)、第三方壳可自由接入(CodexBar 的 Linux 生态已验证该模式)、GUI 可独立重写(Tauri/SwiftUI)不动核心【推断+社区佐证】。

### 工作量评估(熟悉 Rust 的单人,含自测)

| 任务 | 量级 |
|---|---|
| claudex `--json` 快照输出 + daemon 轮询 | 1–2 天 |
| egui 悬浮窗 MVP(进度条、倒计时、点击刷新、位置记忆) | 3–5 天 |
| tray-icon 菜单栏 + Accessory 模式隐藏 Dock + .app 打包脚本 | 1–2 天 |
| **一期合计** | **约 1–1.5 周** |
| 二期 WidgetKit(Swift 壳/uniffi 或 tauri-plugin-widgets、签名公证、App Group) | 1–2 周 + 持续维护税 |

---

## 7. 风险清单

1. **未公开 API 变更/封号风险**:所有 usage 端点均为逆向所得,服务商可随时变更或加强风控;轮询须保守(≥2 分钟),遵守各 provider 的隐性规则【推断】。
2. **WidgetKit 预算不可控**:40–70 次/天是软预算,系统按查看频率动态调整,任何"每 X 分钟必刷新"的承诺都无法兑现【官方+社区】。
3. **Token 刷新竞争**:常驻进程与 Claude Code/Codex CLI 本体并发刷新同一 refresh token 可能互相失效;须单点刷新、401 驱动、带退避【推断,有社区事故记录】。
4. **Keychain 弹窗体验**:改用 FFI 直读 + ad-hoc 重签的组合会导致每次构建后重新弹窗;保持 `security` shell-out 或稳定签名身份可规避【社区一手】。
5. **egui 透明窗 wgpu 后端在 Mac 失效**:锁定 glow 后端,或预留 Tauri 备选【社区】。
6. **分发形态变化带来的连锁改动**:Dock 隐藏、登录项自启、自动更新(self-replace 对 .app 内二进制同样可行但需验证签名)都因 .app 化而需要重新设计【推断】。
7. **多 Space/多显示器**:沉底悬浮窗跨 Space 行为需 objc2 补充,测试矩阵别忘了外接显示器与 Retina 混插【社区】。
