// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "KarukanIME",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "KarukanIME",
            path: "Sources/KarukanIME",
            linkerSettings: [
                .linkedFramework("InputMethodKit")
            ]
        ),
        .testTarget(
            name: "KarukanIMETests",
            dependencies: ["KarukanIME"],
            path: "Tests/KarukanIMETests"
        ),
    ]
)
