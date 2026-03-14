import SwiftUI

struct ContentView: View {
    @StateObject private var config = ConfigManager()
    @State private var selectedTab = 0
    @State private var showSaveConfirmation = false

    var body: some View {
        VStack(spacing: 0) {
            // Header with daemon status
            headerView
            Divider()

            // Tab content
            TabView(selection: $selectedTab) {
                generalTab
                    .tabItem { Label("General", systemImage: "gearshape") }
                    .tag(0)

                providerTab
                    .tabItem { Label("Provider", systemImage: "brain.head.profile") }
                    .tag(1)

                modelTab
                    .tabItem { Label("Model", systemImage: "arrow.down.circle") }
                    .tag(2)

                testTab
                    .tabItem { Label("Test", systemImage: "play.circle") }
                    .tag(3)

                logsTab
                    .tabItem { Label("Logs", systemImage: "doc.text") }
                    .tag(4)
            }
            .padding(.top, 8)

            // Error bar
            if !config.lastError.isEmpty {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                    Text(config.lastError)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(2)
                    Spacer()
                    Button("Dismiss") { config.lastError = "" }
                        .buttonStyle(.borderless)
                        .font(.caption)
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 6)
                .background(.red.opacity(0.1))
            }

            Divider()

            // Bottom bar
            bottomBar
        }
    }

    // MARK: - Header

    private var headerView: some View {
        HStack(spacing: 12) {
            Image(systemName: "waveform.circle.fill")
                .font(.system(size: 28))
                .foregroundStyle(.blue)

            VStack(alignment: .leading, spacing: 2) {
                Text("Open Flow")
                    .font(.title3.bold())
                HStack(spacing: 6) {
                    Circle()
                        .fill(config.daemonRunning ? .green : .red)
                        .frame(width: 8, height: 8)
                    Text(config.daemonRunning
                         ? "Running (PID \(config.daemonPID))"
                         : "Not Running")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if !config.daemonUptime.isEmpty {
                        Text("uptime \(config.daemonUptime)")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                }
            }

            Spacer()

            // Daemon controls
            if config.daemonRunning {
                Button("Restart") { config.restartDaemon() }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                Button("Stop") { config.stopDaemon() }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .tint(.red)
                Button("Force Quit") { config.forceQuitAll() }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .tint(.red)
                    .help("Kill all open-flow processes")
            } else {
                Button("Start Daemon") { config.startDaemon() }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
    }

    // MARK: - General Tab

    private var generalTab: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                SettingsSection(title: "Hotkey", icon: "keyboard") {
                    Picker("Key", selection: $config.hotkey) {
                        ForEach(Array(zip(ConfigManager.hotkeys, ConfigManager.hotkeyLabels)), id: \.0) { key, label in
                            Text(label).tag(key)
                        }
                    }
                    .pickerStyle(.radioGroup)

                    Divider()

                    Picker("Trigger Mode", selection: $config.triggerMode) {
                        ForEach(Array(zip(ConfigManager.triggerModes, ConfigManager.triggerLabels)), id: \.0) { mode, label in
                            Text(label).tag(mode)
                        }
                    }
                    .pickerStyle(.radioGroup)
                }

                // Chinese conversion
                SettingsSection(title: "Text Processing", icon: "textformat.abc") {
                    Picker("Chinese Conversion", selection: $config.chineseConversion) {
                        Text("None").tag("")
                        Text("Simplified → Traditional (簡→繁)").tag("s2t")
                        Text("Traditional → Simplified (繁→簡)").tag("t2s")
                    }
                    .pickerStyle(.radioGroup)

                    Text("Uses macOS native ICU transform. Applied to transcription output before pasting.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                // Permissions section
                SettingsSection(title: "macOS Permissions", icon: "lock.shield") {
                    permissionRow(
                        name: "Accessibility",
                        detail: "Required for global hotkey detection and text paste",
                        granted: config.accessibilityGranted,
                        action: { config.openAccessibilitySettings() }
                    )

                    Divider()

                    permissionRow(
                        name: "Input Monitoring",
                        detail: "Required for listening to keyboard events",
                        granted: config.inputMonitoringGranted,
                        action: { config.openInputMonitoringSettings() }
                    )

                    Divider()

                    permissionRow(
                        name: "Microphone",
                        detail: "Required for recording audio",
                        granted: config.microphoneGranted,
                        action: { config.openMicrophoneSettings() }
                    )

                    Divider()

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Look for \"Open Flow\" in System Settings.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                        Text("After granting permissions, restart the daemon for changes to take effect.")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                }
            }
            .padding(20)
        }
    }

    private func permissionRow(name: String, detail: String, granted: Bool, action: @escaping () -> Void) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(name)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if granted {
                Label("Granted", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                    .font(.callout)
            } else {
                Button("Open Settings") { action() }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
        }
    }

    // MARK: - Provider Tab

    private var providerTab: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                SettingsSection(title: "Speech Recognition Provider", icon: "brain.head.profile") {
                    Picker("Provider", selection: $config.provider) {
                        Text("Local (SenseVoice ONNX)").tag("local")
                        Text("Groq Cloud (Whisper API)").tag("groq")
                    }
                    .pickerStyle(.segmented)

                    if config.provider == "local" {
                        HStack {
                            Image(systemName: "checkmark.shield.fill")
                                .foregroundStyle(.green)
                            Text("All processing is done locally. No data leaves your machine.")
                                .font(.callout)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                if config.provider == "groq" {
                    SettingsSection(title: "Groq API Settings", icon: "cloud") {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text("API Key")
                                Spacer()
                                if !config.groqApiKey.isEmpty {
                                    Label("Set", systemImage: "checkmark.circle.fill")
                                        .font(.caption)
                                        .foregroundStyle(.green)
                                } else if ProcessInfo.processInfo.environment["GROQ_API_KEY"] != nil {
                                    Label("From env", systemImage: "checkmark.circle.fill")
                                        .font(.caption)
                                        .foregroundStyle(.blue)
                                }
                            }

                            SecureField("gsk_...", text: $config.groqApiKey)
                                .textFieldStyle(.roundedBorder)

                            Text("Or set GROQ_API_KEY environment variable. Get a free key at console.groq.com")
                                .font(.caption)
                                .foregroundStyle(.secondary)

                            Divider()

                            Picker("Whisper Model", selection: $config.groqModel) {
                                Text("Large v3 Turbo (faster, cheaper)").tag("whisper-large-v3-turbo")
                                Text("Large v3 (more accurate)").tag("whisper-large-v3")
                            }

                            Divider()

                            HStack {
                                Text("Language")
                                TextField("auto", text: $config.groqLanguage)
                                    .textFieldStyle(.roundedBorder)
                                    .frame(maxWidth: 100)
                            }
                            Text("Leave empty for auto-detect. Examples: en, zh, ja, ko, es, fr")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .padding(20)
        }
    }

    // MARK: - Model Tab

    private var modelTab: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                SettingsSection(title: "SenseVoice Model (Local ASR)", icon: "cpu") {
                    HStack {
                        Text("Status")
                        Spacer()
                        if config.modelReady {
                            Label("Ready", systemImage: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        } else {
                            Label("Not found", systemImage: "xmark.circle.fill")
                                .foregroundStyle(.red)
                        }
                    }

                    HStack {
                        Text("Path")
                        Spacer()
                        Text(config.resolvedModelPath)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }

                    Divider()

                    HStack {
                        Button(config.modelReady ? "Re-download Model" : "Download Model") {
                            config.downloadModel()
                        }
                        .disabled(config.modelDownloading)

                        if config.modelDownloading {
                            ProgressView()
                                .scaleEffect(0.7)
                        }
                    }

                    Text("Downloads SenseVoice-Small (~230 MB) from Hugging Face")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if !config.modelDownloadOutput.isEmpty {
                    SettingsSection(title: "Download Output", icon: "terminal") {
                        ScrollView {
                            Text(config.modelDownloadOutput)
                                .font(.system(.caption, design: .monospaced))
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .textSelection(.enabled)
                        }
                        .frame(maxHeight: 200)
                    }
                }
            }
            .padding(20)
        }
    }

    // MARK: - Test Tab

    private var testTab: some View {
        VStack(alignment: .leading, spacing: 16) {
            SettingsSection(title: "Hotkey Test", icon: "keyboard.badge.eye") {
                Text("Test that your hotkey (Fn key, Right Cmd, etc.) is being detected by the system.")
                    .font(.callout)
                    .foregroundStyle(.secondary)

                HStack {
                    if config.hotkeyTestActive {
                        Button("Stop Test") {
                            config.stopHotkeyTest()
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)

                        Circle()
                            .fill(.green)
                            .frame(width: 10, height: 10)
                            .overlay(
                                Circle()
                                    .fill(.green.opacity(0.3))
                                    .frame(width: 20, height: 20)
                            )

                        Text("Listening...")
                            .font(.callout)
                            .foregroundStyle(.green)
                    } else {
                        Button("Start Listening") {
                            config.startHotkeyTest()
                        }
                        .buttonStyle(.borderedProminent)

                        Text("Press to start monitoring key events")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if !config.hotkeyTestLog.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Event Log")
                        .font(.caption.bold())
                        .foregroundStyle(.secondary)

                    TextEditor(text: .constant(config.hotkeyTestLog))
                        .font(.system(.caption, design: .monospaced))
                        .scrollContentBackground(.hidden)
                        .background(.quaternary.opacity(0.3))
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .frame(minHeight: 150)
                }
            }

            Text("Look for fn=DOWN when you press the Fn key. If nothing appears, Accessibility permission may be missing.")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(20)
    }

    // MARK: - Logs Tab

    private var logsTab: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Daemon Log (last 100 lines)")
                    .font(.headline)
                Spacer()
                Button("Refresh") { config.loadLogs() }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                Button("Open in Finder") {
                    NSWorkspace.shared.activateFileViewerSelecting([config.logFileURL])
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)

            if config.logContent.isEmpty {
                VStack {
                    Spacer()
                    Text("Loading...")
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                TextEditor(text: .constant(config.logContent))
                    .font(.system(.caption, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .background(.quaternary.opacity(0.3))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .padding(.horizontal, 20)
                    .padding(.bottom, 8)
            }
        }
        .onAppear { config.loadLogs() }
    }

    // MARK: - Bottom Bar

    private var bottomBar: some View {
        HStack {
            Button("Reveal Config") {
                NSWorkspace.shared.activateFileViewerSelecting([config.configFileURL])
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            Spacer()

            if showSaveConfirmation {
                Label("Saved", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                    .font(.callout)
                    .transition(.opacity)
            }

            Button("Save & Apply") {
                config.save()
                withAnimation { showSaveConfirmation = true }
                DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                    withAnimation { showSaveConfirmation = false }
                }
                if config.daemonRunning {
                    config.restartDaemon()
                }
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .keyboardShortcut("s", modifiers: .command)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 10)
    }
}

struct SettingsSection<Content: View>: View {
    let title: String
    let icon: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: icon)
                .font(.headline)

            VStack(alignment: .leading, spacing: 10) {
                content
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
    }
}

#Preview {
    ContentView()
        .frame(width: 520, height: 480)
}
