import CTwip
import Foundation

// The services: models the engine computes for an application's own UI (menus, find, spelling, navigation, styles,
// lists, review, elements, sub-templates, data parts, accessibility, shortcuts, gestures). Each is a lightweight view
// of its session; its calls queue in order with the session's operations.

extension EditorSession {
    /// The context menu.
    public var menus: Menus { Menus(session: self) }
    /// Dialog models: open, update, preview and apply.
    public var dialogs: Dialogs { Dialogs(session: self) }
    /// Find and replace.
    public var find: Find { Find(session: self) }
    /// Spell checking.
    public var spelling: Spelling { Spelling(session: self) }
    /// Headings, for a navigation pane.
    public var navigation: Navigation { Navigation(session: self) }
    /// The document's styles.
    public var styles: Styles { Styles(session: self) }
    /// The list galleries, and the characters of fonts for a symbol picker.
    public var lists: Lists { Lists(session: self) }
    /// Revisions, comments and the people who made them.
    public var review: Review { Review(session: self) }
    /// Merge-designer elements: fields and content controls.
    public var elements: Elements { Elements(session: self) }
    /// The template's sub-templates, and editing one in this session.
    public var subTemplates: SubTemplates { SubTemplates(session: self) }
    /// Custom XML data parts.
    public var dataParts: DataParts { DataParts(session: self) }
    /// The accessible structure and announcements.
    public var accessibility: Accessibility { Accessibility(session: self) }
    /// Application key shortcuts.
    public var shortcuts: Shortcuts { Shortcuts(session: self) }
    /// Pointer gesture bindings; they apply while a view presents the session.
    public var gestures: Gestures { Gestures(session: self) }
    /// The clipboard, for applications that move rich content themselves.
    public var clipboard: Clipboard { Clipboard(session: self) }

    /// The context menu.
    public struct Menus: Sendable {
        let session: EditorSession

        /// The context menu for the selection.
        public func contextMenu() async throws -> ContextMenuModel {
            try await session.required(TWIP_CONTEXT_MENU, as: ContextMenuModel.self)
        }

        /// Runs a menu item's action.
        @discardableResult
        public func invoke(_ action: MenuAction) async throws -> Commit {
            try await session.call(TWIP_INVOKE, action, as: Commit.self) ?? Commit()
        }
    }

    /// Dialog models: the engine computes a dialog's values for the selection and each field's state; your UI draws
    /// it, sends the user's changes as patches, and applies it. Cancelling needs no call.
    public struct Dialogs: Sendable {
        let session: EditorSession

        private struct Update: Encodable, Sendable { let model: DialogModel; let patch: DialogPatch }
        private struct OnStyle: Encodable, Sendable { let kind: DialogKind; let style: StyleDialog }
        private struct Preview: Encodable, Sendable { let model: DialogModel; let size: PreviewSize }

        /// Opens a dialog for the selection.
        /// - Throws: ``ErrorCode/unavailable`` when it doesn't open here (``EditorState/dialogs`` says which do).
        public func open(_ kind: DialogKind) async throws -> DialogModel {
            try await session.required(TWIP_DIALOG_OPEN, kind, as: DialogModel.self)
        }

        /// Opens the style dialog: a new style, or one to modify.
        public func open(style: StyleOpen) async throws -> DialogModel {
            try await session.required(TWIP_DIALOG_OPEN_STYLE, style, as: DialogModel.self)
        }

        /// Opens a Format menu dialog (font, paragraph, tabs, ...) on the style dialog's style instead of the selection.
        public func open(_ kind: DialogKind, on style: StyleDialog) async throws -> DialogModel {
            try await session.required(TWIP_DIALOG_OPEN_ON_STYLE, OnStyle(kind: kind, style: style), as: DialogModel.self)
        }

        /// The dialog after the user's changes: its values, and the fields they enable, disable or recompute.
        public func update(_ model: DialogModel, with patch: DialogPatch) async throws -> DialogModel {
            try await session.required(TWIP_DIALOG_UPDATE, Update(model: model, patch: patch), as: DialogModel.self)
        }

        /// Applies the dialog as one undo step.
        @discardableResult
        public func apply(_ model: DialogModel) async throws -> Commit {
            try await session.call(TWIP_DIALOG_APPLY, model, as: Commit.self) ?? Commit()
        }

        /// The style dialog with a Format menu dialog's changes, from a dialog ``open(_:on:)`` opened.
        public func applyToStyle(_ model: DialogModel) async throws -> DialogModel {
            try await session.required(TWIP_DIALOG_APPLY_TO_STYLE, model, as: DialogModel.self)
        }

