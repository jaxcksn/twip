@testable import Twip

extension Fixtures {
    static let state: [Fixture] = [
        // state.rs
        fixture("state", EditorState.self),
        fixture("command-state", CommandState.self),
        fixture("command-value", CommandValue.self),
        fixture("selection-summary", SelectionSummary.self),
        fixture("character-formatting", CharacterFormatting.self),
        fixture("paragraph-formatting", ParagraphFormatting.self),
        fixture("page-formatting", PageFormatting.self),
        fixture("view-state", ViewState.self),
        fixture("review-state", ReviewState.self),
        fixture("missing-font", MissingFont.self),
        fixture("missing-characters", MissingCharacters.self),
        fixture("border-state", BorderState.self),
        fixture("element-kind", ElementKind.self),
        fixture("header-footer-kind", HeaderFooterKind.self),
        fixture("header-footer-type", HeaderFooterType.self),
        fixture("list-state", ListState.self),
        fixture("load-state", LoadState.self),
        fixture("mode", Mode.self),
        fixture("object-kind", ObjectKind.self),
        fixture("selection-kind", SelectionKind.self),
        fixture("stand-in", StandIn.self),
        // record.rs
        fixture("transaction-record", TransactionRecord.self),
        fixture("author", Author.self),
        fixture("identity", Identity.self),
        fixture("timestamp", Timestamp.self),
        fixture("change-summary", ChangeSummary.self),
        fixture("origin", Origin.self),
        // review.rs
        fixture("revision-info", RevisionInfo.self),
        fixture("comment-thread", CommentThread.self),
        fixture("person", Person.self),
        fixture("rich-text", RichText.self),
        fixture("body", TextBody.self),
        fixture("comment-anchor", CommentAnchor.self),
        fixture("balloon-placement", BalloonPlacement.self),
        fixture("markup-view", MarkupView.self),
        fixture("revision-kind", RevisionKind.self),
        // event.rs
        fixture("event", EditorEvent.self),
        fixture("key-outcome", KeyOutcome.self),
        fixture("host-command", HostCommand.self),
        fixture("link-via", LinkVia.self),
        // catalog.rs
        fixture("command-info", CommandInfo.self),
        fixture("command-group", CommandGroup.self),
        fixture("undo-grouping", UndoGrouping.self),
    ]
}
