import Foundation
import Combine
import AppKit

/// Manages reading/writing Open Flow's config.toml and daemon lifecycle
class ConfigManager: ObservableObject {
    // Config fields
    @Published var provider: String = "local"
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
    @Published var modelDownloadOutput: String = ""

    // Log
    @Published var logContent: String = ""

    static let groqModels = ["whisper-large-v3-turbo", "whisper-large-v3"]
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

        // Poll daemon status and permissions every 3 seconds
        statusTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            self?.refreshStatus()
            self?.refreshPermissions()
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

    // MARK: - Config I/O

    func load() {
        guard let content = try? String(contentsOf: configPath, encoding: .utf8) else { return }

        for line in content.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard let (key, value) = parseTOMLLine(trimmed) else { continue }

            switch key {
            case "provider": provider = value
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
            ("groq_api_key", groqApiKey),
            ("groq_model", groqModel),
            ("groq_language", groqLanguage),
            ("hotkey", hotkey),
            ("trigger_mode", triggerMode),
            ("chinese_conversion", chineseConversion),
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

    func checkModelReady() {
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
        // Default data dir path
        return dataDir.appendingPathComponent("models/sensevoice-small").path
    }

    func downloadModel() {
        modelDownloading = true
        modelDownloadOutput = "Starting model download...\n"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            guard let binary = self.findOpenFlowBinary() else {
                DispatchQueue.main.async {
                    self.modelDownloadOutput += "Error: open-flow binary not found. Please install open-flow first.\n"
                    self.modelDownloading = false
                }
                return
            }
            let task = Process()
            task.executableURL = URL(fileURLWithPath: binary)
            task.arguments = ["setup"]

            let pipe = Pipe()
            task.standardOutput = pipe
            task.standardError = pipe

            pipe.fileHandleForReading.readabilityHandler = { handle in
                let data = handle.availableData
                if let str = String(data: data, encoding: .utf8), !str.isEmpty {
                    DispatchQueue.main.async {
                        self.modelDownloadOutput += str
                    }
                }
            }

            do {
                try task.run()
                task.waitUntilExit()
            } catch {
                DispatchQueue.main.async {
                    self.modelDownloadOutput += "\nError: \(error.localizedDescription)\n"
                }
            }

            pipe.fileHandleForReading.readabilityHandler = nil

            DispatchQueue.main.async {
                self.modelDownloading = false
                self.checkModelReady()
                if task.terminationStatus == 0 {
                    self.modelDownloadOutput += "\nModel download complete!\n"
                } else {
                    self.modelDownloadOutput += "\nDownload failed (exit code \(task.terminationStatus))\n"
                }
            }
        }
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
