import SwiftUI

private enum SettingsPane: String, CaseIterable, Identifiable {
    case general
    case vocabulary
    case recognition
    case models
    case permissions
    case diagnostics

    var id: String { rawValue }

    func title(isEnglish: Bool) -> String {
        switch self {
        case .general: isEnglish ? "General" : "通用"
        case .vocabulary: isEnglish ? "Vocabulary" : "词表"
        case .recognition: isEnglish ? "Recognition" : "识别"
        case .models: isEnglish ? "Models" : "模型"
        case .permissions: isEnglish ? "Permissions" : "权限"
        case .diagnostics: isEnglish ? "Diagnostics" : "诊断"
        }
    }

    func subtitle(isEnglish: Bool) -> String {
        switch self {
        case .general:
            isEnglish ? "Tune the core voice input behavior and how text is cleaned before paste." : "调整语音输入核心行为，以及粘贴前的文本处理方式。"
        case .vocabulary:
            isEnglish ? "Manage correction settings and the terms Open Flow should preserve across transcripts." : "管理纠错设置，以及 Open Flow 在转写时应优先保留的词语。"
        case .recognition:
            isEnglish ? "Choose between local SenseVoice and Groq Whisper, then adjust how transcription runs." : "在本地 SenseVoice 和 Groq Whisper 之间切换，并调整转写方式。"
        case .models:
            isEnglish ? "Manage local model presets, download progress, and the storage path on this Mac." : "管理本机模型预设、下载进度和存储路径。"
        case .permissions:
            isEnglish ? "Check the macOS permissions Open Flow needs for recording, hotkeys, and text injection." : "检查 Open Flow 录音、热键和文字注入所需的 macOS 权限。"
        case .diagnostics:
            isEnglish ? "Inspect hotkey events, daemon logs, and download output when something feels off." : "出现异常时查看热键事件、daemon 日志和下载输出。"
        }
    }

    var icon: String {
        switch self {
        case .general: "slider.horizontal.3"
        case .vocabulary: "book.closed"
        case .recognition: "waveform.and.mic"
        case .models: "shippingbox"
        case .permissions: "lock.shield"
        case .diagnostics: "stethoscope"
        }
    }
}

struct ContentView: View {
    @StateObject private var config = ConfigManager()
    @State private var selectedPane: SettingsPane = .recognition
    @State private var showSaveConfirmation = false
    @State private var showCopyConfirmation = false

    private let sidebarWidth: CGFloat = 210
    private let pageSpacing: CGFloat = 16

    private var isEnglish: Bool { config.normalizedUiLanguage == "en" }

