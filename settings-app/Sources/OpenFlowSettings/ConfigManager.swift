import Foundation
import Combine
import AppKit

private struct PermissionSnapshot: Decodable {
    let accessibility: Bool
    let inputMonitoring: Bool
    let microphone: Bool

    enum CodingKeys: String, CodingKey {
        case accessibility
        case inputMonitoring = "input_monitoring"
        case microphone
    }
}

/// Manages reading/writing Open Flow's config.toml and daemon lifecycle
class ConfigManager: ObservableObject {
    // Config fields
    @Published var provider: String = "local"
    @Published var correctionEnabled: String = ""
    @Published var correctionModel: String = "GLM-4-Flash-250414"
    @Published var correctionApiKey: String = ""
    @Published var modelPreset: String = "quantized"
    @Published var groqApiKey: String = ""
    @Published var groqModel: String = "whisper-large-v3-turbo"
    @Published var groqLanguage: String = ""
    @Published var hotkey: String = "right_cmd"
    @Published var triggerMode: String = "toggle"
    @Published var chineseConversion: String = ""
    @Published var modelPath: String = ""

    // Daemon status
    @Published var daemonRunning = false
    @Published var daemonPID: String = ""
    @Published var daemonUptime: String = ""
    @Published var lastError: String = ""

    // Permissions
    @Published var accessibilityGranted = false
    @Published var inputMonitoringGranted = false
    @Published var microphoneGranted = false

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

    static let groqModels = ["whisper-large-v3-turbo", "whisper-large-v3"]
    static let localModelPresets = ["quantized", "fp16"]
    static let localModelPresetLabels = [
        "quantized": "Quantized (default, smaller)",
        "fp16": "FP16 (higher accuracy)"
    ]
    static let hotkeys = ["right_cmd", "right_option", "right_control", "right_shift", "fn", "f13"]
    static let hotkeyLabels = [
        "Right Command (⌘)",
        "Right Option (⌥)",
        "Right Control (⌃)",
        "Right Shift (⇧)",
        "Fn",
        "F13"
    ]
    static let triggerModes = ["toggle", "hold"]
    static let triggerLabels = ["Toggle (press start, press stop)", "Hold (hold to record)"]

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
        refreshStatus()
        refreshPermissions()

        statusTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            guard let self = self else { return }
            let wasRunningBefore = self.daemonRunning
            self.refreshStatus()
            self.refreshPermissions()

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
            let mic = snapshot?.microphone ?? self.checkMicrophone()

            DispatchQueue.main.async {
                self.accessibilityGranted = ax
                self.inputMonitoringGranted = im
                self.microphoneGranted = mic
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

    private func checkAccessibility() -> Bool {
        // AXIsProcessTrusted checks if THIS process (same bundle) has accessibility
        typealias AXFunc = @convention(c) () -> Bool
        guard let handle = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_LAZY),
              let sym = dlsym(handle, "AXIsProcessTrusted") else {
            return false
        }
        let fn = unsafeBitCast(sym, to: AXFunc.self)
        return fn()
    }

