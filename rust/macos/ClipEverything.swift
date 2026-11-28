import AppKit
import Carbon
import SwiftUI

private func local(_ ko: String, _ en: String) -> String {
    Locale.preferredLanguages.first?.hasPrefix("ko") == true ? ko : en
}

private let issuesURL = URL(string: "https://github.com/HosooJeong/ClipEverything/issues/new?template=macos-beta.yml")!

private struct Clip: Decodable, Identifiable {
    let id: Int64
    let name: String
    let tags: String
    let kind: Int
    let preview: String

    var title: String {
        if !name.isEmpty { return name }
        if kind == 5 { return local("이미지", "Image") }
        return preview.split(whereSeparator: \.isNewline).first.map(String.init) ?? local("텍스트", "Text")
    }
}

private struct Payload: Codable, Equatable {
    let kind: String
    let contents: String
}

private struct Request: Encodable {
    enum Operation: String, Encodable { case list, save, read, edit, delete }
    let operation: Operation
    var id: Int64? = nil
    var payload: Payload? = nil
    var name: String? = nil
    var tags: String? = nil
}

private struct Reply<Value: Decodable>: Decodable {
    let value: Value?
    let error: String?
}

private struct Message: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

private struct ClipStore {
    let directory: URL

    func send<Value: Decodable>(_ request: Request) throws -> Value {
        let input = String(decoding: try JSONEncoder().encode(request), as: UTF8.self)
        let pointer = directory.path.withCString { path in
            input.withCString { ce_request(path, $0) }
        }
        guard let pointer else { throw Message(local("보관함을 열 수 없어.", "Could not open the clip library.")) }
        defer { ce_free_string(pointer) }
        let output = Data(bytes: pointer, count: strlen(pointer))
        let reply = try JSONDecoder().decode(Reply<Value>.self, from: output)
        guard let value = reply.value else {
            throw Message(reply.error ?? local("보관함을 읽을 수 없어.", "Could not read the clip library."))
        }
        return value
    }
}

private final class Board: ObservableObject {
    @Published private(set) var clips: [Clip] = []
    @Published private(set) var status = ""
    @Published private(set) var shortcutWarning = ""
    let store: ClipStore

    init(store: ClipStore) {
        self.store = store
        run { clips = try store.send(Request(operation: .list)) }
    }

    @discardableResult private func run(_ operation: () throws -> Void) -> Bool {
        do {
            try operation()
            return true
        } catch {
            status = error.localizedDescription
            return false
        }
    }

    func shortcutsUnavailable() {
        shortcutWarning = local(
            "일부 전역 단축키를 등록할 수 없어. 메뉴 막대에서 같은 기능을 사용할 수 있어.",
            "Some global shortcuts are unavailable. The same actions are in the menu bar."
        )
    }

    func saveClipboard() {
        run {
            let pasteboard = NSPasteboard.general
            let types = pasteboard.types ?? []
            let excludedTypes = ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"]
            guard !types.contains(where: { excludedTypes.contains($0.rawValue) }) else {
                throw Message(local("보호되거나 임시로 표시된 클립은 등록할 수 없어.", "Protected or transient clips are skipped."))
            }
            guard !types.contains(.fileURL) else {
                throw Message(local("이 베타에서는 파일 복사를 지원하지 않아.", "File clips are unavailable in this beta."))
            }
            let payload: Payload
            if let png = pasteboard.data(forType: .png) {
                payload = try imagePayload(png)
            } else if let tiff = pasteboard.data(forType: .tiff),
                      let bitmap = NSBitmapImageRep(data: tiff),
                      let png = bitmap.representation(using: .png, properties: [:]) {
                payload = try imagePayload(png)
            } else if let text = pasteboard.string(forType: .string), !text.isEmpty {
                guard text.utf8.count <= 16 * 1024 * 1024 else {
                    throw Message(local("텍스트가 너무 커.", "The clipboard text is too large."))
                }
                payload = Payload(kind: "text", contents: text)
            } else {
                throw Message(local("클립보드에 등록할 텍스트나 이미지가 없어.", "Copy text or an image before saving a clip."))
            }
            let _: Int64 = try store.send(Request(operation: .save, payload: payload))
            clips = try store.send(Request(operation: .list))
            status = local("클립을 등록했어.", "Clip saved.")
        }
    }

