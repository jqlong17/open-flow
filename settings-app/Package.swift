// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "OpenFlowSettings",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "OpenFlowSettings",
            path: "Sources/OpenFlowSettings"
        )
    ]
)
