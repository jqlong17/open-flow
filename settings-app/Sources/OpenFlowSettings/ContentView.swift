import SwiftUI

private enum SettingsPane: String, CaseIterable, Identifiable {
    case general
    case vocabulary
    case recognition
    case models
    case permissions
    case diagnostics

    var id: String { rawValue }

    var title: String {
        switch self {
        case .general: "General"
        case .vocabulary: "Vocabulary"
        case .recognition: "Recognition"
        case .models: "Models"
        case .permissions: "Permissions"
        case .diagnostics: "Diagnostics"
        }
    }

    var subtitle: String {
        switch self {
        case .general:
            "Tune the core voice input behavior and how text is cleaned before paste."
        case .vocabulary:
            "Manage correction settings and the terms Open Flow should preserve across transcripts."
        case .recognition:
            "Choose between local SenseVoice and Groq Whisper, then adjust how transcription runs."
        case .models:
            "Manage local model presets, download progress, and the storage path on this Mac."
        case .permissions:
            "Check the macOS permissions Open Flow needs for recording, hotkeys, and text injection."
        case .diagnostics:
            "Inspect hotkey events, daemon logs, and download output when something feels off."
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
                    Text("返回应用")
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
                        title: pane.title,
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
                    Text(config.daemonRunning ? "运行中" : "未运行")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                }

                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([config.configFileURL])
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "folder")
                            .font(.system(size: 13, weight: .medium))
                        Text("显示配置")
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
                            title: "Something needs attention",
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
                Text(selectedPane.title)
                    .font(.system(size: 28, weight: .bold, design: .rounded))
                    .foregroundStyle(Color(red: 0.07, green: 0.09, blue: 0.13))

                Text(selectedPane.subtitle)
                    .font(.system(size: 13.5, weight: .medium))
                    .foregroundStyle(Color(red: 0.40, green: 0.45, blue: 0.54))
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            HStack(spacing: 10) {
                borderAction(title: "Refresh", icon: "arrow.clockwise") {
                    config.refreshStatus()
                    config.refreshPermissions()
                    config.checkModelReady()
                    if selectedPane == .diagnostics {
                        config.loadLogs()
                    }
                }

                borderAction(title: selectedPane == .vocabulary ? "Reveal Vocabulary" : "Reveal Config", icon: "folder") {
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
            SettingsCard(title: "Input Behavior", subtitle: "Set how you start recording and how Open Flow reacts while you speak.") {
                VStack(spacing: 0) {
                    rowDivider
                    SettingsRow(label: "Hotkey", description: "Pick the key Open Flow listens for globally.") {
                        Picker("", selection: $config.hotkey) {
                            ForEach(Array(zip(ConfigManager.hotkeys, ConfigManager.hotkeyLabels)), id: \.0) { key, label in
                                Text(label).tag(key)
                            }
                        }
                        .pickerStyle(.menu)
                        .labelsHidden()
                        .frame(width: 220)
                    }

                    rowDivider
                    SettingsRow(label: "Trigger mode", description: "Toggle works well for dictation, while hold feels more like push-to-talk.") {
                        Picker("", selection: $config.triggerMode) {
                            ForEach(Array(zip(ConfigManager.triggerModes, ConfigManager.triggerLabels)), id: \.0) { mode, label in
                                Text(label.replacingOccurrences(of: " (press start, press stop)", with: "").replacingOccurrences(of: " (hold to record)", with: "")).tag(mode)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 320)
                    }
                }
            }

            SettingsCard(title: "Text Processing", subtitle: "Shape the text after ASR before it is pasted into your editor or app.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Chinese conversion", description: "Apply ICU-based simplified/traditional conversion to the final transcription.") {
                        Picker("", selection: $config.chineseConversion) {
                            Text("None").tag("")
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
            SettingsCard(title: "Correction Settings", subtitle: "Control the optional cleanup pass that fixes names, product terms, and obvious ASR mistakes after transcription.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Enable correction", description: "Turn on the extra correction step that uses your vocabulary as hints.") {
                        Toggle("", isOn: correctionEnabledBinding)
                            .labelsHidden()
                            .toggleStyle(.switch)
                    }

                    rowDivider
                    SettingsRow(label: "Model", description: "Correction model used only when this feature is enabled.") {
                        TextField("glm-4.7-flash", text: $config.correctionModel)
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 240)
                    }

                    rowDivider
                    SettingsRow(label: "API key", description: "Stored locally and used only for the correction request.") {
                        VStack(alignment: .trailing, spacing: 8) {
                            HStack(spacing: 8) {
                                SecureField("zhipu api key", text: $config.correctionApiKey)
                                    .textFieldStyle(.roundedBorder)
                                    .frame(width: 240)

                                Link(destination: URL(string: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys")!) {
                                    subtleActionLabel(title: "API Keys", icon: "arrow.up.right.square")
                                        .fixedSize(horizontal: true, vertical: false)
                                }
                            }

                            Text("Apply for your Zhipu API Key on BigModel. The default correction model, GLM-4-Flash-250414, is an official free model and works well for trying hotword correction first.")
                                .font(.system(size: 11.5, weight: .medium))
                                .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))
                                .frame(width: 420, alignment: .trailing)
                                .multilineTextAlignment(.trailing)
                        }
                    }
                }
            }

            SettingsCard(title: "Personal Vocabulary", subtitle: "One term or phrase per line. Keep names, products, project codenames, and domain jargon here so correction stays stable.") {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("This list is saved locally on this Mac.")
                            .font(.system(size: 11.5, weight: .medium))
                            .foregroundStyle(Color(red: 0.43, green: 0.48, blue: 0.56))

                        Spacer()

                        subtleAction(title: "Open File", icon: "folder") {
                            NSWorkspace.shared.activateFileViewerSelecting([config.personalVocabularyFileURL])
                        }
                    }

                    ZStack(alignment: .topLeading) {
                        if config.personalVocabulary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            Text("Open Flow\nSenseVoice\nGLM-4-Flash-250414")
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
        }
    }

