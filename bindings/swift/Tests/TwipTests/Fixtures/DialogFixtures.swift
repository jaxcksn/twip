@testable import Twip

extension Fixtures {
    static let menusAndDialogs: [Fixture] = [
        fixture("client-point", ClientPoint.self),
        fixture("client-rect", ClientRect.self),
        fixture("content-point", ContentPoint.self),
        fixture("context-menu", ContextMenuModel.self),
        fixture("menu-target", MenuTarget.self),
        fixture("menu-item", MenuItem.self),
        fixture("menu-action", MenuAction.self),
        fixture("menu-via", MenuVia.self),
        fixture("context-menu-event", ContextMenuEvent.self),
        fixture("dialog-kind", DialogKind.self),
        fixture("dialog-model", DialogModel.self),
        fixture("dialog-values", DialogValues.self),
        fixture("field-info", FieldInfo.self),
        fixture("field-kind", FieldKind.self),
        fixture("font-style", FontStyle.self),
        fixture("special-indent", SpecialIndent.self),
        fixture("apply-to", ApplyTo.self),
        fixture("tabs-action", TabsAction.self),
        fixture("preview-size", PreviewSize.self),
        fixture("preview-picture", PreviewPicture.self),
    ]
}