        /// The dialog's preview picture.
        public func preview(_ model: DialogModel, size: PreviewSize) async throws -> PreviewPicture {
            try await session.required(TWIP_DIALOG_PREVIEW, Preview(model: model, size: size), as: PreviewPicture.self)
        }

        /// Selects the content of an accessibility issue, from the accessibility dialog.
        public func selectIssue(_ id: String) async throws {
            try await session.call(TWIP_DIALOG_SELECT_ISSUE, id)
        }
    }

    /// Find and replace.
    public struct Find: Sendable {
        let session: EditorSession

        private struct Request: Encodable, Sendable { let query: FindQuery; let direction: FindDirection }
        private struct Replace: Encodable, Sendable { let query: FindQuery; let with: String }
        private struct Count: Decodable, Sendable { let count: Int }

        /// Selects the next match after the selection, wrapping at the end.
        public func next(_ query: FindQuery) async throws -> FindResult {
            try await session.required(TWIP_FIND, Request(query: query, direction: .next), as: FindResult.self)
        }

        /// Selects the previous match before the selection, wrapping at the start.
        public func previous(_ query: FindQuery) async throws -> FindResult {
            try await session.required(TWIP_FIND, Request(query: query, direction: .previous), as: FindResult.self)
        }

        /// Every match, with its context, for a results list.
        public func all(_ query: FindQuery) async throws -> [FindMatch] {
            try await session.call(TWIP_FIND_ALL, query, as: [FindMatch].self) ?? []
        }

        /// Selects a match from ``all(_:)``.
        public func select(_ match: MatchID) async throws {
            try await session.call(TWIP_SELECT_MATCH, match)
        }

        /// Replaces the selected match, then selects the next one.
        public func replace(_ query: FindQuery, with replacement: String) async throws -> FindResult {
            try await session.required(TWIP_REPLACE, Replace(query: query, with: replacement), as: FindResult.self)
        }

        /// Replaces every match as one undo step.
        /// - Returns: How many were replaced.
        @discardableResult
        public func replaceAll(_ query: FindQuery, with replacement: String) async throws -> Int {
            try await session.call(TWIP_REPLACE_ALL, Replace(query: query, with: replacement), as: Count.self)?.count ?? 0
        }
    }

    /// Spell checking.
    public struct Spelling: Sendable {
        let session: EditorSession

        private struct UserWords: Encodable, Sendable { let language: String; let words: [String] }
        private struct Suggestions: Encodable, Sendable { let word: String; let language: String? }

        /// Loads a Hunspell dictionary for its language.
        public func addDictionary(_ dictionary: SpellingDictionary) async throws {
            try await session.call(TWIP_SPELLING_ADD_DICTIONARY, dictionary)
        }

        /// Turns checking on or off.
        public func setEnabled(_ enabled: Bool) async throws {
            try await session.call(TWIP_SPELLING_SET_ENABLED, enabled)
        }

        /// Sets the language text with no language of its own is checked in (BCP 47, `en-US`).
        public func setLanguage(_ language: String) async throws {
            try await session.call(TWIP_SPELLING_SET_LANGUAGE, language)
        }

        /// Ignores a word for the rest of the session.
        public func ignore(_ word: String) async throws {
            try await session.call(TWIP_SPELLING_IGNORE, word)
        }

        /// Adds a word to the user's dictionary; the session's events then carry `wordAdded` to persist it.
        public func add(_ word: String) async throws {
            try await session.call(TWIP_SPELLING_ADD, word)
        }

        /// Loads words the user added earlier, for a language.
        public func loadUserWords(_ words: [String], language: String) async throws {
            try await session.call(TWIP_SPELLING_LOAD_USER_WORDS, UserWords(language: language, words: words))
        }

        /// Replacements for a word, best first.
        public func suggestions(for word: String, language: String? = nil) async throws -> [String] {
            try await session.call(TWIP_SPELLING_SUGGESTIONS, Suggestions(word: word, language: language), as: [String].self) ?? []
        }

        /// Selects the next misspelling after the selection, or returns nil when there is none.
        public func next() async throws -> Misspelling? {
            try await session.call(TWIP_SPELLING_NEXT, as: Misspelling.self)
        }
    }

    /// Headings, for a navigation pane.
    public struct Navigation: Sendable {
        let session: EditorSession

        /// The document's headings, in order.
        public func headings() async throws -> [Heading] {
            try await session.call(TWIP_HEADINGS, as: [Heading].self) ?? []
        }

        /// Moves the selection to a heading and scrolls to it.
        public func select(_ heading: HeadingID) async throws {
            try await session.call(TWIP_SELECT_HEADING, heading)
        }
    }