    private var recognitionPage: some View {
        VStack(alignment: .leading, spacing: pageSpacing) {
            SettingsCard(title: "Speech Recognition Provider", subtitle: "Choose the engine that powers transcription across the app.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Provider", description: "Use local SenseVoice for privacy, or Groq Whisper for cloud transcription.") {
                        Picker("", selection: $config.provider) {
                            Text("Local").tag("local")
                            Text("Groq").tag("groq")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 220)
                    }

                    rowDivider
                    HStack(alignment: .top, spacing: 14) {
                        providerBlurb(
                            title: "Local SenseVoice",
                            icon: "lock.shield",
                            accent: Color(red: 0.24, green: 0.74, blue: 0.49),
                            body: "Audio stays on-device. Best when privacy and offline use matter more than the easiest setup.",
                            isActive: config.provider == "local"
                        )

                        providerBlurb(
                            title: "Groq Whisper",
                            icon: "cloud.sun",
                            accent: Color(red: 0.23, green: 0.58, blue: 0.96),
                            body: "Fast cloud setup with Whisper models. Requires an API key and sends audio to Groq.",
                            isActive: config.provider == "groq"
                        )
                    }
                    .padding(.top, 18)
                }
            }

            if config.provider == "local" {
                SettingsCard(title: "Local Provider Details", subtitle: "Tune the local model preset and inspect its readiness at a glance.") {
                    VStack(spacing: 0) {
                        SettingsRow(label: "Preset", description: "Quantized is lighter, while FP16 aims for higher fidelity.") {
                            Picker("", selection: $config.modelPreset) {
                                Text("Quantized").tag("quantized")
                                Text("FP16").tag("fp16")
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 210)
                        }

                        rowDivider
                        SettingsRow(label: "Model status", description: "Open Flow will use the selected preset from the path below.") {
                            StatusPill(
                                text: config.modelDownloading ? "Downloading" : (config.modelReady ? "Ready" : "Not downloaded"),
                                tone: config.modelDownloading ? .info : (config.modelReady ? .success : .warning)
                            )
                        }

                        rowDivider
                        SettingsRow(label: "Model path", description: "Click to copy the resolved path, or open the folder in Finder when you need to inspect the local preset.") {
                            modelPathActions
                        }
                    }
                }
            } else {
                SettingsCard(title: "Groq Configuration", subtitle: "Connect Whisper through Groq and choose the model profile you want.") {
                    VStack(spacing: 0) {
                        SettingsRow(label: "API key", description: "You can paste a Groq key here or provide it through the GROQ_API_KEY environment variable.") {
                            VStack(alignment: .trailing, spacing: 8) {
                                SecureField("gsk_...", text: $config.groqApiKey)
                                    .textFieldStyle(.roundedBorder)
                                    .frame(width: 300)

                                StatusPill(
                                    text: !config.groqApiKey.isEmpty ? "Stored locally" : (ProcessInfo.processInfo.environment["GROQ_API_KEY"] != nil ? "From env" : "Missing"),
                                    tone: !config.groqApiKey.isEmpty || ProcessInfo.processInfo.environment["GROQ_API_KEY"] != nil ? .success : .warning
                                )
                            }
                        }

                        rowDivider
                        SettingsRow(label: "Whisper model", description: "Large v3 Turbo is the default balance of speed and cost.") {
                            Picker("", selection: $config.groqModel) {
                                Text("Large v3 Turbo").tag("whisper-large-v3-turbo")
                                Text("Large v3").tag("whisper-large-v3")
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 250)
                        }

                        rowDivider
                        SettingsRow(label: "Language hint", description: "Leave empty for auto-detect, or set values like zh, en, ja, or ko.") {
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
            SettingsCard(title: "Model Storage", subtitle: "Keep the local model assets healthy and visible so setup stays predictable.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Preset", description: "Open Flow stores quantized and FP16 in separate folders under Application Support.") {
                        Picker("", selection: $config.modelPreset) {
                            Text("Quantized").tag("quantized")
                            Text("FP16").tag("fp16")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 210)
                    }

                    rowDivider
                    SettingsRow(label: "Model status", description: "Check whether the selected preset is available locally before the daemon tries to load it.") {
                        StatusPill(
                            text: config.modelDownloading ? "Downloading" : (config.modelReady ? "Ready" : "Missing"),
                            tone: config.modelDownloading ? .info : (config.modelReady ? .success : .warning)
                        )
                    }

                    rowDivider
                    SettingsRow(label: "Download", description: "Fetch or refresh the current preset from Hugging Face when the local files are missing or outdated.") {
                        VStack(alignment: .trailing, spacing: 8) {
                            SoftActionButton(
                                title: config.modelReady ? "Re-download Model" : "Download Model",
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
                                Text(config.modelDownloadStatus.isEmpty ? "Preparing download..." : config.modelDownloadStatus)
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
                    SettingsRow(label: "Resolved path", description: "This is the directory the daemon will look at when loading the local ASR preset. You can copy it or open the folder directly in Finder.") {
                        modelPathActions
                    }
                }
            }

            if !config.modelDownloadOutput.isEmpty {
                SettingsCard(title: "Download Output", subtitle: "Raw command output for debugging downloads and confirming what happened last.") {
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
            SettingsCard(title: "macOS Permission Checklist", subtitle: "Open the right system panel when something is missing, then restart the daemon after granting access.") {
                VStack(spacing: 0) {
                    permissionSettingsRow(
                        title: "Accessibility",
                        description: "Needed for global hotkey detection and text paste actions.",
                        granted: config.accessibilityGranted,
                        action: config.openAccessibilitySettings
                    )

                    rowDivider
                    permissionSettingsRow(
                        title: "Input Monitoring",
                        description: "Needed for listening to keyboard events like Fn or Right Command.",
                        granted: config.inputMonitoringGranted,
                        action: config.openInputMonitoringSettings
                    )

                    rowDivider
                    permissionSettingsRow(
                        title: "Microphone",
                        description: "Needed for recording and transcribing your speech.",
                        granted: config.microphoneGranted,
                        action: config.openMicrophoneSettings
                    )
                }
            }

            SettingsCard(title: "After granting access", subtitle: "macOS permission updates are not always live. Restarting the daemon keeps the setup predictable.") {
                SettingsRow(label: "Restart hint", description: "Once all required permissions are granted, restart Open Flow so the daemon re-checks access from a clean state.") {
                    SoftActionButton(
                        title: "Restart daemon",
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
            SettingsCard(title: "Performance Logging", subtitle: "Persist session-level timing and process resource snapshots so we can profile the voice pipeline over time.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Performance mode", description: "When enabled, Open Flow writes JSONL performance logs with end-to-end timing, CPU, and memory checkpoints.") {
                        Toggle("", isOn: performanceLoggingEnabledBinding)
                            .labelsHidden()
                            .toggleStyle(.switch)
                    }

                    rowDivider
                    SettingsRow(label: "Log location", description: "Stored separately from daemon.log so performance analysis stays easy to filter and archive.") {
                        HStack(spacing: 8) {
                            valueCapsule(config.performanceLogDirectoryURL.path)
                            subtleAction(title: "Open Folder", icon: "folder") {
                                NSWorkspace.shared.open(config.performanceLogDirectoryURL)
                            }
                        }
                    }
                }
            }
            #endif

            SettingsCard(title: "Hotkey Test", subtitle: "Listen for modifier changes and confirm the system sees the hotkey you want to use.") {
                VStack(spacing: 0) {
                    SettingsRow(label: "Listener", description: "Start monitoring and press Fn or your configured key to see raw event details.") {
                        HStack(spacing: 12) {
                            SoftActionButton(
                                title: config.hotkeyTestActive ? "Stop Listening" : "Start Listening",
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
                                text: config.hotkeyTestActive ? "Listening..." : "Idle",
                                tone: config.hotkeyTestActive ? .success : .neutral
                            )
                        }
                    }
                }
            }

            if !config.hotkeyTestLog.isEmpty {
                SettingsCard(title: "Hotkey Event Log", subtitle: "Recent global and local modifier events captured while the listener is active.") {
                    logViewer(config.hotkeyTestLog, minHeight: 170, maxHeight: 220)
                }
            }

            SettingsCard(title: "Daemon Log", subtitle: "The last 100 lines from daemon.log, useful for model loading, transcription, and permission issues.") {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        SoftActionButton(
                            title: "Refresh",
                            icon: "arrow.clockwise",
                            fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                            foreground: Color(red: 0.12, green: 0.16, blue: 0.24)
                        ) {
                            config.loadLogs()
                        }

                        SoftActionButton(
                            title: "Open in Finder",
                            icon: "folder",
                            fill: Color(red: 0.92, green: 0.95, blue: 0.99),
                            foreground: Color(red: 0.12, green: 0.16, blue: 0.24)
                        ) {
                            NSWorkspace.shared.activateFileViewerSelecting([config.logFileURL])
                        }

                        Spacer()
                    }

                    logViewer(config.logContent.isEmpty ? "Loading..." : config.logContent, minHeight: 220, maxHeight: 320)
                }
            }

            if !config.modelDownloadOutput.isEmpty {
                SettingsCard(title: "Model Download Output", subtitle: "Command output from the latest model download run.") {
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
                        ? "Saved locally. Restarting daemon so the new settings take effect."
                        : "Saved locally. Launch Open Flow when you are ready to use the new settings.",
                    icon: "checkmark.circle.fill",
                    tint: Color(red: 0.24, green: 0.74, blue: 0.49)
                )
            }

            if showCopyConfirmation {
                miniStatus(
                    text: "Model path copied.",
                    icon: "doc.on.doc.fill",
                    tint: Color(red: 0.23, green: 0.58, blue: 0.96)
                )
            }

            Spacer()

            Text("Changes are written to the local config and vocabulary files.")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Color(red: 0.46, green: 0.51, blue: 0.59))

            SoftActionButton(
                title: "Save Changes",
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
                StatusPill(text: granted ? "Granted" : "Needs Access", tone: granted ? .success : .warning)
                SoftActionButton(
                    title: "Open Settings",
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

            StatusPill(text: isActive ? "Selected" : "Available", tone: isActive ? .info : .neutral)
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

            Button("Dismiss", action: dismiss)
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

    private var modelPathActions: some View {
        HStack(spacing: 8) {
            Button {
                copyModelPath()
            } label: {
                PathPill(text: config.resolvedModelPath)
            }
            .buttonStyle(.plain)
            .help("Copy model path")

            subtleAction(title: "Open Folder", icon: "folder") {
                config.openModelFolder()
            }
            .help("Open the model folder in Finder")
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
