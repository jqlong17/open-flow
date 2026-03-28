import Foundation
import Combine
import AppKit
import AVFoundation

private struct PermissionSnapshotItem: Decodable {
    let status: String
    let granted: Bool
    let canPrompt: Bool
    let source: String

    init(status: String, granted: Bool, canPrompt: Bool, source: String) {
        self.status = status
        self.granted = granted
        self.canPrompt = canPrompt
        self.source = source
    }

    enum CodingKeys: String, CodingKey {
        case status
        case granted
        case canPrompt = "can_prompt"
        case source
    }

    init(from decoder: Decoder) throws {
        if let container = try? decoder.singleValueContainer(),
           let granted = try? container.decode(Bool.self) {
            self.status = granted ? "authorized" : "unknown"
            self.granted = granted
            self.canPrompt = !granted
            self.source = "legacy_bool_snapshot"
            return
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        status = try container.decode(String.self, forKey: .status)
        granted = try container.decode(Bool.self, forKey: .granted)
        canPrompt = try container.decodeIfPresent(Bool.self, forKey: .canPrompt) ?? !granted
        source = try container.decodeIfPresent(String.self, forKey: .source) ?? "unknown"
    }
}

private struct PermissionSnapshot: Decodable {
    let accessibility: PermissionSnapshotItem
    let inputMonitoring: PermissionSnapshotItem
    let microphone: PermissionSnapshotItem

    enum CodingKeys: String, CodingKey {
        case accessibility
        case inputMonitoring = "input_monitoring"
        case microphone
    }
}

struct InputDeviceOption: Identifiable, Decodable, Hashable {
    let name: String
    let isDefault: Bool

    var id: String { name }

    enum CodingKeys: String, CodingKey {
        case name
        case isDefault = "is_default"
    }
}

private struct InputDeviceSnapshot: Decodable {
    let defaultDeviceName: String?
    let devices: [InputDeviceOption]

    enum CodingKeys: String, CodingKey {
        case defaultDeviceName = "default_device_name"
        case devices
    }
}

struct SystemAudioDisplayOption: Identifiable, Decodable, Hashable {
    let id: UInt32
    let width: Int
    let height: Int

    var title: String {
        "Display \(id) • \(width)x\(height)"
    }
}

struct SystemAudioApplicationOption: Identifiable, Decodable, Hashable {
    let processID: Int32
    let bundleIdentifier: String
    let applicationName: String

    var id: Int32 { processID }

    enum CodingKeys: String, CodingKey {
        case processID = "process_id"
        case bundleIdentifier = "bundle_identifier"
        case applicationName = "application_name"
    }
}

private struct SystemAudioPermissionSnapshot: Decodable {
    let screenRecording: Bool

    enum CodingKeys: String, CodingKey {
        case screenRecording = "screen_recording"
    }
}

private struct SystemAudioShareableSnapshot: Decodable {
    let screenRecordingGranted: Bool
    let displays: [SystemAudioDisplayOption]
    let applications: [SystemAudioApplicationOption]
    let windowCount: Int

    enum CodingKeys: String, CodingKey {
        case screenRecordingGranted = "screen_recording_granted"
        case displays
        case applications
        case windowCount = "window_count"
    }
}

private struct SystemAudioProbeSnapshot: Decodable {
    let mode: String
    let target: String
    let durationSeconds: Double
    let callbacks: Int
    let sampleCount: Int
    let sampleRate: Double
    let channels: Int
    let firstCallbackLatencyMs: Double?
    let error: String?

    enum CodingKeys: String, CodingKey {
        case mode
        case target
        case durationSeconds = "duration_seconds"
        case callbacks
        case sampleCount = "sample_count"
        case sampleRate = "sample_rate"
        case channels
        case firstCallbackLatencyMs = "first_callback_latency_ms"
        case error
    }
}

/// Manages reading/writing Open Flow's config.toml and daemon lifecycle
class ConfigManager: ObservableObject {
    enum MicrophonePermissionState: Equatable {
        case notDetermined
        case restricted
        case denied
        case authorized
        case unknown

        var isAuthorized: Bool {
            if case .authorized = self {
                return true
            }
            return false
        }

        init(snapshotStatus: String) {
            switch snapshotStatus {
            case "authorized":
                self = .authorized
            case "denied":
                self = .denied
            case "restricted":
                self = .restricted
            case "not_determined":
                self = .notDetermined
            default:
                self = .unknown
            }
        }
    }

    // Config fields
    @Published var uiLanguage: String = "zh"
    @Published var provider: String = "local"
    @Published var correctionEnabled: String = ""
    @Published var correctionModel: String = "GLM-4.7-Flash"
    @Published var correctionApiKey: String = ""
    @Published var modelPreset: String = "quantized"
    @Published var groqApiKey: String = ""
    @Published var groqModel: String = "whisper-large-v3-turbo"
    @Published var groqLanguage: String = ""
    @Published var hotkey: String = "right_cmd"
    @Published var triggerMode: String = "toggle"
    @Published var captureMode: String = "microphone"
    @Published var inputSource: String = ""
    @Published var systemAudioTargetPID: String = ""
    @Published var systemAudioTargetName: String = ""
    @Published var systemAudioTargetBundleID: String = ""
    @Published var chineseConversion: String = ""
    @Published var performanceLogEnabled: String = ""
    @Published var modelPath: String = ""
    @Published var availableInputDevices: [InputDeviceOption] = []
    @Published var detectedDefaultInputDeviceName: String = ""

    // Daemon status
    @Published var daemonRunning = false
    @Published var daemonPID: String = ""
    @Published var daemonUptime: String = ""
    @Published var lastError: String = ""

    // Permissions
    @Published var accessibilityGranted = false
    @Published var inputMonitoringGranted = false
    @Published var microphoneGranted = false
    @Published var microphonePermissionState: MicrophonePermissionState = .unknown
    @Published var screenRecordingGranted = false

    // Hotkey test
    @Published var hotkeyTestActive = false
    @Published var hotkeyTestLog: String = ""

    // Model status
    @Published var modelReady = false
    @Published var modelDownloading = false
    @Published var modelDownloadProgress: Double = 0
    @Published var modelDownloadStatus: String = ""
    @Published var modelDownloadOutput: String = ""

    // Log
    @Published var logContent: String = ""
    @Published var personalVocabulary: String = ""
    @Published var correctionSystemPrompt: String = ""
    @Published var systemAudioScreenRecordingGranted = false
    @Published var systemAudioDisplays: [SystemAudioDisplayOption] = []
    @Published var systemAudioApplications: [SystemAudioApplicationOption] = []
    @Published var systemAudioWindowCount = 0
    @Published var systemAudioStatus: String = ""
    @Published var systemAudioDesktopProbeSummary: String = ""
    @Published var systemAudioApplicationProbeSummary: String = ""
    @Published var systemAudioProbeRunning = false
    @Published var selectedSystemAudioApplicationPID: String = ""
    @Published var meetingSessionCount = 0
    @Published var latestMeetingSessionName: String = ""
    @Published var latestMeetingSessionUpdatedAt: String = ""
    @Published var latestMeetingSessionStatus: String = ""
    @Published var latestMeetingSessionHasTranscript = false

