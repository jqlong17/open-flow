import Foundation
import AVFAudio
import CoreGraphics
import CoreMedia
import ScreenCaptureKit

private let shareableContentRetryDelaysNs: [UInt64] = [
    0,
    200_000_000,
    400_000_000,
    800_000_000,
]

struct ScreenRecordingPermissionSnapshot: Encodable {
    let screenRecording: Bool

    enum CodingKeys: String, CodingKey {
        case screenRecording = "screen_recording"
    }
}

struct ShareableDisplaySnapshot: Encodable {
    let id: UInt32
    let width: Int
    let height: Int
}

struct ShareableApplicationSnapshot: Encodable {
    let processID: Int32
    let bundleIdentifier: String
    let applicationName: String

    enum CodingKeys: String, CodingKey {
        case processID = "process_id"
        case bundleIdentifier = "bundle_identifier"
        case applicationName = "application_name"
    }
}

struct ShareableContentSnapshot: Encodable {
    let screenRecordingGranted: Bool
    let displays: [ShareableDisplaySnapshot]
    let applications: [ShareableApplicationSnapshot]
    let windowCount: Int

    enum CodingKeys: String, CodingKey {
        case screenRecordingGranted = "screen_recording_granted"
        case displays
        case applications
        case windowCount = "window_count"
    }
}