    private func imagePayload(_ png: Data) throws -> Payload {
        guard png.count <= 32 * 1024 * 1024 else {
            throw Message(local("이미지는 32MiB까지 등록할 수 있어.", "Images must be 32 MiB or smaller."))
        }
        return Payload(kind: "png", contents: png.base64EncodedString())
    }

    func copy(_ clip: Clip) {
        run {
            let payload: Payload = try store.send(Request(operation: .read, id: clip.id))
            let item = NSPasteboardItem()
            let prepared: Bool
            if payload.kind == "text" {
                prepared = item.setString(payload.contents, forType: .string)
            } else if payload.kind == "png", let bytes = Data(base64Encoded: payload.contents) {
                prepared = item.setData(bytes, forType: .png)
            } else {
                prepared = false
            }
            guard prepared else { throw Message(local("복사할 수 없는 클립이야.", "This clip cannot be copied.")) }
            NSPasteboard.general.clearContents()
            guard NSPasteboard.general.writeObjects([item]) else {
                throw Message(local("클립보드에 복사하지 못했어.", "Could not copy to the clipboard."))
            }
            status = local("복사했어. 원하는 앱에서 ⌘V로 붙여넣어.", "Copied. Press ⌘V in the destination app.")
        }
    }

    func edit(_ clip: Clip, name: String, tags: String) -> Bool {
        run {
            let _: Bool = try store.send(Request(operation: .edit, id: clip.id, name: name, tags: tags))
            clips = try store.send(Request(operation: .list))
            status = local("이름과 태그를 저장했어.", "Name and tags saved.")
        }
    }

    func delete(_ clip: Clip) {
        run {
            let _: Bool = try store.send(Request(operation: .delete, id: clip.id))
            clips = try store.send(Request(operation: .list))
            status = local("클립을 삭제했어.", "Clip deleted.")
        }
    }
}

private struct BoardView: View {
    @ObservedObject var board: Board
    @State private var query = ""
    @State private var editing: Clip?
    @State private var deleting: Clip?

