// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-ios-vault-access",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "tauri-plugin-ios-vault-access",
            type: .static,
            targets: ["tauri-plugin-ios-vault-access"]
        )
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-ios-vault-access",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources"
        )
    ]
)
