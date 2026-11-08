// swift-tools-version: 6.2
import Foundation
import PackageDescription

// libtwip.dylib comes from this repository: `node crates/twip-capi/build-native.mjs` (moon run twip-capi:build) builds it
// into target/release, and the CTwip module map reads its header from crates/twip-capi/include.
// ponytail: links target/release by path, fine inside the repo; a binaryTarget xcframework when the package ships.
let lib = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../target/release").standardized.path

let swiftSettings: [SwiftSetting] = [
    // Async methods run on their caller's executor until they suspend, so calls made in order are queued in order.
    .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
    .enableUpcomingFeature("ExistentialAny"),
]

let package = Package(
    name: "Twip",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "Twip", targets: ["Twip"]),
        .library(name: "TwipUI", targets: ["TwipUI"]),
    ],
    targets: [
        .systemLibrary(name: "CTwip"),
        .target(
            name: "Twip",
            dependencies: ["CTwip"],
            swiftSettings: swiftSettings,
            linkerSettings: [.unsafeFlags(["-L", lib, "-Xlinker", "-rpath", "-Xlinker", lib])]
        ),
        .target(name: "TwipUI", dependencies: ["Twip", "CTwip"], swiftSettings: swiftSettings),
        .testTarget(name: "TwipTests", dependencies: ["Twip", "TwipUI", "CTwip"], swiftSettings: swiftSettings + [.swiftLanguageMode(.v5)]),
    ]
)