    private var matches: [Clip] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return board.clips }
        return board.clips.filter { clip in
            [clip.title, clip.tags, clip.preview].contains { $0.localizedStandardContains(term) }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                TextField(local("이름, 태그, 텍스트 검색", "Search names, tags, or text"), text: $query)
                    .textFieldStyle(.roundedBorder)
                Button {
                    board.saveClipboard()
                } label: {
                    Label(local("클립보드 등록", "Save clipboard"), systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
                Link(destination: issuesURL) {
                    Label(local("이슈 제보", "Report an issue"), systemImage: "arrow.up.right")
                }
            }
            if matches.isEmpty {
                ContentUnavailableView(
                    board.clips.isEmpty ? local("아직 등록한 클립이 없어", "No saved clips") : local("검색 결과가 없어", "No results"),
                    systemImage: board.clips.isEmpty ? "clipboard" : "magnifyingglass"
                )
                .frame(maxHeight: .infinity)
            } else {
                List(matches) { clip in
                    HStack(spacing: 10) {
                        Image(systemName: clip.kind == 5 ? "photo" : "text.alignleft")
                            .foregroundStyle(.secondary)
                            .frame(width: 20)
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(clip.title).lineLimit(1)
                            if !clip.tags.isEmpty {
                                Text(clip.tags).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        Spacer(minLength: 12)
                        Button(local("복사", "Copy")) { board.copy(clip) }
                        Menu {
                            Button(local("이름·태그 편집", "Edit name and tags")) { editing = clip }
                            Button(local("삭제", "Delete"), role: .destructive) { deleting = clip }
                        } label: {
                            Image(systemName: "ellipsis.circle")
                        }
                        .menuStyle(.borderlessButton)
                        .fixedSize()
                        .accessibilityLabel(local("클립 작업", "Clip actions"))
                    }
                    .buttonStyle(.borderless)
                    .padding(.vertical, 3)
                }
            }
            Text(board.status.isEmpty ? local("⌃⌥C 클립보드 등록   ⌃⌥V 버튼판 열기", "⌃⌥C Save clipboard   ⌃⌥V Open board") : board.status)
                .foregroundStyle(.secondary)
                .font(.callout)
            if !board.shortcutWarning.isEmpty {
                Text(board.shortcutWarning).font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(minWidth: 540, minHeight: 340)
        .sheet(item: $editing) { clip in
            EditClip(clip: clip, board: board)
        }
        .alert(
            local("이 클립을 삭제할까?", "Delete this clip?"),
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { clip in
            Button(local("삭제", "Delete"), role: .destructive) { board.delete(clip) }
        }
    }
}

private struct EditClip: View {
    let clip: Clip
    @ObservedObject var board: Board
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var tags: String
    @State private var error = ""

    init(clip: Clip, board: Board) {
        self.clip = clip
        self.board = board
        _name = State(initialValue: clip.name)
        _tags = State(initialValue: clip.tags)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(local("클립 편집", "Edit clip")).font(.headline)
            TextField(local("이름", "Name"), text: $name)
            TextField(local("태그", "Tags"), text: $tags)
            if !error.isEmpty { Text(error).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button(local("취소", "Cancel")) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(local("저장", "Save")) {
                    if board.edit(clip, name: name, tags: tags) { dismiss() }
                    else { error = board.status }
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 360)
    }
}

private final class Application: NSObject, NSApplicationDelegate {
    private var board: Board?
    private var window: NSWindow?
    private var statusItem: NSStatusItem?
    private var hotKeyHandler: EventHandlerRef?
    private var hotKeys: [EventHotKeyRef] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let support = try FileManager.default.url(
                for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
            )
            let folder = support.appendingPathComponent("ClipEverything", isDirectory: true)
            try FileManager.default.createDirectory(
                at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700]
            )
            let board = Board(store: ClipStore(directory: folder))
            self.board = board
            let controller = NSHostingController(rootView: BoardView(board: board))
            let window = NSWindow(contentViewController: controller)
            window.title = "ClipEverything"
            window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
            window.setContentSize(NSSize(width: 670, height: 470))
            window.contentMinSize = NSSize(width: 540, height: 340)
            window.isReleasedWhenClosed = false
            window.center()
            self.window = window
            setMenus()
            if !registerHotKeys() { board.shortcutsUnavailable() }
            showBoard()
        } catch {
            NSAlert(error: error).runModal()
            NSApp.terminate(nil)
        }
    }

    private func setMenus() {
        let status = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        let icon = NSImage(systemSymbolName: "clipboard", accessibilityDescription: "ClipEverything")
        icon?.isTemplate = true
        status.button?.image = icon
        let menu = NSMenu()
        addActions(to: menu)
        status.menu = menu
        statusItem = status

        let main = NSMenu()
        let app = NSMenuItem()
        let appMenu = NSMenu(title: "ClipEverything")
        addActions(to: appMenu)
        app.submenu = appMenu
        main.addItem(app)
        let edit = NSMenuItem()
        let editMenu = NSMenu(title: local("편집", "Edit"))
        for (title, action, key) in [
            (local("잘라내기", "Cut"), #selector(NSText.cut(_:)), "x"),
            (local("복사", "Copy"), #selector(NSText.copy(_:)), "c"),
            (local("붙여넣기", "Paste"), #selector(NSText.paste(_:)), "v"),
            (local("모두 선택", "Select All"), #selector(NSText.selectAll(_:)), "a"),
        ] {
            editMenu.addItem(withTitle: title, action: action, keyEquivalent: key)
        }
        edit.submenu = editMenu
        main.addItem(edit)
        NSApp.mainMenu = main
    }

    private func addActions(to menu: NSMenu) {
        menu.addItem(withTitle: local("버튼판 열기", "Open board"), action: #selector(showBoard), keyEquivalent: "").target = self
        menu.addItem(withTitle: local("클립보드 등록", "Save clipboard"), action: #selector(saveFromMenu), keyEquivalent: "").target = self
        menu.addItem(withTitle: local("이슈 제보", "Report an issue"), action: #selector(reportIssue), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: local("ClipEverything 종료", "Quit ClipEverything"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q").target = NSApp
    }

    @objc private func showBoard() {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    @objc private func saveFromMenu() {
        board?.saveClipboard()
        showBoard()
    }

    @objc private func reportIssue() {
        NSWorkspace.shared.open(issuesURL)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showBoard()
        return false
    }

    private func registerHotKeys() -> Bool {
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let target = GetApplicationEventTarget()
        let status = InstallEventHandler(target, { _, event, context in
            guard let event, let context else { return OSStatus(eventNotHandledErr) }
            var identifier = EventHotKeyID()
            let result = GetEventParameter(
                event, OSType(kEventParamDirectObject), OSType(typeEventHotKeyID), nil,
                numericCast(MemoryLayout<EventHotKeyID>.size), nil, &identifier
            )
            guard result == noErr, identifier.signature == 0x434C4950 else { return OSStatus(eventNotHandledErr) }
            let app = Unmanaged<Application>.fromOpaque(context).takeUnretainedValue()
            switch identifier.id {
            case 1: app.saveFromMenu()
            case 2: app.showBoard()
            default: return OSStatus(eventNotHandledErr)
            }
            return noErr
        }, 1, &spec, Unmanaged.passUnretained(self).toOpaque(), &hotKeyHandler)
        guard status == noErr else { return false }
        for (id, key) in [(1, kVK_ANSI_C), (2, kVK_ANSI_V)] {
            var reference: EventHotKeyRef?
            let result = RegisterEventHotKey(
                UInt32(key), UInt32(controlKey | optionKey), EventHotKeyID(signature: 0x434C4950, id: UInt32(id)),
                target, 0, &reference
            )
            if result == noErr, let reference { hotKeys.append(reference) }
        }
        return hotKeys.count == 2
    }

    func applicationWillTerminate(_ notification: Notification) {
        for hotKey in hotKeys { UnregisterEventHotKey(hotKey) }
        if let hotKeyHandler { RemoveEventHandler(hotKeyHandler) }
    }
}

private func smokeTest() throws {
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: folder) }
    let store = ClipStore(directory: folder)
    let text = Payload(kind: "text", contents: "ClipEverything\n한글과 이모지 🧩")
    let id: Int64 = try store.send(Request(operation: .save, payload: text))
    let same: Int64 = try store.send(Request(operation: .save, payload: text))
    let read: Payload = try store.send(Request(operation: .read, id: id))
    let edited: Bool = try store.send(Request(operation: .edit, id: id, name: "Beta", tags: "test"))
    let clips: [Clip] = try store.send(Request(operation: .list))
    guard id == same, text == read, edited, clips.count == 1, clips.first?.title == "Beta", clips.first?.tags == "test" else {
        throw Message("Text, deduplication, or metadata round-trip failed.")
    }
    let png = Payload(kind: "png", contents: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==")
    let pngID: Int64 = try store.send(Request(operation: .save, payload: png))
    let image: Payload = try store.send(Request(operation: .read, id: pngID))
    let deleted: Bool = try store.send(Request(operation: .delete, id: id))
    let remaining: [Clip] = try store.send(Request(operation: .list))
    guard image == png, deleted, remaining.map(\.id) == [pngID], remaining.first?.kind == 5 else {
        throw Message("Image round-trip or deletion failed.")
    }
    print("macOS Swift/Rust clip-store smoke test passed")
}

@main
private struct ClipEverything {
    static func main() throws {
        if CommandLine.arguments.contains("--smoke-test") {
            try smokeTest()
            return
        }
        let app = NSApplication.shared
        let delegate = Application()
        app.setActivationPolicy(.regular)
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