    private func checkInputMonitoring() -> Bool {
        guard let handle = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_LAZY),
              let sym = dlsym(handle, "CGPreflightListenEventAccess") else {
            return false
        }
        typealias Func = @convention(c) () -> Bool
        let fn = unsafeBitCast(sym, to: Func.self)
        return fn()
    }

    private func checkMicrophone() -> Bool {
        // AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        // 0=NotDetermined, 1=Restricted, 2=Denied, 3=Authorized
        typealias AuthFunc = @convention(c) (AnyObject, Selector, AnyObject) -> Int
        guard let cls = NSClassFromString("AVCaptureDevice"),
              let _ = NSClassFromString("NSString") else {
            return false
        }
        let sel = NSSelectorFromString("authorizationStatusForMediaType:")
        let audioStr = "soun" as NSString  // AVMediaTypeAudio
        let status = (cls as AnyObject).perform(sel, with: audioStr)
        // perform returns Unmanaged<AnyObject>? — the actual return is an Int disguised as pointer
        let rawValue = Int(bitPattern: status?.toOpaque())
        return rawValue == 3
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
            self.lastError = "Failed to open System Settings automatically. Please open Privacy & Security manually."
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
            case "provider": provider = value
            case "correction_enabled": correctionEnabled = value
            case "correction_model": correctionModel = value.isEmpty ? "GLM-4-Flash-250414" : value
            case "correction_api_key": correctionApiKey = value
            case "model_preset": modelPreset = value.isEmpty ? "quantized" : value
            case "groq_api_key": groqApiKey = value
            case "groq_model": groqModel = value
            case "groq_language": groqLanguage = value
            case "hotkey": hotkey = value
            case "trigger_mode": triggerMode = value
            case "chinese_conversion": chineseConversion = value
            case "model_path": modelPath = value
            default: break
            }
        }

        checkModelReady()
        loadPersonalVocabulary()
    }

    func save() {
        var existingLines: [String] = []
        var knownKeys = Set<String>()

        if let content = try? String(contentsOf: configPath, encoding: .utf8) {
            existingLines = content.components(separatedBy: "\n")
        }

        let ourValues: [(String, String)] = [
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
            ("chinese_conversion", chineseConversion),
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
                self.lastError = "Failed to save config: \(error.localizedDescription)"
            }
        }
    }

    var normalizedCorrectionModel: String {
        let trimmed = correctionModel.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "glm-4.7-flash" : trimmed
    }

    var correctionIsEnabled: Bool {
        let value = correctionEnabled.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return value == "true" || value == "1" || value == "yes" || value == "on" || value == "enabled"
    }

    func setCorrectionEnabled(_ enabled: Bool) {
        correctionEnabled = enabled ? "true" : "false"
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
                self.lastError = "Failed to save personal vocabulary: \(error.localizedDescription)"
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
                        self?.lastError = "Failed to launch app: \(error.localizedDescription)"
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
            lastError = "Cannot find open-flow binary or .app bundle. Searched: \(searchedPaths())"
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
                        self?.lastError = "Start failed (exit \(task.terminationStatus)): \(output)"
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    self?.lastError = "Start error: \(error.localizedDescription)"
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

    func startHotkeyTest() {
        hotkeyTestActive = true
        hotkeyTestLog = "Listening for hotkey events...\nPress your configured hotkey now.\n\n"

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

    func stopHotkeyTest() {
        hotkeyTestActive = false
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
        if let m = localEventMonitor { NSEvent.removeMonitor(m); localEventMonitor = nil }
        hotkeyTestLog += "\nTest stopped.\n"
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
        Self.localModelPresetLabels[normalizedModelPreset] ?? normalizedModelPreset
    }

    var selectedLocalModelDownloadSummary: String {
        normalizedModelPreset == "fp16"
            ? "Downloads SenseVoice FP16 (~450 MB) from Hugging Face"
            : "Downloads SenseVoice quantized (~230 MB) from Hugging Face"
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
        modelDownloadStatus = "Preparing \(preset) model download..."
        modelDownloadOutput = "Starting model download...\n"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let binary = self.findOpenFlowBinary() else {
                DispatchQueue.main.async {
                    self.modelDownloadOutput += "Error: open-flow binary not found. Please install open-flow first.\n"
                    self.modelDownloadStatus = "Download unavailable"
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
                    self.modelDownloadStatus = "Download failed"
                    self.modelDownloadOutput += "\nError: \(error.localizedDescription)\n"
                }
            }

            pipe.fileHandleForReading.readabilityHandler = nil

            DispatchQueue.main.async {
                self.modelDownloading = false
                self.modelDownloadProgress = task.terminationStatus == 0 ? 1.0 : self.modelDownloadProgress
                self.checkModelReady()
                if task.terminationStatus == 0 {
                    self.modelDownloadStatus = "\(preset.uppercased()) model ready"
                    self.modelDownloadOutput += "\nModel download complete!\n"
                } else {
                    self.modelDownloadStatus = "Download failed"
                    self.modelDownloadOutput += "\nDownload failed (exit code \(task.terminationStatus))\n"
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
            self.modelDownloadStatus = String(format: "Downloading %.1f / %.1f MB (%.0f%%)", currentMB, totalMB, percentValue)
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

            var result = "(No log file found)"
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
}
