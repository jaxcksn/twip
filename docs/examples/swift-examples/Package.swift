// swift-tools-version: 6.2
// Builds every Swift example the guides show against Twip and TwipUI. `swift run` opens the SwiftUI example.
import PackageDescription

let package = Package(
    name: "Examples",
    platforms: [.macOS(.v15)],
    dependencies: [.package(path: "../../../bindings/swift")],
    targets: [
        .executableTarget(
            name: "Examples",
            dependencies: [.product(name: "Twip", package: "swift"), .product(name: "TwipUI", package: "swift")],
            swiftSettings: [.unsafeFlags(["-warnings-as-errors"])]
        ),
    ]
)
