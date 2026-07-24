import Foundation
import SwiftRs
import Tauri
import UIKit
import UniformTypeIdentifiers

private struct BookmarkRequest: Decodable {
    let bookmarkId: String
}

final class IosVaultAccessPlugin: Plugin, UIDocumentPickerDelegate {
    private let fileManager = FileManager.default
    private let lock = NSLock()
    private var pickerInvoke: Invoke?
    private var activeBookmarkId: String?
    private var activeURL: URL?
    private var stagedBookmarkId: String?
    private var stagedURL: URL?

    deinit {
        rollbackStagedScope()
        releaseCurrentScope()
    }

    @objc func chooseFolder(_ invoke: Invoke) {
        DispatchQueue.main.async {
            guard self.pickerInvoke == nil else {
                invoke.reject("A folder picker is already open.")
                return
            }
            guard let viewController = self.manager.viewController else {
                self.pickerInvoke = nil
                invoke.reject("Files is unavailable right now. Try again.")
                return
            }
            self.pickerInvoke = invoke
            let picker = UIDocumentPickerViewController(
                forOpeningContentTypes: [.folder],
                asCopy: false
            )
            picker.delegate = self
            picker.allowsMultipleSelection = false
            viewController.present(picker, animated: true)
        }
    }

    @objc func resolveFolder(_ invoke: Invoke) throws {
        let request = try invoke.parseArgs(BookmarkRequest.self)
        do {
            invoke.resolve(try resolveBookmark(request.bookmarkId))
        } catch {
            invoke.reject(message(for: error))
        }
    }

    @objc func releaseActive(_ invoke: Invoke) {
        rollbackStagedScope()
        releaseCurrentScope()
        invoke.resolve()
    }

    @objc func commitStaged(_ invoke: Invoke) {
        commitStagedScope()
        invoke.resolve()
    }

    @objc func rollbackStaged(_ invoke: Invoke) {
        rollbackStagedScope()
        invoke.resolve()
    }

    @objc func forgetBookmark(_ invoke: Invoke) throws {
        let request = try invoke.parseArgs(BookmarkRequest.self)
        do {
            let file = try bookmarkFile(for: request.bookmarkId, createDirectory: false)
            lock.lock()
            let isActive = activeBookmarkId == request.bookmarkId
            let isStaged = stagedBookmarkId == request.bookmarkId
            lock.unlock()
            if fileManager.fileExists(atPath: file.path) {
                try fileManager.removeItem(at: file)
            }
            if isActive {
                releaseCurrentScope()
            }
            if isStaged {
                rollbackStagedScope()
            }
            invoke.resolve()
        } catch {
            invoke.reject(message(for: error))
        }
    }

