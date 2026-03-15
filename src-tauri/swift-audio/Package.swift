// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "AuraAudio",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "AuraAudio", type: .static, targets: ["AuraAudio"])
    ],
    targets: [
        .target(
            name: "AuraAudio",
            path: "Sources/AuraAudio",
            linkerSettings: [
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("AVFAudio"),
            ]
        )
    ]
)