struct AudioProbeSnapshot: Encodable {
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

private func extractMonoSamples(from sampleBuffer: CMSampleBuffer) -> [Float]? {
    guard let formatDescription = sampleBuffer.formatDescription,
          let asbd = formatDescription.audioStreamBasicDescription else {
        return nil
    }

    let frameCount = CMSampleBufferGetNumSamples(sampleBuffer)
    guard frameCount > 0 else { return nil }

    let channelCount = Int(asbd.mChannelsPerFrame)
    guard channelCount > 0 else { return nil }

    let isFloat = (asbd.mFormatFlags & kAudioFormatFlagIsFloat) != 0
    let isSignedInteger = (asbd.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0
    let isNonInterleaved = (asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0

    var monoSamples = Array<Float>(repeating: 0, count: frameCount)

    do {
        try sampleBuffer.withAudioBufferList { audioBuffers, _ in
            if isNonInterleaved {
                for channelIndex in 0..<min(channelCount, audioBuffers.count) {
                    let audioBuffer = audioBuffers[channelIndex]
                    guard let data = audioBuffer.mData else { continue }

                    if isFloat {
                        let pointer = data.bindMemory(to: Float.self, capacity: frameCount)
                        for frame in 0..<frameCount {
                            monoSamples[frame] += pointer[frame]
                        }
                    } else if isSignedInteger && asbd.mBitsPerChannel == 16 {
                        let pointer = data.bindMemory(to: Int16.self, capacity: frameCount)
                        for frame in 0..<frameCount {
                            monoSamples[frame] += Float(pointer[frame]) / 32768.0
                        }
                    }
                }

                let invChannelCount = 1.0 / Float(channelCount)
                for frame in 0..<frameCount {
                    monoSamples[frame] *= invChannelCount
                }
            } else {
                guard let audioBuffer = audioBuffers.first, let data = audioBuffer.mData else {
                    return
                }

                if isFloat {
                    let pointer = data.bindMemory(to: Float.self, capacity: frameCount * channelCount)
                    for frame in 0..<frameCount {
                        var mixed: Float = 0
                        for channel in 0..<channelCount {
                            mixed += pointer[frame * channelCount + channel]
                        }
                        monoSamples[frame] = mixed / Float(channelCount)
                    }
                } else if isSignedInteger && asbd.mBitsPerChannel == 16 {
                    let pointer = data.bindMemory(to: Int16.self, capacity: frameCount * channelCount)
                    for frame in 0..<frameCount {
                        var mixed: Float = 0
                        for channel in 0..<channelCount {
                            mixed += Float(pointer[frame * channelCount + channel]) / 32768.0
                        }
                        monoSamples[frame] = mixed / Float(channelCount)
                    }
                }
            }
        }
    } catch {
        fputs("[OpenFlowSystemAudioHelper] extractMonoSamples failed: \(error.localizedDescription)\n", stderr)
        return nil
    }

    return monoSamples
}

private final class AudioProbeCollector: NSObject, SCStreamOutput, SCStreamDelegate {
    private let startedAt = Date()
    private let lock = NSLock()

    private var callbacks = 0
    private var sampleCount = 0
    private var sampleRate = 0.0
    private var channels = 0
    private var firstCallbackLatencyMs: Double?
    private var stopError: String?

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of outputType: SCStreamOutputType) {
        guard outputType == .audio else { return }
        guard CMSampleBufferIsValid(sampleBuffer) else { return }

        let format = CMSampleBufferGetFormatDescription(sampleBuffer)
        let asbd = format.flatMap { CMAudioFormatDescriptionGetStreamBasicDescription($0) }?.pointee

        lock.lock()
        callbacks += 1
        sampleCount += CMSampleBufferGetNumSamples(sampleBuffer)
        if let asbd {
            sampleRate = asbd.mSampleRate
            channels = Int(asbd.mChannelsPerFrame)
        }
        if firstCallbackLatencyMs == nil {
            firstCallbackLatencyMs = Date().timeIntervalSince(startedAt) * 1000
        }
        lock.unlock()
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        lock.lock()
        stopError = error.localizedDescription
        lock.unlock()
    }

    func snapshot(mode: String, target: String, durationSeconds: Double) -> AudioProbeSnapshot {
        lock.lock()
        defer { lock.unlock() }

        return AudioProbeSnapshot(
            mode: mode,
            target: target,
            durationSeconds: durationSeconds,
            callbacks: callbacks,
            sampleCount: sampleCount,
            sampleRate: sampleRate,
            channels: channels,
            firstCallbackLatencyMs: firstCallbackLatencyMs,
            error: stopError
        )
    }
}

private final class AudioPipeCollector: NSObject, SCStreamOutput, SCStreamDelegate {
    private let outputHandle = FileHandle.standardOutput
    private let lock = NSLock()
    private var stopError: String?
    private var firstChunkWritten = false

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of outputType: SCStreamOutputType) {
        guard outputType == .audio else { return }
        guard CMSampleBufferIsValid(sampleBuffer) else { return }

        guard let monoSamples = extractMonoSamples(from: sampleBuffer), !monoSamples.isEmpty else {
            return
        }

        var mutableSamples = monoSamples
        let data = mutableSamples.withUnsafeMutableBytes { rawBuffer in
            Data(bytes: rawBuffer.baseAddress!, count: rawBuffer.count)
        }

        do {
            try outputHandle.write(contentsOf: data)
            if !firstChunkWritten {
                firstChunkWritten = true
                fputs("[OpenFlowSystemAudioHelper] first audio chunk written bytes=\(data.count)\n", stderr)
            }
        } catch {
            stopError = error.localizedDescription
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        lock.lock()
        stopError = error.localizedDescription
        lock.unlock()
        fputs("[OpenFlowSystemAudioHelper] stream stopped: \(error.localizedDescription)\n", stderr)
    }
}

@main
struct OpenFlowSystemAudioHelper {
    static func main() async {
        do {
            try await run()
        } catch {
            fputs("OpenFlowSystemAudioHelper error: \(error.localizedDescription)\n", stderr)
            exit(1)
        }
    }

    private static func run() async throws {
        let args = Array(CommandLine.arguments.dropFirst())
        guard let command = args.first else {
            printUsage()
            exit(2)
        }

        switch command {
        case "permissions":
            try emitJSON(ScreenRecordingPermissionSnapshot(screenRecording: preflightScreenRecordingPermission()))
        case "request-permission":
            let granted = requestScreenRecordingPermission()
            try emitJSON(ScreenRecordingPermissionSnapshot(screenRecording: granted))
        case "list-shareable":
            try await emitShareableContentSnapshot()
        case "probe-desktop":
            try await emitDesktopAudioProbeSnapshot(args: Array(args.dropFirst()))
        case "probe-application":
            try await emitApplicationAudioProbeSnapshot(args: Array(args.dropFirst()))
        case "stream-desktop":
            try await runDesktopAudioStream(args: Array(args.dropFirst()))
        case "stream-application":
            try await runApplicationAudioStream(args: Array(args.dropFirst()))
        default:
            printUsage()
            exit(2)
        }
    }

    private static func printUsage() {
        let text = """
        Usage:
          OpenFlowSystemAudioHelper permissions
          OpenFlowSystemAudioHelper request-permission
          OpenFlowSystemAudioHelper list-shareable
          OpenFlowSystemAudioHelper probe-desktop [--display-id <id>] [--seconds <duration>]
          OpenFlowSystemAudioHelper probe-application --pid <pid> [--seconds <duration>]
          OpenFlowSystemAudioHelper stream-desktop [--display-id <id>]
          OpenFlowSystemAudioHelper stream-application --pid <pid>
        """
        print(text)
    }

    private static func emitJSON<T: Encodable>(_ value: T) throws {
        let encoder = JSONEncoder()
        let data = try encoder.encode(value)
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([0x0a]))
    }

    private static func preflightScreenRecordingPermission() -> Bool {
        CGPreflightScreenCaptureAccess()
    }

    private static func requestScreenRecordingPermission() -> Bool {
        CGRequestScreenCaptureAccess()
    }

    private static func emitShareableContentSnapshot() async throws {
        guard preflightScreenRecordingPermission() else {
            try emitJSON(
                ShareableContentSnapshot(
                    screenRecordingGranted: false,
                    displays: [],
                    applications: [],
                    windowCount: 0
                )
            )
            return
        }

        let content = try await fetchShareableContentForDesktopCapture()

        let displays = content.displays.map {
            ShareableDisplaySnapshot(
                id: $0.displayID,
                width: $0.width,
                height: $0.height
            )
        }

        let applications = content.applications
            .filter { app in
                !app.applicationName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    && app.bundleIdentifier != Bundle.main.bundleIdentifier
            }
            .map {
                ShareableApplicationSnapshot(
                    processID: $0.processID,
                    bundleIdentifier: $0.bundleIdentifier,
                    applicationName: $0.applicationName
                )
            }
            .sorted {
                $0.applicationName.localizedCaseInsensitiveCompare($1.applicationName) == .orderedAscending
            }

        try emitJSON(
            ShareableContentSnapshot(
                screenRecordingGranted: true,
                displays: displays,
                applications: applications,
                windowCount: content.windows.count
            )
        )
    }

    private static func emitDesktopAudioProbeSnapshot(args: [String]) async throws {
        guard preflightScreenRecordingPermission() else {
            try emitJSON(
                AudioProbeSnapshot(
                    mode: "desktop",
                    target: "permission_required",
                    durationSeconds: 0,
                    callbacks: 0,
                    sampleCount: 0,
                    sampleRate: 0,
                    channels: 0,
                    firstCallbackLatencyMs: nil,
                    error: "Screen recording permission is required."
                )
            )
            return
        }

        let durationSeconds = parseDoubleFlag(args: args, name: "--seconds") ?? 3.0
        let displayID = parseUInt32Flag(args: args, name: "--display-id")
        let content = try await fetchShareableContentForDesktopCapture()

        guard let display = resolveDisplay(content: content, displayID: displayID) else {
            try emitJSON(
                AudioProbeSnapshot(
                    mode: "desktop",
                    target: displayID.map { "display:\($0)" } ?? "display:auto",
                    durationSeconds: durationSeconds,
                    callbacks: 0,
                    sampleCount: 0,
                    sampleRate: 0,
                    channels: 0,
                    firstCallbackLatencyMs: nil,
                    error: "No eligible display found."
                )
            )
            return
        }

        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )

        let snapshot = try await runAudioProbe(
            filter: filter,
            target: "display:\(display.displayID)",
            mode: "desktop",
            durationSeconds: durationSeconds,
            display: display
        )
        try emitJSON(snapshot)
    }