    private func tr(_ zh: String, _ en: String) -> String {
        isEnglish ? en : zh
    }

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(red: 0.90, green: 0.94, blue: 0.99),
                    Color(red: 0.96, green: 0.97, blue: 0.99),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            HStack(spacing: 0) {
                sidebar
                    .frame(width: sidebarWidth)

                mainSurface
            }
            .padding(12)
        }
        .onAppear {
            config.checkModelReady()
        }
        .onChange(of: config.provider) { newValue in
            if newValue == "local" {
                config.checkModelReady()
            }
        }
        .onChange(of: config.modelPreset) { newValue in
            guard config.provider == "local" else { return }
            config.selectLocalModelPreset(newValue)
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button {
                if config.daemonRunning {
                    config.quitApp()
                } else {
                    config.startDaemon()
                }
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "arrow.left")
                        .font(.system(size: 15, weight: .medium))
                    Text(tr("返回应用", "Back to App"))
                        .font(.system(size: 15, weight: .medium))
                }
                .foregroundStyle(Color(red: 0.40, green: 0.45, blue: 0.54))
                .padding(.horizontal, 6)
                .padding(.top, 8)
            }
            .buttonStyle(.plain)

            VStack(alignment: .leading, spacing: 6) {

                ForEach(SettingsPane.allCases) { pane in
                    SidebarItemButton(
                        title: pane.title(isEnglish: isEnglish),
                        subtitle: nil,
                        icon: pane.icon,
                        isSelected: pane == selectedPane
                    ) {
                        selectedPane = pane
                    }
                }
            }

            Spacer()

            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Circle()
                        .fill(config.daemonRunning ? Color(red: 0.20, green: 0.75, blue: 0.46) : Color(red: 0.74, green: 0.77, blue: 0.82))
                        .frame(width: 8, height: 8)
                    Text(config.daemonRunning ? tr("运行中", "Running") : tr("未运行", "Stopped"))
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                }

                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([config.configFileURL])
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "folder")
                            .font(.system(size: 13, weight: .medium))
                        Text(tr("显示配置", "Reveal Config"))
                            .font(.system(size: 13, weight: .medium))
                    }
                    .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                }
                .buttonStyle(.plain)
            }
            .padding(.bottom, 10)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 14)
        .background(
            RoundedRectangle(cornerRadius: 26, style: .continuous)
                .fill(Color(red: 0.92, green: 0.95, blue: 0.98).opacity(0.95))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 26, style: .continuous)
                .stroke(Color.white.opacity(0.68), lineWidth: 1)
        )
    }

    private var mainSurface: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: pageSpacing) {
                    pageHeader

                    if !config.lastError.isEmpty {
                        bannerCard(
                            icon: "exclamationmark.triangle.fill",
                            title: tr("有一项需要处理", "Something needs attention"),
                            message: config.lastError,
                            tint: Color(red: 0.92, green: 0.37, blue: 0.33)
                        ) {
                            config.lastError = ""
                        }
                    }

                    pageContent
                }
                .padding(24)
            }

            Divider()
                .overlay(Color(red: 0.91, green: 0.93, blue: 0.96))

            bottomBar
                .padding(.horizontal, 24)
                .padding(.vertical, 14)
        }
        .background(
            RoundedRectangle(cornerRadius: 30, style: .continuous)
                .fill(Color.white.opacity(0.98))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 30, style: .continuous)
                .stroke(Color.white.opacity(0.9), lineWidth: 1)
        )
        .shadow(color: Color.black.opacity(0.06), radius: 28, x: 0, y: 12)
    }

    private var pageHeader: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                Text(selectedPane.title(isEnglish: isEnglish))
                    .font(.system(size: 28, weight: .bold, design: .rounded))
                    .foregroundStyle(Color(red: 0.07, green: 0.09, blue: 0.13))

                Text(selectedPane.subtitle(isEnglish: isEnglish))
                    .font(.system(size: 13.5, weight: .medium))
                    .foregroundStyle(Color(red: 0.40, green: 0.45, blue: 0.54))
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            HStack(spacing: 10) {
                borderAction(title: tr("刷新", "Refresh"), icon: "arrow.clockwise") {
                    config.refreshStatus()
                    config.refreshPermissions()
                    config.checkModelReady()
                    if selectedPane == .diagnostics {
                        config.loadLogs()
                    }
                }

                borderAction(title: selectedPane == .vocabulary ? tr("显示词表", "Reveal Vocabulary") : tr("显示配置", "Reveal Config"), icon: "folder") {
                    if selectedPane == .vocabulary {
                        NSWorkspace.shared.activateFileViewerSelecting([config.personalVocabularyFileURL])
                    } else {
                        NSWorkspace.shared.activateFileViewerSelecting([config.configFileURL])
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var pageContent: some View {
        switch selectedPane {
        case .general:
            generalPage
        case .vocabulary:
            vocabularyPage
        case .recognition:
            recognitionPage
        case .models:
            modelsPage
        case .permissions:
            permissionsPage
        case .diagnostics:
            diagnosticsPage
        }
    }

    private var generalPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: tr("输入行为", "Input Behavior"), subtitle: tr("设置如何开始录音，以及 Open Flow 在你说话时如何响应。", "Set how you start recording and how Open Flow reacts while you speak.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("界面语言", "Interface Language"), description: tr("默认使用中文，也可以随时切换成英文。保存后会自动重启 daemon。", "Use Chinese by default, or switch to English anytime. Saving will restart the daemon automatically.")) {
                        Picker("", selection: $config.uiLanguage) {
                            Text("中文").tag("zh")
                            Text("English").tag("en")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 180)
                    }

                    rowDivider
                    SettingsRow(label: tr("热键", "Hotkey"), description: tr("选择 Open Flow 全局监听的按键。", "Pick the key Open Flow listens for globally.")) {
                        Picker("", selection: $config.hotkey) {
                            ForEach(Array(zip(ConfigManager.hotkeys, hotkeyLabels)), id: \.0) { key, label in
                                Text(label).tag(key)
                            }
                        }
                        .pickerStyle(.menu)
                        .labelsHidden()
                        .frame(width: 220)
                    }

                    rowDivider
                    SettingsRow(label: tr("触发模式", "Trigger Mode"), description: tr("切换适合听写，按住更像按住说话。", "Toggle works well for dictation, while hold feels more like push-to-talk.")) {
                        Picker("", selection: $config.triggerMode) {
                            ForEach(Array(zip(ConfigManager.triggerModes, triggerLabels)), id: \.0) { mode, label in
                                Text(label).tag(mode)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 320)
                    }
                }
            }

            SettingsCard(title: tr("文本处理", "Text Processing"), subtitle: tr("在文本粘贴到编辑器或应用之前，对 ASR 结果做最后处理。", "Shape the text after ASR before it is pasted into your editor or app.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("中文转换", "Chinese Conversion"), description: tr("对最终转写结果应用 ICU 的简繁转换。", "Apply ICU-based simplified/traditional conversion to the final transcription.")) {
                        Picker("", selection: $config.chineseConversion) {
                            Text(tr("不转换", "None")).tag("")
                            Text("简 → 繁").tag("s2t")
                            Text("繁 → 简").tag("t2s")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 220)
                    }
                }
            }
        }
    }

    private var vocabularyPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: tr("纠错设置", "Correction Settings"), subtitle: tr("控制是否启用基于个人词表的纠错能力。模型本身在 Models 页面中单独配置。", "Control whether vocabulary-based correction is enabled. The LLM itself is configured separately in the Models page.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("启用纠错", "Enable Correction"), description: tr("开启后会结合你的个人词表做额外纠错。", "Turn on the extra correction step that uses your vocabulary as hints.")) {
                        Toggle("", isOn: correctionEnabledBinding)
                            .labelsHidden()
                            .toggleStyle(.switch)
                    }
                }
            }

            SettingsCard(title: tr("个人词表", "Personal Vocabulary"), subtitle: tr("每行一个词或短语。可把姓名、产品名、项目代号和专业术语放在这里，帮助纠错更稳定。", "One term or phrase per line. Keep names, products, project codenames, and domain jargon here so correction stays stable.")) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text(tr("这份词表仅保存在当前这台 Mac 上。", "This list is saved locally on this Mac."))
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))

                        Spacer()

                        subtleAction(title: tr("打开文件", "Open File"), icon: "folder") {
                            NSWorkspace.shared.activateFileViewerSelecting([config.personalVocabularyFileURL])
                        }
                    }

                    ZStack(alignment: .topLeading) {
                        if config.personalVocabulary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            Text("Open Flow\nSenseVoice\nGLM-4.7-Flash")
                                .font(.system(.body, design: .monospaced))
                                .foregroundStyle(Color(red: 0.63, green: 0.67, blue: 0.74))
                                .padding(.horizontal, 18)
                                .padding(.vertical, 18)
                                .allowsHitTesting(false)
                        }

                        TextEditor(text: $config.personalVocabulary)
                            .font(.system(.body, design: .monospaced))
                            .scrollContentBackground(.hidden)
                            .padding(14)
                    }
                    .background(Color(red: 0.97, green: 0.98, blue: 0.99))
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 18, style: .continuous)
                            .stroke(Color(red: 0.92, green: 0.94, blue: 0.97), lineWidth: 1)
                    )
                    .frame(minHeight: 320)
                }
            }

            SettingsCard(title: tr("纠错系统提示词", "Correction System Prompt"), subtitle: tr("可直接查看和修改纠错阶段使用的系统提示词。建议保留 `{{personal_vocabulary}}` 占位符，让个人词表能自动注入进去。", "View and edit the system prompt used by the correction step. Keep the `{{personal_vocabulary}}` placeholder so your personal vocabulary can still be injected automatically.")) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text(tr("这份提示词仅保存在当前这台 Mac 上。", "This prompt is stored locally on this Mac only."))
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))

                        Spacer()

                        subtleAction(title: tr("恢复默认", "Reset Default"), icon: "arrow.uturn.backward") {
                            config.correctionSystemPrompt = config.defaultCorrectionSystemPrompt
                        }

                        subtleAction(title: tr("打开文件", "Open File"), icon: "folder") {
                            NSWorkspace.shared.activateFileViewerSelecting([config.correctionSystemPromptFileURL])
                        }
                    }

                    TextEditor(text: $config.correctionSystemPrompt)
                        .font(.system(.body, design: .monospaced))
                        .scrollContentBackground(.hidden)
                        .padding(14)
                        .background(Color(red: 0.97, green: 0.98, blue: 0.99))
                        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 18, style: .continuous)
                                .stroke(Color(red: 0.92, green: 0.94, blue: 0.97), lineWidth: 1)
                        )
                        .frame(minHeight: 250)
                }
            }
        }
    }

    private var recognitionPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: tr("语音识别引擎", "Speech Recognition Provider"), subtitle: tr("选择整个应用使用的转写引擎。", "Choose the engine that powers transcription across the app.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("引擎", "Provider"), description: tr("本地 SenseVoice 更注重隐私，Groq Whisper 走云端转写。", "Use local SenseVoice for privacy, or Groq Whisper for cloud transcription.")) {
                        Picker("", selection: $config.provider) {
                            Text(tr("本地", "Local")).tag("local")
                            Text("Groq").tag("groq")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 220)
                    }

                    rowDivider
                    HStack(alignment: .top, spacing: 14) {
                        providerBlurb(
                            title: tr("本地 SenseVoice", "Local SenseVoice"),
                            icon: "lock.shield",
                            accent: Color(red: 0.24, green: 0.74, blue: 0.49),
                            body: tr("音频保留在本机。更适合重视隐私和离线使用的场景。", "Audio stays on-device. Best when privacy and offline use matter more than the easiest setup."),
                            isActive: config.provider == "local"
                        )

                        providerBlurb(
                            title: "Groq Whisper",
                            icon: "cloud.sun",
                            accent: Color(red: 0.23, green: 0.58, blue: 0.96),
                            body: tr("使用 Whisper 的云端方案，上手快，但需要 API Key，并会把音频发送到 Groq。", "Fast cloud setup with Whisper models. Requires an API key and sends audio to Groq."),
                            isActive: config.provider == "groq"
                        )
                    }
                    .padding(.top, 18)
                }
            }

            if config.provider == "local" {
                SettingsCard(title: tr("本地引擎详情", "Local Provider Details"), subtitle: tr("调整本地模型预设，并快速查看是否已就绪。", "Tune the local model preset and inspect its readiness at a glance.")) {
                    VStack(spacing: 0) {
                        SettingsRow(label: tr("预设", "Preset"), description: tr("量化版更轻量，FP16 更偏向高保真。", "Quantized is lighter, while FP16 aims for higher fidelity.")) {
                            Picker("", selection: $config.modelPreset) {
                                Text(tr("量化版", "Quantized")).tag("quantized")
                                Text("FP16").tag("fp16")
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 210)
                        }

                        rowDivider
                        SettingsRow(label: tr("模型状态", "Model Status"), description: tr("Open Flow 会从下方路径使用当前选中的预设。", "Open Flow will use the selected preset from the path below.")) {
                            StatusPill(
                                text: config.modelDownloading ? tr("下载中", "Downloading") : (config.modelReady ? tr("已就绪", "Ready") : tr("未下载", "Not downloaded")),
                                tone: config.modelDownloading ? .info : (config.modelReady ? .success : .warning)
                            )
                        }

                        rowDivider
                        SettingsRow(label: tr("模型路径", "Model Path"), description: tr("点击可复制实际路径，或在需要时用 Finder 打开对应文件夹。", "Click to copy the resolved path, or open the folder in Finder when you need to inspect the local preset.")) {
                            modelPathActions
                        }
                    }
                }
            } else {
                SettingsCard(title: tr("Groq 配置", "Groq Configuration"), subtitle: tr("通过 Groq 接入 Whisper，并选择你想使用的模型。", "Connect Whisper through Groq and choose the model profile you want.")) {
                    VStack(spacing: 0) {
                        SettingsRow(label: tr("API Key", "API Key"), description: tr("你可以在这里粘贴 Groq Key，或通过 GROQ_API_KEY 环境变量提供。", "You can paste a Groq key here or provide it through the GROQ_API_KEY environment variable.")) {
                            VStack(alignment: .trailing, spacing: 8) {
                                SecureField("gsk_...", text: $config.groqApiKey)
                                    .textFieldStyle(.roundedBorder)
                                    .frame(width: 300)

                                StatusPill(
                                    text: !config.groqApiKey.isEmpty ? tr("已本地保存", "Stored locally") : (ProcessInfo.processInfo.environment["GROQ_API_KEY"] != nil ? tr("来自环境变量", "From env") : tr("缺失", "Missing")),
                                    tone: !config.groqApiKey.isEmpty || ProcessInfo.processInfo.environment["GROQ_API_KEY"] != nil ? .success : .warning
                                )
                            }
                        }

                        rowDivider
                        SettingsRow(label: tr("Whisper 模型", "Whisper Model"), description: tr("Large v3 Turbo 是默认的速度和成本平衡方案。", "Large v3 Turbo is the default balance of speed and cost.")) {
                            Picker("", selection: $config.groqModel) {
                                Text("Large v3 Turbo").tag("whisper-large-v3-turbo")
                                Text("Large v3").tag("whisper-large-v3")
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 250)
                        }

                        rowDivider
                        SettingsRow(label: tr("语言提示", "Language Hint"), description: tr("留空时自动识别，也可以填写 zh、en、ja、ko 等值。", "Leave empty for auto-detect, or set values like zh, en, ja, or ko.")) {
                            TextField("auto", text: $config.groqLanguage)
                                .textFieldStyle(.roundedBorder)
                                .frame(width: 120)
                        }
                    }
                }
            }
        }
    }

    private var modelsPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: tr("模型存储", "Model Storage"), subtitle: tr("让本地模型资产保持清晰可见，便于安装和排查。", "Keep the local model assets healthy and visible so setup stays predictable.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("预设", "Preset"), description: tr("Open Flow 会在 Application Support 下分别存储量化版和 FP16。", "Open Flow stores quantized and FP16 in separate folders under Application Support.")) {
                        Picker("", selection: $config.modelPreset) {
                            Text(tr("量化版", "Quantized")).tag("quantized")
                            Text("FP16").tag("fp16")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 210)
                    }

                    rowDivider
                    SettingsRow(label: tr("模型状态", "Model Status"), description: tr("在 daemon 加载前确认当前预设是否已经下载到本地。", "Check whether the selected preset is available locally before the daemon tries to load it.")) {
                        StatusPill(
                            text: config.modelDownloading ? tr("下载中", "Downloading") : (config.modelReady ? tr("已就绪", "Ready") : tr("缺失", "Missing")),
                            tone: config.modelDownloading ? .info : (config.modelReady ? .success : .warning)
                        )
                    }

                    rowDivider
                    SettingsRow(label: tr("下载", "Download"), description: tr("当本地文件缺失或过旧时，从 Hugging Face 获取或刷新当前预设。", "Fetch or refresh the current preset from Hugging Face when the local files are missing or outdated.")) {
                        VStack(alignment: .trailing, spacing: 8) {
                            SoftActionButton(
                                title: config.modelReady ? tr("重新下载模型", "Re-download Model") : tr("下载模型", "Download Model"),
                                icon: "arrow.down.circle.fill",
                                fill: Color(red: 0.23, green: 0.58, blue: 0.96),
                                foreground: .white
                            ) {
                                config.downloadModel()
                            }
                            .disabled(config.modelDownloading)

                            if config.modelDownloading {
                                ProgressView(value: config.modelDownloadProgress)
                                    .progressViewStyle(.linear)
                                    .frame(width: 220)
                                Text(config.modelDownloadStatus.isEmpty ? tr("正在准备下载...", "Preparing download...") : config.modelDownloadStatus)
                                    .font(.system(size: 11.5, weight: .medium))
                                    .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                            } else {
                                Text(config.selectedLocalModelDownloadSummary)
                                    .font(.system(size: 11.5, weight: .medium))
                                    .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                            }
                        }
                    }

                    rowDivider
                    SettingsRow(label: tr("实际路径", "Resolved Path"), description: tr("这是 daemon 加载本地 ASR 预设时会读取的目录。你可以复制路径，也可以直接在 Finder 中打开。", "This is the directory the daemon will look at when loading the local ASR preset. You can copy it or open the folder directly in Finder.")) {
                        modelPathActions
                    }
                }
            }

            SettingsCard(title: tr("LLM 配置", "LLM Configuration"), subtitle: tr("把大模型作为独立能力进行管理，便于后续扩展语音纠错之外的更多能力。", "Manage large-language-model settings as a standalone capability so Open Flow can grow beyond post-ASR correction.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("模型", "Model"), description: tr("当前用于纠错，也会作为未来其他 LLM 功能的统一入口。", "Used for correction today, and designed to become the shared entry for future LLM-powered features.")) {
                        Picker("", selection: $config.correctionModel) {
                            ForEach(ConfigManager.correctionModels, id: \.self) { model in
                                Text(model).tag(model)
                            }
                        }
                        .pickerStyle(.menu)
                        .labelsHidden()
                        .frame(width: 260)
                    }

                    rowDivider
                    SettingsRow(label: tr("API Key", "API Key"), description: tr("仅保存在本地，用于调用智谱等大模型服务。", "Stored locally and used for calling Zhipu or other large-model services.")) {
                        VStack(alignment: .trailing, spacing: 8) {
                            HStack(spacing: 8) {
                                SecureField("zhipu api key", text: $config.correctionApiKey)
                                    .textFieldStyle(.roundedBorder)
                                    .frame(width: 260)

                                Link(destination: URL(string: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys")!) {
                                    subtleActionLabel(title: tr("查看 API Keys", "API Keys"), icon: "arrow.up.right.square")
                                        .fixedSize(horizontal: true, vertical: false)
                                }
                            }

                            Text(tr("可在 BigModel 申请智谱 API Key。默认模型为 GLM-4.7-Flash，并支持在下拉列表中切换到其他预设模型。", "Apply for your Zhipu API Key on BigModel. The default model is GLM-4.7-Flash, and you can switch to other supported presets from the dropdown."))
                                .font(.system(size: 11.5, weight: .medium))
                                .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                                .frame(width: 460, alignment: .trailing)
                                .multilineTextAlignment(.trailing)
                        }
                    }
                }
            }

            if !config.modelDownloadOutput.isEmpty {
                SettingsCard(title: tr("下载输出", "Download Output"), subtitle: tr("原始命令输出，可用于排查下载问题并确认上一次发生了什么。", "Raw command output for debugging downloads and confirming what happened last.")) {
                    ScrollView {
                        Text(config.modelDownloadOutput)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(Color(red: 0.25, green: 0.28, blue: 0.34))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                            .padding(18)
                    }
                    .frame(minHeight: 180, maxHeight: 260)
                    .background(Color(red: 0.97, green: 0.98, blue: 0.99))
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                }
            }
        }
    }

    private var permissionsPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: tr("macOS 权限检查", "macOS Permission Checklist"), subtitle: tr("缺少权限时打开对应系统面板，授权后再重启 daemon。", "Open the right system panel when something is missing, then restart the daemon after granting access.")) {
                VStack(spacing: 0) {
                    permissionSettingsRow(
                        title: tr("辅助功能", "Accessibility"),
                        description: tr("用于全局热键检测和文字粘贴操作。", "Needed for global hotkey detection and text paste actions."),
                        granted: config.accessibilityGranted,
                        action: config.openAccessibilitySettings
                    )

                    rowDivider
                    permissionSettingsRow(
                        title: tr("输入监控", "Input Monitoring"),
                        description: tr("用于监听 Fn、右 Command 等键盘事件。", "Needed for listening to keyboard events like Fn or Right Command."),
                        granted: config.inputMonitoringGranted,
                        action: config.openInputMonitoringSettings
                    )

                    rowDivider
                    permissionSettingsRow(
                        title: tr("麦克风", "Microphone"),
                        description: tr("用于录音和转写你的语音。", "Needed for recording and transcribing your speech."),
                        granted: config.microphoneGranted,
                        action: config.openMicrophoneSettings
                    )
                }
            }

            SettingsCard(title: tr("授权后", "After Granting Access"), subtitle: tr("macOS 的权限更新不一定实时生效。重启 daemon 会更稳妥。", "macOS permission updates are not always live. Restarting the daemon keeps the setup predictable.")) {
                SettingsRow(label: tr("重启提示", "Restart Hint"), description: tr("当必需权限都授权后，重启 Open Flow，让 daemon 从干净状态重新检查权限。", "Once all required permissions are granted, restart Open Flow so the daemon re-checks access from a clean state.")) {
                    SoftActionButton(
                        title: tr("重启 daemon", "Restart daemon"),
                        icon: "arrow.clockwise",
                        fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                        foreground: Color(red: 0.12, green: 0.16, blue: 0.24)
                    ) {
                        config.restartDaemon()
                    }
                }
            }
        }
    }

    private var diagnosticsPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            #if OPENFLOW_PERF_DEV_UI
            SettingsCard(title: tr("性能日志", "Performance Logging"), subtitle: tr("持久化保存会话级耗时和进程资源快照，便于持续分析语音链路性能。", "Persist session-level timing and process resource snapshots so we can profile the voice pipeline over time.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("性能模式", "Performance Mode"), description: tr("启用后，Open Flow 会写入带有端到端耗时、CPU 和内存检查点的 JSONL 性能日志。", "When enabled, Open Flow writes JSONL performance logs with end-to-end timing, CPU, and memory checkpoints.")) {
                        Toggle("", isOn: performanceLoggingEnabledBinding)
                            .labelsHidden()
                            .toggleStyle(.switch)
                    }

                    rowDivider
                    SettingsRow(label: tr("日志位置", "Log Location"), description: tr("与 daemon.log 分开存储，便于筛选和归档性能分析数据。", "Stored separately from daemon.log so performance analysis stays easy to filter and archive.")) {
                        HStack(spacing: 8) {
                            valueCapsule(config.performanceLogDirectoryURL.path)
                            subtleAction(title: tr("打开文件夹", "Open Folder"), icon: "folder") {
                                NSWorkspace.shared.open(config.performanceLogDirectoryURL)
                            }
                        }
                    }
                }
            }
            #endif

            SettingsCard(title: tr("热键测试", "Hotkey Test"), subtitle: tr("监听修饰键变化，确认系统是否能识别你想用的热键。", "Listen for modifier changes and confirm the system sees the hotkey you want to use.")) {
                VStack(spacing: 0) {
                    SettingsRow(label: tr("监听器", "Listener"), description: tr("开始监听后，按下 Fn 或已配置热键，查看原始事件详情。", "Start monitoring and press Fn or your configured key to see raw event details.")) {
                        HStack(spacing: 12) {
                            SoftActionButton(
                                title: config.hotkeyTestActive ? tr("停止监听", "Stop Listening") : tr("开始监听", "Start Listening"),
                                icon: config.hotkeyTestActive ? "stop.fill" : "play.fill",
                                fill: config.hotkeyTestActive ? Color(red: 0.95, green: 0.38, blue: 0.36) : Color(red: 0.23, green: 0.58, blue: 0.96),
                                foreground: .white
                            ) {
                                if config.hotkeyTestActive {
                                    config.stopHotkeyTest()
                                } else {
                                    config.startHotkeyTest()
                                }
                            }

                            StatusPill(
                                text: config.hotkeyTestActive ? tr("监听中...", "Listening...") : tr("空闲", "Idle"),
                                tone: config.hotkeyTestActive ? .success : .neutral
                            )
                        }
                    }
                }
            }

            if !config.hotkeyTestLog.isEmpty {
                SettingsCard(title: tr("热键事件日志", "Hotkey Event Log"), subtitle: tr("监听期间捕获到的最近全局和本地修饰键事件。", "Recent global and local modifier events captured while the listener is active.")) {
                    logViewer(config.hotkeyTestLog, minHeight: 170, maxHeight: 220)
                }
            }

            SettingsCard(title: tr("Daemon 日志", "Daemon Log"), subtitle: tr("显示 daemon.log 的最近 100 行，便于排查模型加载、转写和权限问题。", "The last 100 lines from daemon.log, useful for model loading, transcription, and permission issues.")) {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        SoftActionButton(
                            title: tr("刷新", "Refresh"),
                            icon: "arrow.clockwise",
                            fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                            foreground: Color(red: 0.12, green: 0.16, blue: 0.24)
                        ) {
                            config.loadLogs()
                        }

                        SoftActionButton(
                            title: tr("在 Finder 中打开", "Open in Finder"),
                            icon: "folder",
                            fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                            foreground: Color(red: 0.12, green: 0.16, blue: 0.24)
                        ) {
                            NSWorkspace.shared.activateFileViewerSelecting([config.logFileURL])
                        }

                        Spacer()
                    }

                    logViewer(config.logContent.isEmpty ? tr("加载中...", "Loading...") : config.logContent, minHeight: 220, maxHeight: 320)
                }
            }

            if !config.modelDownloadOutput.isEmpty {
                SettingsCard(title: tr("模型下载输出", "Model Download Output"), subtitle: tr("最近一次模型下载命令的输出内容。", "Command output from the latest model download run.")) {
                    logViewer(config.modelDownloadOutput, minHeight: 180, maxHeight: 240)
                }
            }
        }
        .onAppear {
            config.loadLogs()
        }
    }

    private var bottomBar: some View {
        HStack(spacing: 14) {
            if showSaveConfirmation {
                miniStatus(
                    text: config.daemonRunning
                        ? tr("已保存到本地。正在重启 daemon 以应用新设置。", "Saved locally. Restarting daemon so the new settings take effect.")
                        : tr("已保存到本地。等你准备好后再启动 Open Flow 即可使用新设置。", "Saved locally. Launch Open Flow when you are ready to use the new settings."),
                    icon: "checkmark.circle.fill",
                    tint: Color(red: 0.24, green: 0.74, blue: 0.49)
                )
            }

            if showCopyConfirmation {
                miniStatus(
                    text: tr("模型路径已复制。", "Model path copied."),
                    icon: "doc.on.doc.fill",
                    tint: Color(red: 0.23, green: 0.58, blue: 0.96)
                )
            }

            Spacer()

            Text(tr("更改会写入本地配置和个人词表文件。", "Changes are written to the local config and vocabulary files."))
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Color(red: 0.46, green: 0.51, blue: 0.59))

            SoftActionButton(
                title: tr("保存更改", "Save Changes"),
                icon: "square.and.arrow.down",
                fill: Color(red: 0.23, green: 0.58, blue: 0.96),
                foreground: .white
            ) {
                saveAllChanges()
            }
            .keyboardShortcut("s", modifiers: .command)
        }
    }

    private func permissionSettingsRow(title: String, description: String, granted: Bool, action: @escaping () -> Void) -> some View {
        SettingsRow(label: title, description: description) {
            HStack(spacing: 10) {
                StatusPill(text: granted ? tr("已授权", "Granted") : tr("需要授权", "Needs Access"), tone: granted ? .success : .warning)
                SoftActionButton(
                    title: tr("打开设置", "Open Settings"),
                    icon: "arrow.up.right.square",
                    fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                    foreground: Color(red: 0.12, green: 0.16, blue: 0.24),
                    action: action
                )
            }
        }
    }

    private func providerBlurb(title: String, icon: String, accent: Color, body: String, isActive: Bool) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(accent)
                Text(title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color(red: 0.09, green: 0.11, blue: 0.15))
            }

            Text(body)
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                .fixedSize(horizontal: false, vertical: true)

            StatusPill(text: isActive ? tr("已选中", "Selected") : tr("可用", "Available"), tone: isActive ? .info : .neutral)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(isActive ? accent.opacity(0.09) : Color(red: 0.97, green: 0.98, blue: 0.99))
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(isActive ? accent.opacity(0.28) : Color(red: 0.92, green: 0.94, blue: 0.97), lineWidth: 1)
        )
    }

    private func miniStatus(text: String, icon: String, tint: Color) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
            Text(text)
                .lineLimit(1)
        }
        .font(.system(size: 12.5, weight: .medium))
        .foregroundStyle(tint)
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(tint.opacity(0.10))
        .clipShape(Capsule())
    }

    private func bannerCard(icon: String, title: String, message: String, tint: Color, dismiss: @escaping () -> Void) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .foregroundStyle(tint)
                .font(.system(size: 18, weight: .semibold))

            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color(red: 0.11, green: 0.13, blue: 0.17))
                Text(message)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Color(red: 0.40, green: 0.45, blue: 0.54))
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            Button(tr("关闭", "Dismiss"), action: dismiss)
                .buttonStyle(.plain)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(tint)
        }
        .padding(16)
        .background(tint.opacity(0.09))
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func borderAction(title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon)
                Text(title)
            }
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Color(red: 0.16, green: 0.18, blue: 0.24))
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(Color(red: 0.97, green: 0.98, blue: 0.99))
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .stroke(Color(red: 0.90, green: 0.93, blue: 0.96), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    private func subtleAction(title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            subtleActionLabel(title: title, icon: icon)
        }
        .buttonStyle(.plain)
    }

    private func subtleActionLabel(title: String, icon: String) -> some View {
        HStack(spacing: 7) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .semibold))
            Text(title)
                .font(.system(size: 12.5, weight: .semibold))
        }
        .foregroundStyle(Color(red: 0.16, green: 0.19, blue: 0.25))
        .padding(.horizontal, 11)
        .padding(.vertical, 9)
        .background(Color(red: 0.96, green: 0.97, blue: 0.99))
        .clipShape(Capsule())
        .overlay(
            Capsule()
                .stroke(Color(red: 0.90, green: 0.93, blue: 0.96), lineWidth: 1)
        )
    }

    private func valueCapsule(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12.5, weight: .medium))
            .foregroundStyle(Color(red: 0.24, green: 0.28, blue: 0.34))
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(Color(red: 0.96, green: 0.97, blue: 0.99))
            .clipShape(Capsule())
    }

    private func copyModelPath() {
        config.copyModelPathToClipboard()
        withAnimation(.easeOut(duration: 0.2)) { showCopyConfirmation = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.8) {
            withAnimation(.easeIn(duration: 0.2)) { showCopyConfirmation = false }
        }
    }

    private func saveAllChanges() {
        config.save()
        config.savePersonalVocabulary()
        config.saveCorrectionSystemPrompt()
        if config.daemonRunning {
            config.restartDaemon()
        }
        withAnimation(.easeOut(duration: 0.2)) { showSaveConfirmation = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 4) {
            withAnimation(.easeIn(duration: 0.2)) { showSaveConfirmation = false }
        }
    }

    private var correctionEnabledBinding: Binding<Bool> {
        Binding(
            get: { config.correctionIsEnabled },
            set: { config.setCorrectionEnabled($0) }
        )
    }

    private var performanceLoggingEnabledBinding: Binding<Bool> {
        #if OPENFLOW_PERF_DEV_UI
        Binding(
            get: { config.performanceLoggingIsEnabled },
            set: { config.setPerformanceLoggingEnabled($0) }
        )
        #else
        Binding(
            get: { false },
            set: { _ in }
        )
        #endif
    }

    private var hotkeyLabels: [String] {
        isEnglish
            ? ["Right Command (⌘)", "Right Option (⌥)", "Right Control (⌃)", "Right Shift (⇧)", "Fn", "F13"]
            : ["右 Command (⌘)", "右 Option (⌥)", "右 Control (⌃)", "右 Shift (⇧)", "Fn", "F13"]
    }

    private var triggerLabels: [String] {
        isEnglish ? ["Toggle", "Hold"] : ["切换", "按住"]
    }

    private var modelPathActions: some View {
        HStack(spacing: 8) {
            Button {
                copyModelPath()
            } label: {
                PathPill(text: config.resolvedModelPath)
            }
            .buttonStyle(.plain)
            .help(tr("复制模型路径", "Copy model path"))

            subtleAction(title: tr("打开文件夹", "Open Folder"), icon: "folder") {
                config.openModelFolder()
            }
            .help(tr("在 Finder 中打开模型文件夹", "Open the model folder in Finder"))
        }
    }

    private var rowDivider: some View {
        Divider()
            .overlay(Color(red: 0.93, green: 0.94, blue: 0.97))
            .padding(.leading, 2)
    }

    private func logViewer(_ text: String, minHeight: CGFloat, maxHeight: CGFloat) -> some View {
        ScrollView {
            Text(text)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(Color(red: 0.24, green: 0.27, blue: 0.32))
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
                .padding(18)
        }
        .frame(minHeight: minHeight, maxHeight: maxHeight)
        .background(Color(red: 0.97, green: 0.98, blue: 0.99))
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

}