    /// The document's styles.
    public struct Styles: Sendable {
        let session: EditorSession

        private struct Tile: Encodable, Sendable { let id: StyleID; let size: PreviewSize }

        /// The styles, of one type or all.
        public func list(_ type: StyleType? = nil) async throws -> [StyleInfo] {
            try await session.call(TWIP_STYLES, type, as: [StyleInfo].self) ?? []
        }

        /// A style's gallery tile.
        public func tile(_ id: StyleID, size: PreviewSize) async throws -> PreviewPicture {
            try await session.required(TWIP_STYLE_TILE, Tile(id: id, size: size), as: PreviewPicture.self)
        }
    }

    /// The list galleries, and the characters of fonts for a symbol picker.
    public struct Lists: Sendable {
        let session: EditorSession

        private struct Preview: Encodable, Sendable { let id: String; let size: PreviewSize }
        private struct Character: Encodable, Sendable { let family: String; let character: String; let size: PreviewSize }

        /// A gallery's sections and entries.
        public func galleries(_ kind: ListGalleryKind) async throws -> [ListGallerySection] {
            try await session.call(TWIP_LIST_GALLERIES, kind, as: [ListGallerySection].self) ?? []
        }

        /// A gallery entry's preview.
        public func preview(_ entry: String, size: PreviewSize) async throws -> PreviewPicture {
            try await session.required(TWIP_LIST_ENTRY_PREVIEW, Preview(id: entry, size: size), as: PreviewPicture.self)
        }

        /// The characters a font family has.
        public func characters(of family: String) async throws -> [CharRange] {
            try await session.call(TWIP_FONT_CHARACTERS, family, as: [CharRange].self) ?? []
        }

        /// One character drawn in a font.
        public func picture(of character: String, family: String, size: PreviewSize) async throws -> PreviewPicture {
            try await session.required(TWIP_CHARACTER_PICTURE, Character(family: family, character: character, size: size), as: PreviewPicture.self)
        }
    }

    /// Revisions, comments and the people who made them. Accept, reject and comment with commands.
    public struct Review: Sendable {
        let session: EditorSession

        /// The tracked revisions, in document order.
        public func revisions() async throws -> [RevisionInfo] {
            try await session.call(TWIP_REVISIONS, as: [RevisionInfo].self) ?? []
        }

        /// The comment threads, in document order.
        public func comments() async throws -> [CommentThread] {
            try await session.call(TWIP_COMMENTS, as: [CommentThread].self) ?? []
        }

        /// The people with revisions or comments.
        public func people() async throws -> [Person] {
            try await session.call(TWIP_PEOPLE, as: [Person].self) ?? []
        }
    }

    /// Merge-designer elements: fields and content controls.
    public struct Elements: Sendable {
        let session: EditorSession

        private struct Structure: Encodable, Sendable { let root: ElementID?; let depth: Int? }
        private struct Select: Encodable, Sendable { let id: ElementID; let reveal: Bool }

        /// The elements, all or those a filter matches, in document order.
        public func list(_ filter: ElementFilter? = nil) async throws -> [ElementInfo] {
            try await session.call(TWIP_ELEMENTS, filter, as: [ElementInfo].self) ?? []
        }

        /// One element's detail.
        public func detail(_ id: ElementID) async throws -> ElementDetail {
            try await session.required(TWIP_ELEMENT, id, as: ElementDetail.self)
        }

        /// The element's boxes on screen, one per line it spans; none while it's out of view or unpresented.
        public func boxes(_ id: ElementID) async throws -> [ClientRect] {
            try await session.call(TWIP_ELEMENT_BOXES, id, as: [ClientRect].self) ?? []
        }

        /// The document's block structure, or an element's, to a depth.
        public func structure(root: ElementID? = nil, depth: Int? = nil) async throws -> [Block] {
            try await session.call(TWIP_STRUCTURE, Structure(root: root, depth: depth), as: [Block].self) ?? []
        }

        /// Selects a whole element, scrolling to it unless `reveal` is false.
        public func select(_ id: ElementID, reveal: Bool = true) async throws {
            try await session.call(TWIP_SELECT_ELEMENT, Select(id: id, reveal: reveal))
        }
    }

    /// The template's sub-templates. Edit, create, rename and remove them with commands (`.editSubTemplate`, ...).
    public struct SubTemplates: Sendable {
        let session: EditorSession

        private struct Export: Encodable, Sendable { let name: String; let category: String? }

        /// The sub-templates the template's rules name.
        public func list() async throws -> [SubTemplateInfo] {
            try await session.call(TWIP_SUB_TEMPLATES, as: [SubTemplateInfo].self) ?? []
        }