    func documentPicker(
        _ controller: UIDocumentPickerViewController,
        didPickDocumentsAt urls: [URL]
    ) {
        guard let invoke = takePickerInvoke() else { return }
        guard let url = urls.first else {
            invoke.resolve(cancelledResult())
            return
        }

        do {
            guard url.startAccessingSecurityScopedResource() else {
                throw AccessError.denied
            }
            var adopted = false
            defer {
                if !adopted { url.stopAccessingSecurityScopedResource() }
            }

            let name = try validateFolder(url)
            let bookmarkId = existingBookmarkId(for: url) ?? UUID().uuidString.lowercased()
            let bookmark = try url.bookmarkData(
                options: .minimalBookmark,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
            try bookmark.write(to: bookmarkFile(for: bookmarkId, createDirectory: true), options: .atomic)
            stageScope(url: url, bookmarkId: bookmarkId)
            adopted = true
            invoke.resolve(result(bookmarkId: bookmarkId, url: url, name: name))
        } catch {
            invoke.reject(message(for: error))
        }
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        takePickerInvoke()?.resolve(cancelledResult())
    }

    private func resolveBookmark(_ bookmarkId: String) throws -> [String: Any] {
        guard UUID(uuidString: bookmarkId) != nil else { throw AccessError.missing }

        lock.lock()
        let currentURL = activeBookmarkId == bookmarkId ? activeURL : nil
        lock.unlock()
        if let currentURL {
            return result(bookmarkId: bookmarkId, url: currentURL, name: try validateFolder(currentURL))
        }

        let data = try Data(contentsOf: bookmarkFile(for: bookmarkId, createDirectory: false))
        var stale = false
        let url = try URL(
            resolvingBookmarkData: data,
            options: [],
            relativeTo: nil,
            bookmarkDataIsStale: &stale
        )
        guard url.startAccessingSecurityScopedResource() else { throw AccessError.denied }
        var adopted = false
        defer {
            if !adopted { url.stopAccessingSecurityScopedResource() }
        }

        let name = try validateFolder(url)
        if stale {
            let refreshed = try url.bookmarkData(
                options: .minimalBookmark,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
            try refreshed.write(to: bookmarkFile(for: bookmarkId, createDirectory: true), options: .atomic)
        }
        stageScope(url: url, bookmarkId: bookmarkId)
        adopted = true
        return result(bookmarkId: bookmarkId, url: url, name: name)
    }

    private func validateFolder(_ url: URL) throws -> String {
        let values = try url.resourceValues(forKeys: [.isDirectoryKey, .nameKey])
        guard values.isDirectory == true else { throw AccessError.notFolder }
        guard try url.checkResourceIsReachable() else { throw AccessError.unavailable }
        return values.name ?? url.lastPathComponent
    }

    private func stageScope(url: URL, bookmarkId: String) {
        lock.lock()
        let previous = stagedURL
        stagedURL = url
        stagedBookmarkId = bookmarkId
        lock.unlock()
        previous?.stopAccessingSecurityScopedResource()
    }

    private func commitStagedScope() {
        lock.lock()
        guard let next = stagedURL else {
            lock.unlock()
            return
        }
        let previous = activeURL
        activeURL = next
        activeBookmarkId = stagedBookmarkId
        stagedURL = nil
        stagedBookmarkId = nil
        lock.unlock()
        previous?.stopAccessingSecurityScopedResource()
    }

    private func rollbackStagedScope() {
        lock.lock()
        let staged = stagedURL
        stagedURL = nil
        stagedBookmarkId = nil
        lock.unlock()
        staged?.stopAccessingSecurityScopedResource()
    }

    private func releaseCurrentScope() {
        lock.lock()
        let previous = activeURL
        activeURL = nil
        activeBookmarkId = nil
        lock.unlock()
        previous?.stopAccessingSecurityScopedResource()
    }

    private func takePickerInvoke() -> Invoke? {
        let invoke = pickerInvoke
        pickerInvoke = nil
        return invoke
    }

    private func bookmarkDirectory(create: Bool) throws -> URL {
        let base = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = base.appendingPathComponent("VaultBookmarks", isDirectory: true)
        if create {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        return directory
    }

    private func bookmarkFile(for id: String, createDirectory: Bool) throws -> URL {
        guard UUID(uuidString: id) != nil else { throw AccessError.missing }
        let directory = try bookmarkDirectory(create: createDirectory)
        return directory.appendingPathComponent("\(id).bookmark")
    }

    private func existingBookmarkId(for selectedURL: URL) -> String? {
        guard
            let directory = try? bookmarkDirectory(create: true),
            let files = try? fileManager.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: nil
            )
        else { return nil }

        let selectedPath = selectedURL.standardizedFileURL.path
        for file in files where file.pathExtension == "bookmark" {
            guard let data = try? Data(contentsOf: file) else { continue }
            var stale = false
            guard
                let resolved = try? URL(
                    resolvingBookmarkData: data,
                    options: [],
                    relativeTo: nil,
                    bookmarkDataIsStale: &stale
                ),
                resolved.standardizedFileURL.path == selectedPath
            else { continue }
            let id = file.deletingPathExtension().lastPathComponent
            if UUID(uuidString: id) != nil { return id }
        }
        return nil
    }

    private func result(bookmarkId: String, url: URL, name: String) -> [String: Any] {
        ["bookmarkId": bookmarkId, "path": url.path, "name": name, "cancelled": false]
    }

    private func cancelledResult() -> [String: Any] {
        ["bookmarkId": "", "path": "", "name": "", "cancelled": true]
    }

    private func message(for error: Error) -> String {
        if let accessError = error as? AccessError { return accessError.localizedDescription }
        return "The selected folder is unavailable. Open Files and try again."
    }
}

private enum AccessError: LocalizedError {
    case denied
    case missing
    case notFolder
    case unavailable

    var errorDescription: String? {
        switch self {
        case .denied:
            return "HelixNotes no longer has access to this folder. Choose it again in Files."
        case .missing:
            return "Saved access to this folder is missing. Choose it again in Files."
        case .notFolder:
            return "The selected item is not a folder."
        case .unavailable:
            return "The selected folder is unavailable. Open Files and try again."
        }
    }
}

@_cdecl("init_plugin_ios_vault_access")
func initPlugin() -> Plugin {
    IosVaultAccessPlugin()
}