private struct SidebarItemButton: View {
    let title: String
    let subtitle: String?
    let icon: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(alignment: .center, spacing: 12) {
                Image(systemName: icon)
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 18, height: 18)
                    .foregroundStyle(isSelected ? Color(red: 0.11, green: 0.14, blue: 0.20) : Color(red: 0.40, green: 0.46, blue: 0.55))

                VStack(alignment: .leading, spacing: 4) {
                    Text(title)
                        .font(.system(size: 15, weight: isSelected ? .semibold : .medium))
                        .foregroundStyle(Color(red: 0.11, green: 0.14, blue: 0.20))

                    if let subtitle {
                        Text(subtitle)
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(Color(red: 0.42, green: 0.48, blue: 0.58))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(isSelected ? Color.white.opacity(0.55) : Color.clear)
            )
        }
        .buttonStyle(.plain)
        .animation(.easeOut(duration: 0.14), value: isSelected)
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 17, weight: .semibold, design: .rounded))
                    .foregroundStyle(Color(red: 0.09, green: 0.11, blue: 0.15))

                Text(subtitle)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                    .fixedSize(horizontal: false, vertical: true)
            }

            VStack(alignment: .leading, spacing: 0) {
                content
            }
            .padding(6)
            .background(Color.white)
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(Color(red: 0.92, green: 0.94, blue: 0.97), lineWidth: 1)
            )
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(red: 0.98, green: 0.99, blue: 1.0))
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(Color(red: 0.93, green: 0.95, blue: 0.98), lineWidth: 1)
        )
    }
}