        /// A standalone DOCX of a sub-template's body.
        public func export(_ name: String, category: String? = nil) async throws -> Data {
            try await session.required(TWIP_SUB_TEMPLATE_EXPORT, Export(name: name, category: category), as: Data.self)
        }
    }

    /// Custom XML data parts. Write them with the `.writeDataPart` command.
    public struct DataParts: Sendable {
        let session: EditorSession

        private struct Request: Encodable, Sendable {
            let namespace: String?
            let itemID: String?
            enum CodingKeys: String, CodingKey { case namespace, itemID = "itemId" }
        }

        /// The data parts, all or those in a namespace or with an item id.
        public func list(namespace: String? = nil, itemID: String? = nil) async throws -> [DataPart] {
            try await session.call(TWIP_DATA_PARTS, Request(namespace: namespace, itemID: itemID), as: [DataPart].self) ?? []
        }
    }

    /// The accessible structure and announcements.
    public struct Accessibility: Sendable {
        let session: EditorSession

        /// The document's accessible tree, for a screen reader.
        public func tree() async throws -> AccessibleNode {
            try await session.required(TWIP_ACCESSIBLE_TREE, as: AccessibleNode.self)
        }

        /// Moves the caret to the start of a node's content.
        public func go(to node: AccessibleID) async throws {
            try await session.call(TWIP_ACCESSIBLE_GO, node)
        }

        /// Sets the kinds of announcement the editor makes.
        public func setAnnouncements(_ kinds: [AnnouncementKind]) async throws {
            try await session.call(TWIP_ANNOUNCEMENTS_SET, kinds)
        }
    }

    /// Application key shortcuts: pressing one emits `shortcut` instead of editing.
    public struct Shortcuts: Sendable {
        let session: EditorSession

        /// Registers a shortcut, replacing one with the same id.
        public func register(_ shortcut: Shortcut) async throws {
            try await session.call(TWIP_SHORTCUTS_REGISTER, shortcut)
        }

        /// Removes a shortcut.
        public func unregister(_ id: String) async throws {
            try await session.call(TWIP_SHORTCUTS_UNREGISTER, id)
        }

        /// What a key chord (`"Ctrl+B"`) does: a command, an application shortcut, or nil.
        public func lookup(_ keys: String) async throws -> KeyBinding? {
            try await session.call(TWIP_SHORTCUTS_LOOKUP, keys, as: KeyBinding.self)
        }
    }

    /// Pointer gesture bindings.
    public struct Gestures: Sendable {
        let session: EditorSession

        private struct Assign: Encodable, Sendable { let gesture: GestureID; let action: GestureAction }

        /// Every gesture and its action.
        public func bindings() async throws -> [GestureBinding] {
            try await session.call(TWIP_GESTURES_BINDINGS, as: [GestureBinding].self) ?? []
        }

        /// Binds a gesture to an action.
        public func set(_ gesture: GestureID, to action: GestureAction) async throws {
            try await session.call(TWIP_GESTURES_SET, Assign(gesture: gesture, action: action))
        }

        /// Puts one gesture's binding back, or every one.
        public func reset(_ gesture: GestureID? = nil) async throws {
            try await session.call(TWIP_GESTURES_RESET, gesture)
        }
    }

    /// The clipboard's content as the engine reads and writes it.
    public struct Clipboard: Sendable {
        let session: EditorSession

        private struct Copy: Encodable, Sendable { let cut: Bool }
        private struct Paste: Encodable, Sendable { let data: ClipboardData; let asText: Bool }

        /// The selection as clipboard data (and cuts it, with `cut`); nil for a caret.
        public func copy(cut: Bool = false) async throws -> ClipboardData? {
            try await session.call(TWIP_COPY, Copy(cut: cut), as: ClipboardData.self)
        }

        /// Pastes clipboard data at the selection, or only its text.
        public func paste(_ data: ClipboardData, asText: Bool = false) async throws {
            try await session.call(TWIP_PASTE, Paste(data: data, asText: asText))
        }
    }

    /// A call whose result must be there.
    func required<T: Decodable & Sendable>(_ method: TwipMethod, _ request: some Encodable & Sendable, as: T.Type) async throws -> T {
        guard let value = try await call(method, request, as: T.self) else { throw TwipError(.internal, "the engine returned nothing") }
        return value
    }

    /// A call with no request whose result must be there.
    func required<T: Decodable & Sendable>(_ method: TwipMethod, as: T.Type) async throws -> T {
        guard let value = try await call(method, as: T.self) else { throw TwipError(.internal, "the engine returned nothing") }
        return value
    }
}
