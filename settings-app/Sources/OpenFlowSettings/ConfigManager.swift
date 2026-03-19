import Foundation
import Combine
import AppKit

/// Manages reading/writing Open Flow's config.toml and daemon lifecycle
class ConfigManager: ObservableObject {
    // Config fields
    @Published var provider: String = "local"
    @Published var ttsProvider: String = "system"
    @Published var ttsModel: String = "microsoft/VibeVoice-Realtime-0.5B"
    @Published var ttsVoicePath: String = ""
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

    @Published var ttsDepsChecking = false
    @Published var ttsDepsLastResult = "Not checked"
    @Published var ttsHasSay = false
    @Published var ttsHasFfmpeg = false
    @Published var ttsHasPython = false
    @Published var ttsHasBundledScript = false
    @Published var ttsHasVibevoiceRuntime = false
    @Published var ttsHasVoiceFile = false
    @Published var ttsInstallInProgress = false
    @Published var ttsInstallProgress: Double = 0
    @Published var ttsInstallStatus = ""
    @Published var ttsInstallOutput = ""

    // Log
    @Published var logContent: String = ""

    static let groqModels = ["whisper-large-v3-turbo", "whisper-large-v3"]
    static let ttsProviders = ["system", "local_model"]
    static let ttsProviderLabels = [
        "system": "System (macOS)",
        "local_model": "Local Model (Python + VibeVoice)"
    ]
    static let localModelPresets = ["quantized", "fp16"]
    static let localModelPresetLabels = [
        "quantized": "Quantized (default, smaller)",
        "fp16": "FP16 (higher accuracy)"
    ]
    static let hotkeys = ["right_cmd", "fn", "f13"]
    static let hotkeyLabels = ["Right Command (⌘)", "Fn", "F13"]
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
        refreshTtsDependencies()

        // Poll daemon status and permissions every 3 seconds
        statusTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            guard let self = self else { return }
            let wasRunningBefore = self.daemonRunning
            self.refreshStatus()
            self.refreshPermissions()

            // If daemon just died (was running, now isn't), quit settings app
            // The daemon kills us on exit anyway, but this handles edge cases
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
        let ax = checkAccessibility()
        let im = checkInputMonitoring()
        let mic = checkMicrophone()
        DispatchQueue.main.async {
            self.accessibilityGranted = ax
            self.inputMonitoringGranted = im
            self.microphoneGranted = mic
        }
        #endif
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