    static let groqModels = ["whisper-large-v3-turbo", "whisper-large-v3"]
    static let localModelPresets = ["quantized", "fp16"]
    static let correctionModels = [
        "GLM-4.7-Flash",
        "GLM-4.6V-Flash",
        "GLM-4.1V-Thinking-Flash",
        "GLM-4-Flash-250414",
        "GLM-4V-Flash"
    ]
    static let hotkeys = ["right_cmd", "right_option", "right_control", "right_shift", "fn", "f13"]
    static let triggerModes = ["toggle", "hold"]
    static let captureModes = ["microphone", "system_audio_desktop", "system_audio_microphone", "system_audio_application"]

    private var configPath: URL
    private var dataDir: URL
    private var statusTimer: Timer?

    init() {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let configDir = appSupport.appendingPathComponent("com.openflow.open-flow")
        dataDir = appSupport.appendingPathComponent("com.openflow.open-flow")
        configPath = configDir.appendingPathComponent("config.toml")

        try? FileManager.default.createDirectory(at: configDir, withIntermediateDirectories: true)

        load()
        refreshInputDevices()
        refreshStatus()
        refreshPermissions()
        refreshSystemAudioDiagnostics()
        refreshMeetingSessionsOverview()

        statusTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            guard let self = self else { return }
            let wasRunningBefore = self.daemonRunning
            self.refreshStatus()

            if wasRunningBefore && !self.daemonRunning {
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                    NSApplication.shared.terminate(nil)
                }
            }
        }
    }

    deinit {
        statusTimer?.invalidate()
    }

    // MARK: - Permissions

    func refreshPermissions() {
        #if os(macOS)
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            let snapshot = self.fetchPermissionSnapshotFromOpenFlow()
            let ax = snapshot?.accessibility ?? self.checkAccessibility()
            let im = snapshot?.inputMonitoring ?? self.checkInputMonitoring()
            let micState = self.resolveMicrophonePermissionState(from: snapshot?.microphone)
            let mic = micState.isAuthorized

            DispatchQueue.main.async {
                self.accessibilityGranted = ax.granted
                self.inputMonitoringGranted = im.granted
                self.microphonePermissionState = micState
                self.microphoneGranted = mic
                self.screenRecordingGranted = self.systemAudioScreenRecordingGranted
            }
        }
        #endif
    }

    private func fetchPermissionSnapshotFromOpenFlow() -> PermissionSnapshot? {
        guard let binary = findOpenFlowBinary() else { return nil }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = ["permissions", "--json"]

        let stdout = Pipe()
        let stderr = Pipe()
        task.standardOutput = stdout
        task.standardError = stderr

        do {
            try task.run()
            task.waitUntilExit()
        } catch {
            return nil
        }

        guard task.terminationStatus == 0 else {
            return nil
        }

        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return nil }
        return try? JSONDecoder().decode(PermissionSnapshot.self, from: data)
    }

    private func checkAccessibility() -> PermissionSnapshotItem {
        // AXIsProcessTrusted checks if THIS process (same bundle) has accessibility
        typealias AXFunc = @convention(c) () -> Bool
        guard let handle = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_LAZY),
              let sym = dlsym(handle, "AXIsProcessTrusted") else {
            return PermissionSnapshotItem(status: "unknown", granted: false, canPrompt: true, source: "ax_is_process_trusted")
        }
        let fn = unsafeBitCast(sym, to: AXFunc.self)
        let granted = fn()
        return PermissionSnapshotItem(
            status: granted ? "authorized" : "needs_manual_grant",
            granted: granted,
            canPrompt: !granted,
            source: "ax_is_process_trusted"
        )
    }

    private func checkInputMonitoring() -> PermissionSnapshotItem {
        guard let handle = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_LAZY),
              let sym = dlsym(handle, "CGPreflightListenEventAccess") else {
            return PermissionSnapshotItem(status: "unknown", granted: false, canPrompt: true, source: "cg_preflight_listen_event_access")
        }
        typealias Func = @convention(c) () -> Bool
        let fn = unsafeBitCast(sym, to: Func.self)
        let granted = fn()
        return PermissionSnapshotItem(
            status: granted ? "authorized" : "needs_manual_grant",
            granted: granted,
            canPrompt: !granted,
            source: "cg_preflight_listen_event_access"
        )
    }

    private var nativeMicrophonePermissionState: MicrophonePermissionState {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .notDetermined:
            return .notDetermined
        case .restricted:
            return .restricted
        case .denied:
            return .denied
        case .authorized:
            return .authorized
        @unknown default:
            return .unknown
        }
    }

    private func resolveMicrophonePermissionState(from snapshot: PermissionSnapshotItem?) -> MicrophonePermissionState {
        if let snapshot {
            return MicrophonePermissionState(snapshotStatus: snapshot.status)
        }
        return nativeMicrophonePermissionState
    }

    var microphonePermissionStatusText: String {
        switch microphonePermissionState {
        case .authorized:
            return usesEnglish ? "Granted" : "已授权"
        case .notDetermined:
            return usesEnglish ? "Not Requested" : "未请求"
        case .denied:
            return usesEnglish ? "Denied" : "已拒绝"
        case .restricted:
            return usesEnglish ? "Restricted" : "受限制"
        case .unknown:
            return usesEnglish ? "Unknown" : "未知"
        }
    }

    var microphonePermissionActionTitle: String {
        switch microphonePermissionState {
        case .authorized:
            return usesEnglish ? "Open Settings" : "打开设置"
        case .denied, .restricted:
            return usesEnglish ? "Open Settings" : "打开设置"
        case .notDetermined, .unknown:
            return usesEnglish ? "Request Access" : "请求授权"
        }
    }

    func resolveMicrophonePermission() {
        switch microphonePermissionState {
        case .authorized:
            openMicrophoneSettings()
        case .denied, .restricted:
            openMicrophoneSettings()
        case .notDetermined, .unknown:
            requestMicrophonePermission()
        }
    }

    private func requestMicrophonePermission() {
        AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.refreshPermissions()
                if granted {
                    self.lastError = ""
                } else {
                    self.lastError = self.usesEnglish
                        ? "Microphone access was not granted. Please enable Open Flow in System Settings."
                        : "麦克风授权未完成，请在系统设置里为 Open Flow 打开麦克风权限。"
                    self.openMicrophoneSettings()
                }
            }
        }
    }

    private func openSystemSettings(candidates: [String]) {
        let fallbackCandidates = candidates + [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
            "x-apple.systempreferences:com.apple.preference.security",
        ]

        for candidate in fallbackCandidates {
            guard let url = URL(string: candidate) else { continue }
            if NSWorkspace.shared.open(url) {
                return
            }
        }

        DispatchQueue.main.async {
            self.lastError = self.usesEnglish
                ? "Failed to open System Settings automatically. Please open Privacy & Security manually."
                : "无法自动打开系统设置，请手动前往“隐私与安全性”。"
        }
    }

    func openAccessibilitySettings() {
        openSystemSettings(candidates: [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        ])
    }

    func openMicrophoneSettings() {
        openSystemSettings(candidates: [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        ])
    }

    func openInputMonitoringSettings() {
        openSystemSettings(candidates: [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ListenEvent",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?InputAccessories",
        ])
    }

    func openScreenRecordingSettings() {
        openSystemSettings(candidates: [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        ])
    }

    func copyModelPathToClipboard() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(resolvedModelPath, forType: .string)
    }

    func openModelFolder() {
        let fileManager = FileManager.default
        let resolvedURL = URL(fileURLWithPath: resolvedModelPath)

        if fileManager.fileExists(atPath: resolvedURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([resolvedURL])
            return
        }

        let parentURL = resolvedURL.deletingLastPathComponent()
        if fileManager.fileExists(atPath: parentURL.path) {
            NSWorkspace.shared.open(parentURL)
        } else {
            NSWorkspace.shared.open(dataDir)
        }
    }

    // MARK: - Config I/O

    func load() {
        guard let content = try? String(contentsOf: configPath, encoding: .utf8) else { return }

        for line in content.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard let (key, value) = parseTOMLLine(trimmed) else { continue }

            switch key {
            case "ui_language": uiLanguage = value.isEmpty ? "zh" : value
            case "provider": provider = value
            case "correction_enabled": correctionEnabled = value
            case "correction_model": correctionModel = value.isEmpty ? "GLM-4.7-Flash" : value
            case "correction_api_key": correctionApiKey = value
            case "model_preset": modelPreset = value.isEmpty ? "quantized" : value
            case "groq_api_key": groqApiKey = value
            case "groq_model": groqModel = value
            case "groq_language": groqLanguage = value
            case "hotkey": hotkey = value
            case "trigger_mode": triggerMode = value
            case "capture_mode": captureMode = value.isEmpty ? "microphone" : value
            case "input_source": inputSource = value
            case "system_audio_target_pid": systemAudioTargetPID = value
            case "system_audio_target_name": systemAudioTargetName = value
            case "system_audio_target_bundle_id": systemAudioTargetBundleID = value
            case "chinese_conversion": chineseConversion = value
            case "performance_log_enabled": performanceLogEnabled = value
            case "model_path": modelPath = value
            default: break
            }
        }

        checkModelReady()
        loadPersonalVocabulary()
        loadCorrectionSystemPrompt()
    }

    func save() {
        var existingLines: [String] = []
        var knownKeys = Set<String>()

        if let content = try? String(contentsOf: configPath, encoding: .utf8) {
            existingLines = content.components(separatedBy: "\n")
        }

        let ourValues: [(String, String)] = [
            ("ui_language", normalizedUiLanguage),
            ("provider", provider),
            ("correction_enabled", correctionEnabled),
            ("correction_model", normalizedCorrectionModel),
            ("correction_api_key", correctionApiKey),
            ("model_preset", normalizedModelPreset),
            ("groq_api_key", groqApiKey),
            ("groq_model", groqModel),
            ("groq_language", groqLanguage),
            ("hotkey", hotkey),
            ("trigger_mode", triggerMode),
            ("capture_mode", normalizedCaptureMode),
            ("input_source", inputSource),
            ("system_audio_target_pid", systemAudioTargetPID),
            ("system_audio_target_name", systemAudioTargetName),
            ("system_audio_target_bundle_id", systemAudioTargetBundleID),
            ("chinese_conversion", chineseConversion),
            ("performance_log_enabled", performanceLogEnabled),
            ("model_path", modelPath),
        ]

        let ourKeys = Set(ourValues.map { $0.0 })
        var outputLines: [String] = []

        for line in existingLines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let (key, _) = parseTOMLLine(trimmed), ourKeys.contains(key) {
                if let val = ourValues.first(where: { $0.0 == key }) {
                    outputLines.append("\(key) = \"\(val.1)\"")
                    knownKeys.insert(key)
                }
            } else {
                outputLines.append(line)
            }
        }

        for (key, value) in ourValues where !knownKeys.contains(key) {
            outputLines.append("\(key) = \"\(value)\"")
        }

        while outputLines.last?.trimmingCharacters(in: .whitespaces).isEmpty == true {
            outputLines.removeLast()
        }

        let output = outputLines.joined(separator: "\n") + "\n"
        do {
            try output.write(to: configPath, atomically: true, encoding: .utf8)
        } catch {
            DispatchQueue.main.async {
                self.lastError = self.usesEnglish
                    ? "Failed to save config: \(error.localizedDescription)"
                    : "保存配置失败：\(error.localizedDescription)"
            }
        }
    }

    var normalizedCorrectionModel: String {
        let trimmed = correctionModel.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "GLM-4.7-Flash" : trimmed
    }

    var normalizedCaptureMode: String {
        switch captureMode.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "system_audio_desktop":
            return "system_audio_desktop"
        case "system_audio_microphone":
            return "system_audio_microphone"
        case "system_audio_application":
            return "system_audio_application"
        default:
            return "microphone"
        }
    }

    var normalizedUiLanguage: String {
        let trimmed = uiLanguage.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.hasPrefix("en") ? "en" : "zh"
    }

    var usesEnglish: Bool {
        normalizedUiLanguage == "en"
    }

    var correctionIsEnabled: Bool {
        let value = correctionEnabled.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return value == "true" || value == "1" || value == "yes" || value == "on" || value == "enabled"
    }

    func setCorrectionEnabled(_ enabled: Bool) {
        correctionEnabled = enabled ? "true" : "false"
    }

    var performanceLoggingIsEnabled: Bool {
        let value = performanceLogEnabled.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return value == "true" || value == "1" || value == "yes" || value == "on" || value == "enabled"
    }

    func setPerformanceLoggingEnabled(_ enabled: Bool) {
        performanceLogEnabled = enabled ? "true" : "false"
    }

    var personalVocabularyFileURL: URL {
        dataDir.appendingPathComponent("personal_vocabulary.txt")
    }

    func loadPersonalVocabulary() {
        guard let content = try? String(contentsOf: personalVocabularyFileURL, encoding: .utf8) else {
            personalVocabulary = ""
            return
        }
        personalVocabulary = content
    }

    func savePersonalVocabulary() {
        do {
            try personalVocabulary.write(to: personalVocabularyFileURL, atomically: true, encoding: .utf8)
        } catch {
            DispatchQueue.main.async {
                self.lastError = self.usesEnglish
                    ? "Failed to save personal vocabulary: \(error.localizedDescription)"
                    : "保存个人词表失败：\(error.localizedDescription)"
            }
        }
    }

    var correctionSystemPromptFileURL: URL {
        dataDir.appendingPathComponent("correction_system_prompt.txt")
    }

    var defaultCorrectionSystemPrompt: String {
        """
        你是一个语音转写轻量纠错器。你的唯一任务是修正明显的 ASR 识别错误。
        规则：
        1. 只修正明显错误，不要改写句式，不要润色，不要总结，不要解释。
        2. 不要补充用户没说过的事实，不要扩写。
        3. 如果原文已经合理，就原样输出。
        4. 优先参考个人词表中的常用词、品牌词、人名、项目名；当原文发音或拼写接近这些词时，可纠正为词表中的标准写法。
        5. 保留原有语言风格和中英文混排方式。
        6. 最终只输出纠错后的单段文本，不要输出任何说明。

        个人词表：
        {{personal_vocabulary}}
        """
    }

    func loadCorrectionSystemPrompt() {
        guard let content = try? String(contentsOf: correctionSystemPromptFileURL, encoding: .utf8) else {
            correctionSystemPrompt = defaultCorrectionSystemPrompt
            return
        }
        let trimmed = content.trimmingCharacters(in: .whitespacesAndNewlines)
        correctionSystemPrompt = trimmed.isEmpty ? defaultCorrectionSystemPrompt : content
    }

    func saveCorrectionSystemPrompt() {
        let content = correctionSystemPrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? defaultCorrectionSystemPrompt
            : correctionSystemPrompt
        do {
            try content.write(to: correctionSystemPromptFileURL, atomically: true, encoding: .utf8)
        } catch {
            DispatchQueue.main.async {
                self.lastError = self.usesEnglish
                    ? "Failed to save correction prompt: \(error.localizedDescription)"
                    : "保存纠错提示词失败：\(error.localizedDescription)"
            }
        }
    }

    private func parseTOMLLine(_ line: String) -> (String, String)? {
        guard !line.hasPrefix("#"), !line.isEmpty else { return nil }
        let parts = line.split(separator: "=", maxSplits: 1)
        guard parts.count == 2 else { return nil }
        let key = parts[0].trimmingCharacters(in: .whitespaces)
        var value = parts[1].trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("\"") && value.hasSuffix("\"") {
            value = String(value.dropFirst().dropLast())
        }
        return (key, value)
    }

    // MARK: - Audio Devices

    func refreshInputDevices() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let snapshot = self.fetchInputDeviceSnapshotFromOpenFlow() else { return }

            DispatchQueue.main.async {
                self.availableInputDevices = snapshot.devices
                self.detectedDefaultInputDeviceName = snapshot.defaultDeviceName ?? ""
            }
        }
    }

    private func fetchInputDeviceSnapshotFromOpenFlow() -> InputDeviceSnapshot? {
        guard let binary = findOpenFlowBinary() else { return nil }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = ["audio-devices", "--json"]

        let stdout = Pipe()
        let stderr = Pipe()
        task.standardOutput = stdout
        task.standardError = stderr

        do {
            try task.run()
            task.waitUntilExit()
        } catch {
            return nil
        }

        guard task.terminationStatus == 0 else {
            return nil
        }

        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return nil }
        return try? JSONDecoder().decode(InputDeviceSnapshot.self, from: data)
    }

    var resolvedInputSourceLabel: String {
        let trimmed = inputSource.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return trimmed
        }
        if !detectedDefaultInputDeviceName.isEmpty {
            return detectedDefaultInputDeviceName
        }
        return usesEnglish ? "System Default" : "系统默认"
    }

    var resolvedCaptureModeLabel: String {
        switch normalizedCaptureMode {
        case "system_audio_desktop":
            return usesEnglish ? "Desktop Audio" : "桌面音频"
        case "system_audio_microphone":
            return usesEnglish ? "Desktop Audio + Microphone (Meeting)" : "桌面音频 + 麦克风（会议）"
        case "system_audio_application":
            return usesEnglish ? "Application Audio" : "应用音频"
        default:
            return usesEnglish ? "Microphone" : "麦克风"
        }
    }

    // MARK: - Daemon Control

    func refreshStatus() {
        let pidPath = dataDir.appendingPathComponent("daemon.pid")
        guard let pidStr = try? String(contentsOf: pidPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines),
              let pid = Int32(pidStr) else {
            DispatchQueue.main.async {
                self.daemonRunning = false
                self.daemonPID = ""
                self.daemonUptime = ""
            }
            return
        }

        let running = kill(pid, 0) == 0
        var uptime = ""
        if running {
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/bin/ps")
            task.arguments = ["-p", String(pid), "-o", "etime="]
            let pipe = Pipe()
            task.standardOutput = pipe
            try? task.run()
            task.waitUntilExit()
            uptime = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        }

        DispatchQueue.main.async {
            self.daemonRunning = running
            self.daemonPID = running ? String(pid) : ""
            self.daemonUptime = uptime
        }
    }

    private func findOpenFlowBinary() -> String? {
        // 1. Next to this settings app binary (inside .app bundle Contents/MacOS/)
        if let selfPath = Bundle.main.executableURL?.deletingLastPathComponent() {
            let bundled = selfPath.appendingPathComponent("open-flow")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled.path
            }
        }
        // 2. Common install locations
        for path in ["/usr/local/bin/open-flow", "/opt/homebrew/bin/open-flow"] {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }
        // 3. Try to find via `which`
        let which = Process()
        which.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        which.arguments = ["open-flow"]
        let pipe = Pipe()
        which.standardOutput = pipe
        which.standardError = Pipe()
        if let _ = try? which.run() {
            which.waitUntilExit()
            if which.terminationStatus == 0 {
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                if let path = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
                   !path.isEmpty {
                    return path
                }
            }
        }
        return nil
    }

    private func findSystemAudioHelperBinary() -> String? {
        if let selfPath = Bundle.main.executableURL?.deletingLastPathComponent() {
            let bundled = selfPath.appendingPathComponent("OpenFlowSystemAudioHelper")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled.path
            }
        }

        let appSupportCandidates = [
            "settings-app/.build/arm64-apple-macosx/release/OpenFlowSystemAudioHelper",
            "settings-app/.build/x86_64-apple-macosx/release/OpenFlowSystemAudioHelper",
            "settings-app/.build/release/OpenFlowSystemAudioHelper",
        ]

        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()

        for relativePath in appSupportCandidates {
            let candidate = repoRoot.appendingPathComponent(relativePath)
            if FileManager.default.isExecutableFile(atPath: candidate.path) {
                return candidate.path
            }
        }

        return nil
    }

    /// Find the .app bundle containing this settings binary
    private func findAppBundle() -> URL? {
        // Walk up from our binary to find the .app bundle
        var url = Bundle.main.executableURL?.deletingLastPathComponent()
        for _ in 0..<5 {
            guard let u = url else { break }
            if u.lastPathComponent.hasSuffix(".app") {
                return u
            }
            url = u.deletingLastPathComponent()
        }
        return nil
    }

    func startDaemon() {
        save()
        lastError = ""

        // Strategy 1: Relaunch the .app bundle (foreground mode, best UX)
        if let appBundle = findAppBundle() {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                let task = Process()
                task.executableURL = URL(fileURLWithPath: "/usr/bin/open")
                task.arguments = [appBundle.path]
                do {
                    try task.run()
                    task.waitUntilExit()
                } catch {
                    DispatchQueue.main.async {
                        self?.lastError = self?.usesEnglish == true
                            ? "Failed to launch app: \(error.localizedDescription)"
                            : "启动应用失败：\(error.localizedDescription)"
                    }
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                    self?.refreshStatus()
                }
            }
            return
        }

        // Strategy 2: Use open-flow CLI binary (background mode)
        guard let binary = findOpenFlowBinary() else {
            lastError = usesEnglish
                ? "Cannot find open-flow binary or .app bundle. Searched: \(searchedPaths())"
                : "找不到 open-flow 可执行文件或 .app 包。已搜索：\(searchedPaths())"
            return
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let task = Process()
            task.executableURL = URL(fileURLWithPath: binary)
            task.arguments = ["start"]
            let pipe = Pipe()
            task.standardOutput = pipe
            task.standardError = pipe
            do {
                try task.run()
                task.waitUntilExit()
                let output = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
                if task.terminationStatus != 0 {
                    DispatchQueue.main.async {
                        self?.lastError = self?.usesEnglish == true
                            ? "Start failed (exit \(task.terminationStatus)): \(output)"
                            : "启动失败（退出码 \(task.terminationStatus)）：\(output)"
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    self?.lastError = self?.usesEnglish == true
                        ? "Start error: \(error.localizedDescription)"
                        : "启动出错：\(error.localizedDescription)"
                }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                self?.refreshStatus()
            }
        }
    }

    func stopDaemon() {
        lastError = ""

        // Try CLI stop first, then fall back to PID kill
        if let binary = findOpenFlowBinary() {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                let task = Process()
                task.executableURL = URL(fileURLWithPath: binary)
                task.arguments = ["stop"]
                let pipe = Pipe()
                task.standardOutput = pipe
                task.standardError = pipe
                try? task.run()
                task.waitUntilExit()

                DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                    self?.refreshStatus()
                }
            }
        } else {
            // Direct PID kill
            forceQuitAll()
        }
    }

    func restartDaemon() {
        save()
        lastError = ""

        // Stop first
        let pidPath = dataDir.appendingPathComponent("daemon.pid")
        if let binary = findOpenFlowBinary() {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                let stop = Process()
                stop.executableURL = URL(fileURLWithPath: binary)
                stop.arguments = ["stop"]
                try? stop.run()
                stop.waitUntilExit()

                Thread.sleep(forTimeInterval: 2.0)

                // Then start (on main thread to use startDaemon logic)
                DispatchQueue.main.async {
                    self?.startDaemon()
                }
            }
        } else {
            // Kill by PID then start
            if let pidStr = try? String(contentsOf: pidPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines),
               let pid = Int32(pidStr) {
                kill(pid, SIGTERM)
            }
            try? FileManager.default.removeItem(at: pidPath)
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
                self?.startDaemon()
            }
        }
    }

    func forceQuitAll() {
        lastError = ""
        // Kill the daemon by PID (not pkill which would kill us too)
        let pidPath = dataDir.appendingPathComponent("daemon.pid")
        if let pidStr = try? String(contentsOf: pidPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines),
           let pid = Int32(pidStr) {
            kill(pid, SIGTERM)
            Thread.sleep(forTimeInterval: 1.0)
            if kill(pid, 0) == 0 {
                kill(pid, SIGKILL)  // force kill if still alive
            }
        }
        try? FileManager.default.removeItem(at: pidPath)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            self?.refreshStatus()
        }
    }

    /// Quit the Open Flow app (daemon). Sends SIGTERM to the daemon PID.
    func quitApp() {
        let pidPath = dataDir.appendingPathComponent("daemon.pid")
        if let pidStr = try? String(contentsOf: pidPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines),
           let pid = Int32(pidStr) {
            kill(pid, SIGTERM)
        }
        // Daemon's signal handler will trigger clean exit with process::exit(0)
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            self?.refreshStatus()
        }
    }

    private func searchedPaths() -> String {
        var paths: [String] = []
        if let selfPath = Bundle.main.executableURL?.deletingLastPathComponent() {
            paths.append(selfPath.appendingPathComponent("open-flow").path)
        }
        paths.append(contentsOf: ["/usr/local/bin/open-flow", "/opt/homebrew/bin/open-flow"])
        return paths.joined(separator: ", ")
    }

    // MARK: - Hotkey Test

    // MARK: - System Audio PoC

    func refreshSystemAudioDiagnostics() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            let permissionSnapshot = self.fetchSystemAudioPermissionSnapshot()
            let shareableSnapshot = self.fetchSystemAudioShareableSnapshot()

            DispatchQueue.main.async {
                if let permissionSnapshot {
                    self.systemAudioScreenRecordingGranted = permissionSnapshot.screenRecording
                    self.screenRecordingGranted = permissionSnapshot.screenRecording
                }

                if let shareableSnapshot {
                    self.systemAudioScreenRecordingGranted = shareableSnapshot.screenRecordingGranted
                    self.screenRecordingGranted = shareableSnapshot.screenRecordingGranted
                    self.systemAudioDisplays = shareableSnapshot.displays
                    self.systemAudioApplications = shareableSnapshot.applications
                    self.systemAudioWindowCount = shareableSnapshot.windowCount
                    if self.selectedSystemAudioApplicationPID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                       let firstApp = shareableSnapshot.applications.first {
                        self.selectedSystemAudioApplicationPID = String(firstApp.processID)
                    }
                    if self.systemAudioTargetPID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                       let firstApp = shareableSnapshot.applications.first {
                        self.systemAudioTargetPID = String(firstApp.processID)
                        self.systemAudioTargetName = firstApp.applicationName
                        self.systemAudioTargetBundleID = firstApp.bundleIdentifier
                    }
                    self.systemAudioStatus = self.usesEnglish
                        ? "Fetched \(shareableSnapshot.displays.count) displays and \(shareableSnapshot.applications.count) applications."
                        : "已获取 \(shareableSnapshot.displays.count) 个显示器和 \(shareableSnapshot.applications.count) 个应用。"
                } else {
                    self.screenRecordingGranted = self.systemAudioScreenRecordingGranted
                    self.systemAudioDisplays = []
                    self.systemAudioApplications = []
                    self.systemAudioWindowCount = 0
                    self.systemAudioStatus = self.usesEnglish
                        ? "System audio helper is available, but shareable content is not ready yet."
                        : "系统音频 helper 已可用，但可捕获内容暂时还未就绪。"
                }
            }
        }
    }

    func requestSystemAudioPermission() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let binary = self.findSystemAudioHelperBinary() else {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "System audio helper not found."
                        : "未找到系统音频 helper。"
                }
                return
            }

            let task = Process()
            task.executableURL = URL(fileURLWithPath: binary)
            task.arguments = ["request-permission"]

            let stdout = Pipe()
            task.standardOutput = stdout
            task.standardError = Pipe()

            do {
                try task.run()
                task.waitUntilExit()
            } catch {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "Failed to request screen recording permission: \(error.localizedDescription)"
                        : "请求屏幕录制权限失败：\(error.localizedDescription)"
                }
                return
            }

            DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                self.refreshSystemAudioDiagnostics()
            }
        }
    }

    private func fetchSystemAudioPermissionSnapshot() -> SystemAudioPermissionSnapshot? {
        guard let binary = findSystemAudioHelperBinary() else { return nil }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = ["permissions"]

        let stdout = Pipe()
        task.standardOutput = stdout
        task.standardError = Pipe()

        do {
            try task.run()
            task.waitUntilExit()
        } catch {
            return nil
        }

        guard task.terminationStatus == 0 else { return nil }
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return nil }
        return try? JSONDecoder().decode(SystemAudioPermissionSnapshot.self, from: data)
    }

    private func fetchSystemAudioShareableSnapshot() -> SystemAudioShareableSnapshot? {
        guard let binary = findSystemAudioHelperBinary() else { return nil }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = ["list-shareable"]

        let stdout = Pipe()
        task.standardOutput = stdout
        task.standardError = Pipe()

        do {
            try task.run()
            task.waitUntilExit()
        } catch {
            return nil
        }

        guard task.terminationStatus == 0 else { return nil }
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return nil }
        return try? JSONDecoder().decode(SystemAudioShareableSnapshot.self, from: data)
    }

    func runDesktopSystemAudioProbe() {
        runSystemAudioProbe(arguments: ["probe-desktop", "--seconds", "3"]) { [weak self] snapshot in
            guard let self = self else { return }
            self.systemAudioDesktopProbeSummary = self.describeProbeSnapshot(snapshot)
        }
    }

    func runApplicationSystemAudioProbe() {
        let trimmed = selectedSystemAudioApplicationPID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let pid = Int32(trimmed) else {
            systemAudioApplicationProbeSummary = usesEnglish
                ? "Pick an application first."
                : "请先选择一个应用。"
            return
        }

        if let app = systemAudioApplications.first(where: { String($0.processID) == trimmed }) {
            systemAudioTargetPID = trimmed
            systemAudioTargetName = app.applicationName
            systemAudioTargetBundleID = app.bundleIdentifier
        }

        runSystemAudioProbe(arguments: ["probe-application", "--pid", String(pid), "--seconds", "3"]) { [weak self] snapshot in
            guard let self = self else { return }
            self.systemAudioApplicationProbeSummary = self.describeProbeSnapshot(snapshot)
        }
    }

    private func runSystemAudioProbe(arguments: [String], onSuccess: @escaping (SystemAudioProbeSnapshot) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let binary = self.findSystemAudioHelperBinary() else {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "System audio helper not found."
                        : "未找到系统音频 helper。"
                }
                return
            }

            DispatchQueue.main.async {
                self.systemAudioProbeRunning = true
            }

            defer {
                DispatchQueue.main.async {
                    self.systemAudioProbeRunning = false
                }
            }

            let task = Process()
            task.executableURL = URL(fileURLWithPath: binary)
            task.arguments = arguments

            let stdout = Pipe()
            task.standardOutput = stdout
            task.standardError = Pipe()

            do {
                try task.run()
                task.waitUntilExit()
            } catch {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "Failed to run system audio probe: \(error.localizedDescription)"
                        : "运行系统音频探测失败：\(error.localizedDescription)"
                }
                return
            }

            guard task.terminationStatus == 0 else {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "System audio probe exited with code \(task.terminationStatus)."
                        : "系统音频探测退出码为 \(task.terminationStatus)。"
                }
                return
            }

            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            guard !data.isEmpty,
                  let snapshot = try? JSONDecoder().decode(SystemAudioProbeSnapshot.self, from: data) else {
                DispatchQueue.main.async {
                    self.systemAudioStatus = self.usesEnglish
                        ? "System audio probe returned unreadable output."
                        : "系统音频探测返回了无法解析的输出。"
                }
                return
            }

            DispatchQueue.main.async {
                onSuccess(snapshot)
            }
        }
    }

    private func describeProbeSnapshot(_ snapshot: SystemAudioProbeSnapshot) -> String {
        let callbackPart = usesEnglish
            ? "\(snapshot.callbacks) callbacks"
            : "\(snapshot.callbacks) 次回调"
        let samplePart = usesEnglish
            ? "\(snapshot.sampleCount) samples"
            : "\(snapshot.sampleCount) 个样本"
        let formatPart = snapshot.sampleRate > 0 && snapshot.channels > 0
            ? String(format: usesEnglish ? "%.0f Hz / %d ch" : "%.0f Hz / %d 通道", snapshot.sampleRate, snapshot.channels)
            : (usesEnglish ? "format pending" : "格式待定")
        let latencyPart: String
        if let latency = snapshot.firstCallbackLatencyMs {
            latencyPart = String(format: usesEnglish ? "first callback %.0f ms" : "首帧回调 %.0f ms", latency)
        } else {
            latencyPart = usesEnglish ? "no callback yet" : "尚未收到回调"
        }

        if let error = snapshot.error, !error.isEmpty {
            return usesEnglish
                ? "\(snapshot.target) • \(error)"
                : "\(snapshot.target) • \(error)"
        }

        return "\(snapshot.target) • \(callbackPart) • \(samplePart) • \(formatPart) • \(latencyPart)"
    }

    func startHotkeyTest() {
        hotkeyTestActive = true
        hotkeyTestLog = usesEnglish
            ? "Listening for hotkey events...\nPress your configured hotkey now.\n\n"
            : "正在监听热键事件...\n现在请按下你配置的热键。\n\n"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            // Use CGEventTap to listen for Fn key — same as the daemon does
            typealias Callback = @convention(c) (
                _ proxy: UnsafeMutableRawPointer?,
                _ type: UInt32,
                _ event: UnsafeMutableRawPointer?,
                _ userInfo: UnsafeMutableRawPointer?
            ) -> UnsafeMutableRawPointer?

            // We'll monitor flag changes via IOHIDManager or simply poll NSEvent
            // Simpler approach: use NSEvent.addGlobalMonitorForEvents
            DispatchQueue.main.async {
                self.startNSEventMonitor()
            }
        }
    }

    // MARK: - Meeting Sessions

    func refreshMeetingSessionsOverview() {
        let rootURL = meetingSessionsDirectoryURL

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            let fileManager = FileManager.default

            guard fileManager.fileExists(atPath: rootURL.path) else {
                DispatchQueue.main.async {
                    self.meetingSessionCount = 0
                    self.latestMeetingSessionName = ""
                    self.latestMeetingSessionUpdatedAt = ""
                    self.latestMeetingSessionHasTranscript = false
                    self.latestMeetingSessionStatus = self.usesEnglish
                        ? "No meeting sessions have been saved on this Mac yet."
                        : "当前这台 Mac 上还没有保存过会议记录。"
                }
                return
            }

            let keys: [URLResourceKey] = [.isDirectoryKey, .contentModificationDateKey]
            let children = (try? fileManager.contentsOfDirectory(
                at: rootURL,
                includingPropertiesForKeys: keys,
                options: [.skipsHiddenFiles]
            )) ?? []

            let directories = children.compactMap { url -> (url: URL, date: Date)? in
                guard let values = try? url.resourceValues(forKeys: Set(keys)),
                      values.isDirectory == true else {
                    return nil
                }
                return (url, values.contentModificationDate ?? .distantPast)
            }
            .sorted { $0.date > $1.date }

            let latest = directories.first
            let formatter = DateFormatter()
            formatter.locale = Locale(identifier: self.usesEnglish ? "en_US_POSIX" : "zh_CN")
            formatter.dateStyle = .medium
            formatter.timeStyle = .short

            DispatchQueue.main.async {
                self.meetingSessionCount = directories.count

                guard let latest else {
                    self.latestMeetingSessionName = ""
                    self.latestMeetingSessionUpdatedAt = ""
                    self.latestMeetingSessionHasTranscript = false
                    self.latestMeetingSessionStatus = self.usesEnglish
                        ? "No meeting sessions have been saved on this Mac yet."
                        : "当前这台 Mac 上还没有保存过会议记录。"
                    return
                }

                let transcriptURL = latest.url.appendingPathComponent("merged_transcript.md")
                let hasTranscript = fileManager.fileExists(atPath: transcriptURL.path)

                self.latestMeetingSessionName = latest.url.lastPathComponent
                self.latestMeetingSessionUpdatedAt = formatter.string(from: latest.date)
                self.latestMeetingSessionHasTranscript = hasTranscript
                self.latestMeetingSessionStatus = hasTranscript
                    ? (self.usesEnglish
                        ? "Latest session is ready for review."
                        : "最近一次会议记录已经可查看。")
                    : (self.usesEnglish
                        ? "Latest session exists, but the merged transcript is not ready yet."
                        : "最近一次会议目录已生成，但合并稿暂时还未写完。")
            }
        }
    }

    func openMeetingSessionsFolder() {
        let fileManager = FileManager.default
        let rootURL = meetingSessionsDirectoryURL

        if fileManager.fileExists(atPath: rootURL.path) {
            NSWorkspace.shared.open(rootURL)
        } else {
            NSWorkspace.shared.open(dataDir)
        }
    }

    func openLatestMeetingSession() {
        guard let latestURL = latestMeetingSessionURL else {
            openMeetingSessionsFolder()
            return
        }
        NSWorkspace.shared.activateFileViewerSelecting([latestURL])
    }

    func stopHotkeyTest() {
        hotkeyTestActive = false
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
        if let m = localEventMonitor { NSEvent.removeMonitor(m); localEventMonitor = nil }
        hotkeyTestLog += usesEnglish ? "\nTest stopped.\n" : "\n测试已停止。\n"
    }

    private var eventMonitor: Any?
    private var localEventMonitor: Any?

    private func startNSEventMonitor() {
        let mask: NSEvent.EventTypeMask = [.flagsChanged, .keyDown, .keyUp]

        // Monitor modifier changes globally and function-key events when relevant.
        eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: mask) { [weak self] event in
            guard let self = self, self.hotkeyTestActive else { return }

            let timestamp = String(format: "%.1f", event.timestamp)
            if event.type == .flagsChanged {
                let flags = event.modifierFlags.rawValue
                let fnDown = (flags & 0x800000) != 0
                let cmdDown = event.modifierFlags.contains(.command)
                let optDown = event.modifierFlags.contains(.option)
                let ctrlDown = event.modifierFlags.contains(.control)
                let shiftDown = event.modifierFlags.contains(.shift)
                let line = "[\(timestamp)] type=flagsChanged keycode=\(event.keyCode) fn=\(fnDown ? "DOWN" : "up  ") cmd=\(cmdDown ? "DOWN" : "up  ") opt=\(optDown ? "DOWN" : "up  ") ctrl=\(ctrlDown ? "DOWN" : "up  ") shift=\(shiftDown ? "DOWN" : "up  ")\n"
                DispatchQueue.main.async {
                    self.appendHotkeyTestLine(line)
                }
            } else if self.hotkey == "f13", event.keyCode == 105 {
                let line = "[\(timestamp)] type=\(event.type == .keyDown ? "keyDown" : "keyUp  ") key=f13 keycode=\(event.keyCode)\n"
                DispatchQueue.main.async {
                    self.appendHotkeyTestLine(line)
                }
            }
        }

        // Also monitor local events (when this app is focused)
        localEventMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
            guard let self = self, self.hotkeyTestActive else { return event }

            let timestamp = String(format: "%.1f", event.timestamp)
            if event.type == .flagsChanged {
                let flags = event.modifierFlags.rawValue
                let fnDown = (flags & 0x800000) != 0
                let cmdDown = event.modifierFlags.contains(.command)
                let optDown = event.modifierFlags.contains(.option)
                let ctrlDown = event.modifierFlags.contains(.control)
                let shiftDown = event.modifierFlags.contains(.shift)
                let line = "[\(timestamp)] type=flagsChanged keycode=\(event.keyCode) fn=\(fnDown ? "DOWN ✅" : "up    ") cmd=\(cmdDown ? "DOWN" : "up  ") opt=\(optDown ? "DOWN" : "up  ") ctrl=\(ctrlDown ? "DOWN" : "up  ") shift=\(shiftDown ? "DOWN" : "up  ")\n"
                DispatchQueue.main.async {
                    self.appendHotkeyTestLine(line)
                }
            } else if self.hotkey == "f13", event.keyCode == 105 {
                let line = "[\(timestamp)] type=\(event.type == .keyDown ? "keyDown" : "keyUp  ") key=f13 keycode=\(event.keyCode)\n"
                DispatchQueue.main.async {
                    self.appendHotkeyTestLine(line)
                }
            }
            return event
        }
    }

    private func appendHotkeyTestLine(_ line: String) {
        hotkeyTestLog += line
        let lines = hotkeyTestLog.components(separatedBy: "\n")
        if lines.count > 50 {
            hotkeyTestLog = lines.suffix(40).joined(separator: "\n")
        }
    }

    // MARK: - Model Management

    var normalizedModelPreset: String {
        modelPreset == "fp16" ? "fp16" : "quantized"
    }


    var selectedLocalModelLabel: String {
        if usesEnglish {
            return normalizedModelPreset == "fp16" ? "FP16 (higher accuracy)" : "Quantized (default, smaller)"
        }
        return normalizedModelPreset == "fp16" ? "FP16（精度更高）" : "量化版（默认，更轻量）"
    }

    var selectedLocalModelDownloadSummary: String {
        if usesEnglish {
            return normalizedModelPreset == "fp16"
                ? "Downloads SenseVoice FP16 (~450 MB) from Hugging Face"
                : "Downloads SenseVoice quantized (~230 MB) from Hugging Face"
        }
        return normalizedModelPreset == "fp16"
            ? "从 Hugging Face 下载 SenseVoice FP16（约 450 MB）"
            : "从 Hugging Face 下载 SenseVoice 量化版（约 230 MB）"
    }

    func defaultModelPath(for preset: String) -> String {
        let subdir = preset == "fp16" ? "sensevoice-small-fp16" : "sensevoice-small"
        return dataDir.appendingPathComponent("models/\(subdir)").path
    }

    func selectLocalModelPreset(_ preset: String) {
        let normalized = preset == "fp16" ? "fp16" : "quantized"
        modelPreset = normalized
        modelPath = defaultModelPath(for: normalized)
        save()
        checkModelReady()
    }

    func ensureSelectedLocalModelReady(autoDownload: Bool = true) {
        guard provider == "local" else { return }
        selectLocalModelPreset(modelPreset)
        if autoDownload && !modelReady && !modelDownloading {
            downloadModel()
        }
    }

    func checkModelReady() {
        if modelPath.isEmpty {
            modelPath = defaultModelPath(for: normalizedModelPreset)
        }

        let path = resolvedModelPath
        if path.isEmpty {
            modelReady = false
            return
        }
        let dir = URL(fileURLWithPath: path)
        let onnxExists = FileManager.default.fileExists(atPath: dir.appendingPathComponent("model_quant.onnx").path)
            || FileManager.default.fileExists(atPath: dir.appendingPathComponent("model.onnx").path)
        let tokensExist = FileManager.default.fileExists(atPath: dir.appendingPathComponent("tokens.json").path)
        modelReady = onnxExists && tokensExist
    }

    var resolvedModelPath: String {
        if !modelPath.isEmpty { return modelPath }
        return defaultModelPath(for: normalizedModelPreset)
    }

    func downloadModel() {
        let preset = normalizedModelPreset
        modelDownloading = true
        modelDownloadProgress = 0
        modelDownloadStatus = usesEnglish ? "Preparing \(preset) model download..." : "正在准备 \(preset) 模型下载..."
        modelDownloadOutput = usesEnglish ? "Starting model download...\n" : "开始下载模型...\n"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let binary = self.findOpenFlowBinary() else {
                DispatchQueue.main.async {
                    self.modelDownloadOutput += self.usesEnglish
                        ? "Error: open-flow binary not found. Please install open-flow first.\n"
                        : "错误：未找到 open-flow 可执行文件，请先安装 open-flow。\n"
                    self.modelDownloadStatus = self.usesEnglish ? "Download unavailable" : "无法下载"
                    self.modelDownloading = false
                }
                return
            }
            let task = Process()
            task.executableURL = URL(fileURLWithPath: binary)
            task.arguments = ["model", "use", preset, "--download"]

            let pipe = Pipe()
            task.standardOutput = pipe
            task.standardError = pipe

            var outputBuffer = ""

            pipe.fileHandleForReading.readabilityHandler = { handle in
                let data = handle.availableData
                if let str = String(data: data, encoding: .utf8), !str.isEmpty {
                    DispatchQueue.main.async {
                        self.modelDownloadOutput += str
                    }
                    outputBuffer += str
                    self.updateModelDownloadProgress(from: outputBuffer)
                }
            }

            do {
                try task.run()
                task.waitUntilExit()
            } catch {
                DispatchQueue.main.async {
                    self.modelDownloadStatus = self.usesEnglish ? "Download failed" : "下载失败"
                    self.modelDownloadOutput += self.usesEnglish
                        ? "\nError: \(error.localizedDescription)\n"
                        : "\n错误：\(error.localizedDescription)\n"
                }
            }

            pipe.fileHandleForReading.readabilityHandler = nil

            DispatchQueue.main.async {
                self.modelDownloading = false
                self.modelDownloadProgress = task.terminationStatus == 0 ? 1.0 : self.modelDownloadProgress
                self.checkModelReady()
                if task.terminationStatus == 0 {
                    self.modelDownloadStatus = self.usesEnglish
                        ? "\(preset.uppercased()) model ready"
                        : "\(preset.uppercased()) 模型已就绪"
                    self.modelDownloadOutput += self.usesEnglish ? "\nModel download complete!\n" : "\n模型下载完成！\n"
                } else {
                    self.modelDownloadStatus = self.usesEnglish ? "Download failed" : "下载失败"
                    self.modelDownloadOutput += self.usesEnglish
                        ? "\nDownload failed (exit code \(task.terminationStatus))\n"
                        : "\n下载失败（退出码 \(task.terminationStatus)）\n"
                }
            }
        }
    }

    private func updateModelDownloadProgress(from output: String) {
        let pattern = #"([0-9]+(?:\.[0-9]+)?) MB / ([0-9]+(?:\.[0-9]+)?) MB\s+\(([0-9]+)%\)"#
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return }
        let range = NSRange(output.startIndex..., in: output)
        guard let match = regex.matches(in: output, range: range).last else { return }

        let current = nsSubstring(output, range: match.range(at: 1))
        let total = nsSubstring(output, range: match.range(at: 2))
        let percent = nsSubstring(output, range: match.range(at: 3))

        guard let currentMB = Double(current),
              let totalMB = Double(total),
              let percentValue = Double(percent) else {
            return
        }

        DispatchQueue.main.async {
            self.modelDownloadProgress = min(max(percentValue / 100.0, 0), 1)
            self.modelDownloadStatus = self.usesEnglish
                ? String(format: "Downloading %.1f / %.1f MB (%.0f%%)", currentMB, totalMB, percentValue)
                : String(format: "正在下载 %.1f / %.1f MB (%.0f%%)", currentMB, totalMB, percentValue)
        }
    }

    private func nsSubstring(_ source: String, range: NSRange) -> String {
        guard let swiftRange = Range(range, in: source) else { return "" }
        return String(source[swiftRange])
    }

    // MARK: - Logs

    func loadLogs() {
        let logPath = dataDir.appendingPathComponent("daemon.log")

        // Read on background thread to avoid freezing UI
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            // Use `tail` command to read only last 100 lines — avoids loading huge files
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/usr/bin/tail")
            task.arguments = ["-n", "100", logPath.path]
            let pipe = Pipe()
            task.standardOutput = pipe
            task.standardError = Pipe()

            var result = self.usesEnglish ? "(No log file found)" : "（未找到日志文件）"
            if let _ = try? task.run() {
                task.waitUntilExit()
                if let output = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8),
                   !output.isEmpty {
                    result = output
                }
            }

            DispatchQueue.main.async {
                self.logContent = result
            }
        }
    }

    var configFileURL: URL { configPath }
    var logFileURL: URL { dataDir.appendingPathComponent("daemon.log") }
    var performanceLogDirectoryURL: URL { dataDir.appendingPathComponent("performance") }
    var meetingSessionsDirectoryURL: URL { dataDir.appendingPathComponent("meeting-sessions") }

    private var latestMeetingSessionURL: URL? {
        let trimmed = latestMeetingSessionName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return meetingSessionsDirectoryURL.appendingPathComponent(trimmed)
    }
}
