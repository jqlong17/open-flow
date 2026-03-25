// swift-tools-version: 5.9
import Foundation
import PackageDescription

let perfDevUIEnabled = ProcessInfo.processInfo.environment["OPENFLOW_PERF_DEV_UI"] == "1"

let package = Package(
    name: "OpenFlowSettings",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "OpenFlowSettings",
            path: "Sources/OpenFlowSettings",
            swiftSettings: perfDevUIEnabled ? [
                .define("OPENFLOW_PERF_DEV_UI")
            ] : []
        ),
        .executableTarget(
            name: "OpenFlowSystemAudioHelper",
            path: "Sources/OpenFlowSystemAudioHelper"
        )
    ]
)