    func openAccessibilitySettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }

    func openMicrophoneSettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!)
    }

    func openInputMonitoringSettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")!)
    }

    func copyModelPathToClipboard() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(resolvedModelPath, forType: .string)
    }

    // MARK: - Config I/O

    func load() {
        guard let content = try? String(contentsOf: configPath, encoding: .utf8) else { return }

        for line in content.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard let (key, value) = parseTOMLLine(trimmed) else { continue }

            switch key {
            case "provider": provider = value
            case "tts_provider": ttsProvider = value
            case "tts_model": ttsModel = value.isEmpty ? "microsoft/VibeVoice-Realtime-0.5B" : value
            case "tts_voice_path": ttsVoicePath = value
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
    }

    func save() {
        var existingLines: [String] = []
        var knownKeys = Set<String>()

        if let content = try? String(contentsOf: configPath, encoding: .utf8) {
            existingLines = content.components(separatedBy: "\n")
        }

        let ourValues: [(String, String)] = [
            ("provider", provider),
            ("tts_provider", normalizedTtsProvider),
            ("tts_model", normalizedTtsModel),
            ("tts_voice_path", ttsVoicePath),
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
        hotkeyTestLog = "Listening for Fn key events...\nPress Fn key now.\n\n"

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
        // Monitor flag changes (modifier keys including Fn)
        eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            guard let self = self, self.hotkeyTestActive else { return }

            let flags = event.modifierFlags.rawValue
            let fnDown = (flags & 0x800000) != 0
            let timestamp = String(format: "%.1f", event.timestamp)

            let line = "[\(timestamp)] flags=0x\(String(flags, radix: 16)) fn=\(fnDown ? "DOWN" : "up  ") keycode=\(event.keyCode)\n"
            DispatchQueue.main.async {
                self.hotkeyTestLog += line
                // Keep log manageable
                let lines = self.hotkeyTestLog.components(separatedBy: "\n")
                if lines.count > 50 {
                    self.hotkeyTestLog = lines.suffix(40).joined(separator: "\n")
                }
            }
        }

        // Also monitor local events (when this app is focused)
        localEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            guard let self = self, self.hotkeyTestActive else { return event }

            let flags = event.modifierFlags.rawValue
            let fnDown = (flags & 0x800000) != 0
            let timestamp = String(format: "%.1f", event.timestamp)

            let line = "[\(timestamp)] flags=0x\(String(flags, radix: 16)) fn=\(fnDown ? "DOWN ✅" : "up    ") keycode=\(event.keyCode)\n"
            DispatchQueue.main.async {
                self.hotkeyTestLog += line
                let lines = self.hotkeyTestLog.components(separatedBy: "\n")
                if lines.count > 50 {
                    self.hotkeyTestLog = lines.suffix(40).joined(separator: "\n")
                }
            }
            return event
        }
    }

    // MARK: - Model Management

    var normalizedModelPreset: String {
        modelPreset == "fp16" ? "fp16" : "quantized"
    }

    var ttsDepsReady: Bool {
        if normalizedTtsProvider == "local_model" {
            return ttsHasPython && ttsHasFfmpeg && ttsHasBundledScript && ttsHasVibevoiceRuntime && ttsHasVoiceFile
        }
        return ttsHasSay && ttsHasFfmpeg
    }

    var ttsCanAutoInstall: Bool {
        normalizedTtsProvider == "local_model"
    }

    var normalizedTtsProvider: String {
        ttsProvider == "local_model" ? "local_model" : "system"
    }

    var normalizedTtsModel: String {
        let v = ttsModel.trimmingCharacters(in: .whitespacesAndNewlines)
        return v.isEmpty ? "microsoft/VibeVoice-Realtime-0.5B" : v
    }

    var ttsModelURL: String {
        if normalizedTtsModel.contains("/") {
            return "https://huggingface.co/\(normalizedTtsModel)"
        }
        return normalizedTtsModel
    }

    func refreshTtsDependencies() {
        let provider = normalizedTtsProvider
        let voicePath = ttsVoicePath.trimmingCharacters(in: .whitespacesAndNewlines)

        DispatchQueue.main.async {
            self.ttsDepsChecking = true
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            let sayPath = self.findCommandPath("say", candidates: ["/usr/bin/say"])
            let ffmpegPath = self.findCommandPath("ffmpeg", candidates: ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"])
            let pythonCandidates = self.findPythonCandidates()
            let pythonPath = pythonCandidates.first
            let scriptPath = self.findBundledTtsScriptPath()

            var hasRuntime = false
            var runtimeDetail = ""
            var runtimePythonPath: String?
            if provider == "local_model", !pythonCandidates.isEmpty {
                let probe = self.checkAnyPythonVibevoiceRuntime(candidates: pythonCandidates)
                hasRuntime = probe.0
                runtimeDetail = probe.1
                runtimePythonPath = probe.2
            }

            let hasVoice: Bool
            if provider == "local_model" {
                if !voicePath.isEmpty {
                    hasVoice = FileManager.default.fileExists(atPath: voicePath)
                } else {
                    hasVoice = self.findAnyVoiceEmbeddingPt()
                }
            } else {
                hasVoice = true
            }

            let summary: String
            if provider == "local_model" {
                let ok = !pythonCandidates.isEmpty && ffmpegPath != nil && scriptPath != nil && hasRuntime && hasVoice
                summary = ok ? "Local model TTS is ready" : "Local model TTS has missing dependencies"
            } else {
                let ok = sayPath != nil && ffmpegPath != nil
                summary = ok ? "System TTS is ready" : "System TTS has missing dependencies"
            }

            var logLines: [String] = []
            logLines.append("[TTS][DependencyCheck] provider=\(provider) summary=\(summary)")
            logLines.append("[TTS][DependencyCheck] say=\(sayPath ?? "missing (failed command: command -v say)")")
            logLines.append("[TTS][DependencyCheck] ffmpeg=\(ffmpegPath ?? "missing (failed command: command -v ffmpeg)")")
            if provider == "local_model" {
                let candidatesLine = pythonCandidates.isEmpty ? "none" : pythonCandidates.joined(separator: ",")
                logLines.append("[TTS][DependencyCheck] python_candidates=\(candidatesLine)")
                logLines.append("[TTS][DependencyCheck] python3=\(runtimePythonPath ?? pythonPath ?? "missing (failed command: command -v python3)")")
                logLines.append("[TTS][DependencyCheck] bundled_script=\(scriptPath ?? "missing (failed command: test -f <app>/Contents/Resources/vibevoice_tts.py)")")
                if !runtimeDetail.isEmpty {
                    logLines.append("[TTS][DependencyCheck] python_runtime=\(runtimeDetail)")
                } else {
                    logLines.append("[TTS][DependencyCheck] python_runtime=\(hasRuntime ? "ok" : "failed")")
                }
                if !voicePath.isEmpty {
                    logLines.append("[TTS][DependencyCheck] voice_file=\(hasVoice ? "ok path=\(voicePath)" : "missing path=\(voicePath) (failed command: test -f \(voicePath))")")
                } else {
                    logLines.append("[TTS][DependencyCheck] voice_file=\(hasVoice ? "ok auto-detected" : "missing auto-detected")")
                }
            }

            self.appendDaemonLog(logLines)

            DispatchQueue.main.async {
                self.ttsHasSay = sayPath != nil
                self.ttsHasFfmpeg = ffmpegPath != nil
                self.ttsHasPython = !pythonCandidates.isEmpty
                self.ttsHasBundledScript = scriptPath != nil
                self.ttsHasVibevoiceRuntime = provider == "local_model" ? hasRuntime : true
                self.ttsHasVoiceFile = hasVoice
                self.ttsDepsLastResult = summary
                self.ttsDepsChecking = false
            }
        }
    }

    func installTtsDependencies() {
        guard normalizedTtsProvider == "local_model" else { return }
        guard !ttsInstallInProgress else { return }

        DispatchQueue.main.async {
            self.ttsInstallInProgress = true
            self.ttsInstallProgress = 0
            self.ttsInstallStatus = "Preparing dependency installation..."
            self.ttsInstallOutput = ""
        }

        appendDaemonLog(["[TTS][Installer] start"])

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            let totalSteps = 7.0
            var completed = 0.0
            let updateStep: (String) -> Void = { status in
                completed += 1.0
                let progress = min(max(completed / totalSteps, 0), 1)
                DispatchQueue.main.async {
                    self.ttsInstallStatus = status
                    self.ttsInstallProgress = progress
                }
            }

            do {
                var python = self.findPythonCandidates().first

                if python == nil {
                    guard let brew = self.findCommandPath("brew", candidates: ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]) else {
                        throw NSError(domain: "TTS", code: 1, userInfo: [NSLocalizedDescriptionKey: "python3 未找到，且 brew 不可用，无法自动安装"]) 
                    }
                    try self.runLoggedCommand(program: brew, args: ["install", "python"], label: "install_python")
                    python = self.findPythonCandidates().first
                }
                updateStep("Python ready")

                if self.findCommandPath("ffmpeg", candidates: ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"]) == nil {
                    guard let brew = self.findCommandPath("brew", candidates: ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]) else {
                        throw NSError(domain: "TTS", code: 2, userInfo: [NSLocalizedDescriptionKey: "ffmpeg 未找到，且 brew 不可用，无法自动安装"]) 
                    }
                    try self.runLoggedCommand(program: brew, args: ["install", "ffmpeg"], label: "install_ffmpeg")
                }
                updateStep("ffmpeg ready")

                guard let bootstrapPython = python else {
                    throw NSError(domain: "TTS", code: 3, userInfo: [NSLocalizedDescriptionKey: "无法定位可用 python3"]) 
                }

                let venvDir = self.dataDir.appendingPathComponent("tts-pyenv")
                let venvPython = venvDir.appendingPathComponent("bin/python3").path
                if !FileManager.default.fileExists(atPath: venvPython) {
                    try self.runLoggedCommand(program: bootstrapPython, args: ["-m", "venv", venvDir.path], label: "create_venv")
                } else {
                    self.appendDaemonLog(["[TTS][Installer] reuse venv path=\(venvDir.path)"])
                }
                updateStep("virtualenv ready")

                try self.runLoggedCommand(program: venvPython, args: ["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel"], label: "upgrade_pip")
                updateStep("pip toolchain ready")

                try self.runLoggedCommand(program: venvPython, args: ["-m", "pip", "install", "torch"], label: "install_torch")
                updateStep("torch installed")

                try self.runLoggedCommand(
                    program: venvPython,
                    args: [
                        "-m", "pip", "install",
                        "numpy",
                        "tqdm",
                        "requests",
                        "transformers==4.51.3",
                        "accelerate>=1.6.0",
                        "safetensors>=0.4.3",
                        "diffusers",
                        "soundfile"
                    ],
                    label: "install_runtime_deps"
                )
                updateStep("runtime deps installed")

                do {
                    try self.runLoggedCommand(
                        program: venvPython,
                        args: ["-m", "pip", "install", "--no-deps", "--force-reinstall", "git+https://github.com/microsoft/VibeVoice.git"],
                        label: "install_vibevoice_git_nodeps"
                    )
                } catch {
                    try self.runLoggedCommand(
                        program: venvPython,
                        args: ["-m", "pip", "install", "--no-deps", "--force-reinstall", "vibevoice"],
                        label: "install_vibevoice_pypi_nodeps"
                    )
                }
                updateStep("vibevoice installed")

                let verify = self.checkPythonVibevoiceRuntime(python: venvPython)
                if !verify.0 {
                    throw NSError(domain: "TTS", code: 4, userInfo: [NSLocalizedDescriptionKey: "依赖验证失败: \(verify.1)"])
                }

                self.appendDaemonLog(["[TTS][Installer] completed successfully python=\(venvPython)"])

                DispatchQueue.main.async {
                    self.ttsInstallStatus = "Dependencies installed"
                    self.ttsInstallProgress = 1
                    self.ttsInstallInProgress = false
                    self.refreshTtsDependencies()
                }
            } catch {
                let msg = error.localizedDescription
                self.appendDaemonLog(["[TTS][Installer] failed error=\(msg)"])
                DispatchQueue.main.async {
                    self.ttsInstallStatus = "Install failed"
                    self.ttsInstallInProgress = false
                    self.ttsInstallOutput += "\nInstall failed: \(msg)\n"
                    self.refreshTtsDependencies()
                }
            }
        }
    }

    private func runLoggedCommand(program: String, args: [String], label: String) throws {
        let cmd = ([program] + args).joined(separator: " ")
        appendDaemonLog(["[TTS][Installer] step=\(label) run=\(cmd)"])
        DispatchQueue.main.async {
            self.ttsInstallStatus = "Running \(label)..."
            self.ttsInstallOutput += "\n$ \(cmd)\n"
        }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: program)
        task.arguments = args
        let outPipe = Pipe()
        let errPipe = Pipe()
        task.standardOutput = outPipe
        task.standardError = errPipe

        try task.run()
        task.waitUntilExit()

        let out = String(data: outPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let merged = (out + "\n" + err).trimmingCharacters(in: .whitespacesAndNewlines)
        if !merged.isEmpty {
            let clipped = String(merged.prefix(1200))
            appendDaemonLog(["[TTS][Installer] step=\(label) output=\(clipped)"])
            DispatchQueue.main.async {
                self.ttsInstallOutput += clipped + "\n"
            }
        }

        if task.terminationStatus != 0 {
            throw NSError(domain: "TTS", code: Int(task.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "\(label) failed (exit \(task.terminationStatus))"])
        }
    }

    private func findCommandPath(_ name: String, candidates: [String]) -> String? {
        let cmdTask = Process()
        cmdTask.executableURL = URL(fileURLWithPath: "/bin/sh")
        cmdTask.arguments = ["-lc", "command -v \(name)"]
        let cmdPipe = Pipe()
        cmdTask.standardOutput = cmdPipe
        cmdTask.standardError = Pipe()
        do {
            try cmdTask.run()
            cmdTask.waitUntilExit()
            if cmdTask.terminationStatus == 0 {
                let out = String(data: cmdPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if let path = out, !path.isEmpty { return path }
            }
        } catch {
        }

        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/sh")
        task.arguments = ["-lc", "command -v \(name)"]
        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = Pipe()
        do {
            try task.run()
            task.waitUntilExit()
            guard task.terminationStatus == 0 else { return nil }
            let out = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return out?.isEmpty == false ? out : nil
        } catch {
            return nil
        }
    }

    private func findBundledTtsScriptPath() -> String? {
        guard let exe = Bundle.main.executableURL else { return nil }
        let resources = exe.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Resources")
        let script = resources.appendingPathComponent("vibevoice_tts.py")
        return FileManager.default.fileExists(atPath: script.path) ? script.path : nil
    }

    private func findPythonCandidates() -> [String] {
        var result: [String] = []

        let envOverride = ProcessInfo.processInfo.environment["OPEN_FLOW_TTS_PYTHON"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if let envOverride = envOverride,
           !envOverride.isEmpty,
           FileManager.default.isExecutableFile(atPath: envOverride) {
            result.append(envOverride)
        }

        let venvPython = dataDir.appendingPathComponent("tts-pyenv/bin/python3").path
        if FileManager.default.isExecutableFile(atPath: venvPython), !result.contains(venvPython) {
            result.append(venvPython)
        }

        if let detected = findCommandPath("python3", candidates: ["/usr/bin/python3"]), !detected.isEmpty {
            if !result.contains(detected) {
                result.append(detected)
            }
        }

        for path in ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"] {
            if FileManager.default.isExecutableFile(atPath: path), !result.contains(path) {
                result.append(path)
            }
        }

        return result
    }

    private func checkAnyPythonVibevoiceRuntime(candidates: [String]) -> (Bool, String, String?) {
        var details: [String] = []
        for python in candidates {
            let probe = checkPythonVibevoiceRuntime(python: python)
            if probe.0 {
                return (true, "ok", python)
            }
            details.append(probe.1)
        }
        return (false, details.joined(separator: " | "), nil)
    }

    private func checkPythonVibevoiceRuntime(python: String) -> (Bool, String) {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: python)
        task.arguments = ["-c", vibevoiceRuntimeProbeCode()]
        let outPipe = Pipe()
        let errPipe = Pipe()
        task.standardOutput = outPipe
        task.standardError = errPipe
        do {
            try task.run()
            task.waitUntilExit()
            if task.terminationStatus == 0 {
                return (true, "ok")
            }
            let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let msg = "failed command: \(python) -c <vibevoice runtime probe> stderr=\(err)"
            return (false, msg)
        } catch {
            return (false, "failed command: \(python) -c <vibevoice runtime probe> spawn_error=\(error.localizedDescription)")
        }
    }

    private func vibevoiceRuntimeProbeCode() -> String {
        [
            "import torch",
            "import soundfile",
            "from vibevoice.modular.modeling_vibevoice_streaming_inference import VibeVoiceStreamingForConditionalGenerationInference",
            "from vibevoice.processor.vibevoice_streaming_processor import VibeVoiceStreamingProcessor"
        ].joined(separator: "\n")
    }

    private func findAnyVoiceEmbeddingPt() -> Bool {
        let roots = [
            NSString(string: "~/Library/Application Support/com.openflow.open-flow/vibevoice").expandingTildeInPath,
            NSString(string: "~/Library/Application Support/com.openflow.open-flow").expandingTildeInPath,
            NSString(string: "~/.cache/huggingface/hub").expandingTildeInPath
        ]

        for root in roots {
            guard FileManager.default.fileExists(atPath: root) else { continue }
            guard let enumerator = FileManager.default.enumerator(atPath: root) else { continue }
            var scanned = 0
            let start = Date()
            while let item = enumerator.nextObject() as? String {
                scanned += 1
                if scanned > 15000 || Date().timeIntervalSince(start) > 2.0 {
                    break
                }
                if item.hasSuffix(".pt") && item.contains("voices/streaming_model") {
                    return true
                }
            }
        }
        return false
    }

    private func appendDaemonLog(_ lines: [String]) {
        guard !lines.isEmpty else { return }
        let logPath = dataDir.appendingPathComponent("daemon.log")
        let formatter = ISO8601DateFormatter()
        let ts = formatter.string(from: Date())
        let payload = lines.map { "[\(ts)] \($0)" }.joined(separator: "\n") + "\n"
        guard let data = payload.data(using: .utf8) else { return }

        if !FileManager.default.fileExists(atPath: logPath.path) {
            FileManager.default.createFile(atPath: logPath.path, contents: data)
            return
        }

        do {
            let fh = try FileHandle(forWritingTo: logPath)
            try fh.seekToEnd()
            try fh.write(contentsOf: data)
            try fh.close()
        } catch {
        }
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
