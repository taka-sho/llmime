// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "llmime-settings",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "llmime-settings", targets: ["LlmimeSettings"])
    ],
    targets: [
        .executableTarget(
            name: "LlmimeSettings",
            path: "Sources/LlmimeSettings",
            linkerSettings: [
                .unsafeFlags([
                    "-L../../target/debug/deps",
                    "-lllmime_imk",
                ]),
            ]
        )
    ]
)
