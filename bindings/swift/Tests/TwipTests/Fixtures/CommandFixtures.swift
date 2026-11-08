@testable import Twip

extension Fixtures {
    /// The fixtures of the types in crates/twip/src/api/command.rs.
    static let commands: [Fixture] = [
        fixture("command", Command.self),
        fixture("command-id", CommandID.self),
        fixture("command-result", CommandResult.self),
        fixture("zoom", Zoom.self),
        fixture("zoom-fit", ZoomFit.self),
        fixture("auto-fit", AutoFit.self),
        fixture("border-edges", BorderEdges.self),
        fixture("border-preset", BorderPreset.self),
        fixture("border-style", BorderStyle.self),
        fixture("case-change", CaseChange.self),
        fixture("cell-shift", CellShift.self),
        fixture("delete-unit", DeleteUnit.self),
        fixture("element-class", ElementClass.self),
        fixture("element-field", ElementField.self),
        fixture("horizontal-side", HorizontalSide.self),
        fixture("level-change", LevelChange.self),
        fixture("list-kind", ListKind.self),
        fixture("move-unit", MoveUnit.self),
        fixture("page-number-format", PageNumberFormat.self),
        fixture("page-number-place", PageNumberPlace.self),
        fixture("paste-as", PasteAs.self),
        fixture("replace-scope", ReplaceScope.self),
        fixture("section-break-kind", SectionBreakKind.self),
        fixture("select-unit", SelectUnit.self),
        fixture("selection-mode", SelectionMode.self),
        fixture("severity", Severity.self),
        fixture("vertical-direction", VerticalDirection.self),
        fixture("vertical-side", VerticalSide.self),
    ]
}
