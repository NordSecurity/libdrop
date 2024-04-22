// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "LibdropSwift",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15),
        .tvOS(.v17)
    ],
    products: [
        .library(
            name: "LibdropSwift",
            targets: ["LibdropSwift", "norddropFFI"]),
    ],
    targets: [
        .target(
            name: "LibdropSwift",
            dependencies: [
                "norddropFFI"
            ],
            path: "Sources",
            linkerSettings: [.linkedFramework("CoreServices")]
            ),
        .binaryTarget(
            name: "norddropFFI",
            url: "$XCFRAMEWORK_URL",
            checksum: "$XCFRAMEWORK_CHECKSUM"
        )
    ]
)