private struct SettingsRow<Control: View>: View {
    let label: String
    let description: String
    @ViewBuilder let control: Control

    var body: some View {
        HStack(alignment: .center, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Text(label)
                    .font(.system(size: 13.5, weight: .semibold))
                    .foregroundStyle(Color(red: 0.09, green: 0.11, blue: 0.15))
                Text(description)
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: 24)

            control
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }
}

private enum PillTone {
    case neutral
    case info
    case success
    case warning

    var foreground: Color {
        switch self {
        case .neutral: Color(red: 0.39, green: 0.45, blue: 0.53)
        case .info: Color(red: 0.21, green: 0.47, blue: 0.88)
        case .success: Color(red: 0.16, green: 0.58, blue: 0.37)
        case .warning: Color(red: 0.76, green: 0.48, blue: 0.11)
        }
    }

    var background: Color {
        switch self {
        case .neutral: Color(red: 0.95, green: 0.96, blue: 0.98)
        case .info: Color(red: 0.90, green: 0.95, blue: 1.0)
        case .success: Color(red: 0.90, green: 0.97, blue: 0.93)
        case .warning: Color(red: 1.0, green: 0.95, blue: 0.88)
        }
    }
}

private struct StatusPill: View {
    let text: String
    let tone: PillTone

    var body: some View {
        Text(text)
            .font(.system(size: 12.5, weight: .semibold))
            .foregroundStyle(tone.foreground)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(tone.background)
            .clipShape(Capsule())
    }
}

private struct PathPill: View {
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "doc.on.doc")
                .font(.system(size: 12, weight: .semibold))
            Text(text)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .font(.system(size: 12.5, weight: .medium))
        .foregroundStyle(Color(red: 0.24, green: 0.28, blue: 0.34))
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Color(red: 0.96, green: 0.97, blue: 0.99))
        .clipShape(Capsule())
        .overlay(
            Capsule()
                .stroke(Color(red: 0.91, green: 0.93, blue: 0.96), lineWidth: 1)
        )
    }
}

private struct SoftActionButton: View {
    let title: String
    let icon: String
    let fill: Color
    let foreground: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon)
                    .font(.system(size: 12, weight: .semibold))
                Text(title)
                    .font(.system(size: 12.5, weight: .semibold))
            }
            .foregroundStyle(foreground)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(fill)
            .clipShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

#Preview {
    ContentView()
        .frame(width: 1160, height: 780)
}
