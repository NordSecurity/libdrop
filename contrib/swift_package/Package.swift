// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "LibdropSwift",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15)
    ],
    products: [
        .library(
            name: "LibdropSwift",
            targets: ["LibdropSwift", "libdropFFI"]),
    ],
    targets: [
        .target(
            name: "LibdropSwift",
            dependencies: [
                "libdropFFI"
            ],
            path: "Sources",
            linkerSettings: [.linkedFramework("CoreServices")]
            ),
        .binaryTarget(
            name: "libdropFFI",
            url: "$XCFRAMEWORK_URL",
            checksum: "$XCFRAMEWORK_CHECKSUM"
        )
    ]
)