    private static func emitApplicationAudioProbeSnapshot(args: [String]) async throws {
        guard preflightScreenRecordingPermission() else {
            try emitJSON(
                AudioProbeSnapshot(
                    mode: "application",
                    target: "permission_required",
                    durationSeconds: 0,
                    callbacks: 0,
                    sampleCount: 0,
                    sampleRate: 0,
                    channels: 0,
                    firstCallbackLatencyMs: nil,
                    error: "Screen recording permission is required."
                )
            )
            return
        }

        guard let processID = parseInt32Flag(args: args, name: "--pid") else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Missing required --pid argument."]
            )
        }

        let durationSeconds = parseDoubleFlag(args: args, name: "--seconds") ?? 3.0
        let content = try await fetchShareableContentForDesktopCapture()

        guard let display = content.displays.first else {
            try emitJSON(
                AudioProbeSnapshot(
                    mode: "application",
                    target: "pid:\(processID)",
                    durationSeconds: durationSeconds,
                    callbacks: 0,
                    sampleCount: 0,
                    sampleRate: 0,
                    channels: 0,
                    firstCallbackLatencyMs: nil,
                    error: "No display available for ScreenCaptureKit filter."
                )
            )
            return
        }

        guard let app = content.applications.first(where: { $0.processID == processID }) else {
            try emitJSON(
                AudioProbeSnapshot(
                    mode: "application",
                    target: "pid:\(processID)",
                    durationSeconds: durationSeconds,
                    callbacks: 0,
                    sampleCount: 0,
                    sampleRate: 0,
                    channels: 0,
                    firstCallbackLatencyMs: nil,
                    error: "Target application was not found in shareable content."
                )
            )
            return
        }

        let filter = SCContentFilter(
            display: display,
            including: [app],
            exceptingWindows: []
        )

        let snapshot = try await runAudioProbe(
            filter: filter,
            target: "app:\(app.applicationName)#\(app.processID)",
            mode: "application",
            durationSeconds: durationSeconds,
            display: display
        )
        try emitJSON(snapshot)
    }

    private static func runDesktopAudioStream(args: [String]) async throws {
        guard preflightScreenRecordingPermission() else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 10,
                userInfo: [NSLocalizedDescriptionKey: "Screen recording permission is required."]
            )
        }

        let displayID = parseUInt32Flag(args: args, name: "--display-id")
        let content = try await fetchShareableContentForDesktopCapture()
        guard let display = resolveDisplay(content: content, displayID: displayID) else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 11,
                userInfo: [NSLocalizedDescriptionKey: "No eligible display found."]
            )
        }

        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )
        try await runAudioPipe(filter: filter, display: display)
    }

    private static func runApplicationAudioStream(args: [String]) async throws {
        guard preflightScreenRecordingPermission() else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 12,
                userInfo: [NSLocalizedDescriptionKey: "Screen recording permission is required."]
            )
        }

        guard let processID = parseInt32Flag(args: args, name: "--pid") else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 13,
                userInfo: [NSLocalizedDescriptionKey: "Missing required --pid argument."]
            )
        }

        let content = try await fetchShareableContentForDesktopCapture()
        guard let display = content.displays.first else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 14,
                userInfo: [NSLocalizedDescriptionKey: "No display available for ScreenCaptureKit filter."]
            )
        }
        guard let app = content.applications.first(where: { $0.processID == processID }) else {
            throw NSError(
                domain: "OpenFlowSystemAudioHelper",
                code: 15,
                userInfo: [NSLocalizedDescriptionKey: "Target application was not found in shareable content."]
            )
        }

        let filter = SCContentFilter(
            display: display,
            including: [app],
            exceptingWindows: []
        )
        try await runAudioPipe(filter: filter, display: display)
    }

    private static func runAudioProbe(
        filter: SCContentFilter,
        target: String,
        mode: String,
        durationSeconds: Double,
        display: SCDisplay
    ) async throws -> AudioProbeSnapshot {
        let collector = AudioProbeCollector()
        let configuration = SCStreamConfiguration()
        configuration.capturesAudio = true
        configuration.excludesCurrentProcessAudio = true
        configuration.width = display.width
        configuration.height = display.height
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 60)
        configuration.queueDepth = 3

        let stream = SCStream(filter: filter, configuration: configuration, delegate: collector)
        let sampleQueue = DispatchQueue(label: "com.openflow.system-audio.probe")
        try stream.addStreamOutput(collector, type: .audio, sampleHandlerQueue: sampleQueue)

        do {
            try await stream.startCapture()
            try await Task.sleep(nanoseconds: UInt64(max(durationSeconds, 0.5) * 1_000_000_000))
            try await stream.stopCapture()
        } catch {
            return AudioProbeSnapshot(
                mode: mode,
                target: target,
                durationSeconds: durationSeconds,
                callbacks: 0,
                sampleCount: 0,
                sampleRate: 0,
                channels: 0,
                firstCallbackLatencyMs: nil,
                error: error.localizedDescription
            )
        }

        return collector.snapshot(
            mode: mode,
            target: target,
            durationSeconds: durationSeconds
        )
    }

    private static func runAudioPipe(filter: SCContentFilter, display: SCDisplay) async throws {
        let collector = AudioPipeCollector()
        let configuration = SCStreamConfiguration()
        configuration.capturesAudio = true
        configuration.excludesCurrentProcessAudio = true
        configuration.width = display.width
        configuration.height = display.height
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 60)
        configuration.queueDepth = 3

        let stream = SCStream(filter: filter, configuration: configuration, delegate: collector)
        let sampleQueue = DispatchQueue(label: "com.openflow.system-audio.stream")
        try stream.addStreamOutput(collector, type: .audio, sampleHandlerQueue: sampleQueue)
        try await stream.startCapture()

        while true {
            try await Task.sleep(nanoseconds: 60_000_000_000)
        }
    }

    private static func fetchShareableContent(onScreenWindowsOnly: Bool) async throws -> SCShareableContent {
        try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: onScreenWindowsOnly
        )
    }

    private static func fetchShareableContentForDesktopCapture() async throws -> SCShareableContent {
        var lastContent: SCShareableContent?

        for (attempt, delayNs) in shareableContentRetryDelaysNs.enumerated() {
            if delayNs > 0 {
                try await Task.sleep(nanoseconds: delayNs)
            }

            let content = try await fetchShareableContent(onScreenWindowsOnly: true)
            lastContent = content
            if !content.displays.isEmpty {
                if attempt > 0 {
                    fputs("[OpenFlowSystemAudioHelper] fetched shareable content after retry attempt=\(attempt + 1) displays=\(content.displays.count)\n", stderr)
                }
                return content
            }

            fputs("[OpenFlowSystemAudioHelper] shareable content had no displays attempt=\(attempt + 1) onScreenWindowsOnly=true windows=\(content.windows.count) apps=\(content.applications.count)\n", stderr)
        }

        let fallbackContent = try await fetchShareableContent(onScreenWindowsOnly: false)
        if !fallbackContent.displays.isEmpty {
            fputs("[OpenFlowSystemAudioHelper] fallback shareable content succeeded with onScreenWindowsOnly=false displays=\(fallbackContent.displays.count)\n", stderr)
            return fallbackContent
        }

        if let lastContent {
            return lastContent
        }

        return fallbackContent
    }

    private static func resolveDisplay(content: SCShareableContent, displayID: UInt32?) -> SCDisplay? {
        if let displayID {
            return content.displays.first(where: { $0.displayID == displayID })
        }
        return content.displays.first
    }

    private static func parseUInt32Flag(args: [String], name: String) -> UInt32? {
        guard let raw = parseFlag(args: args, name: name) else { return nil }
        return UInt32(raw)
    }

    private static func parseInt32Flag(args: [String], name: String) -> Int32? {
        guard let raw = parseFlag(args: args, name: name) else { return nil }
        return Int32(raw)
    }

    private static func parseDoubleFlag(args: [String], name: String) -> Double? {
        guard let raw = parseFlag(args: args, name: name) else { return nil }
        return Double(raw)
    }

    private static func parseFlag(args: [String], name: String) -> String? {
        guard let index = args.firstIndex(of: name), args.count > index + 1 else {
            return nil
        }
        return args[index + 1]
    }

}
