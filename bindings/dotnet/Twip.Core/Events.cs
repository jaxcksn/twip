using System.Drawing;

namespace Twip;

/// <summary>The base of the session's event arguments: each carries the exact snapshot of its revision.</summary>
public abstract class EditorEventArgs : EventArgs
{
    private protected EditorEventArgs(EditorState snapshot) => Snapshot = snapshot;

    /// <summary>The state at the time of the event. Don't reread <see cref="EditorSession.State"/>: it may be newer.</summary>
    public EditorState Snapshot { get; }
}

/// <summary>The state changed. Coalesced: a handler may skip intermediate snapshots.</summary>
public sealed class StateChangedEventArgs : EditorEventArgs
{
    internal StateChangedEventArgs(EditorState snapshot) : base(snapshot) { }

    /// <summary>The snapshot's state sequence.</summary>
    public long StateSequence => Snapshot.StateSequence;
}

/// <summary>A transaction committed. Exactly one per commit, in order, never coalesced.</summary>
public sealed class DocumentChangedEventArgs : EditorEventArgs
{
    internal DocumentChangedEventArgs(TransactionRecord record, EditorState snapshot) : base(snapshot) => Record = record;

    /// <summary>The committed transaction.</summary>
    public TransactionRecord Record { get; }
}

/// <summary>The selection changed.</summary>
public sealed class SelectionChangedEventArgs : EditorEventArgs
{
    internal SelectionChangedEventArgs(SelectionSummary selection, EditorState snapshot) : base(snapshot) => Selection = selection;

    /// <summary>The new selection.</summary>
    public SelectionSummary Selection { get; }
}

/// <summary>How a link was activated.</summary>
public enum LinkActivation
{
    /// <summary>Ctrl+click while editing.</summary>
    CtrlClick,
    /// <summary>A click while read only.</summary>
    Click,
}

/// <summary>The user activated a hyperlink. The application decides whether to open it.</summary>
public sealed class LinkActivatedEventArgs : EditorEventArgs
{
    internal LinkActivatedEventArgs(string address, LinkActivation via, EditorState snapshot) : base(snapshot) => (Address, Via) = (address, via);

    /// <summary>The link address.</summary>
    public string Address { get; }

    /// <summary>How it was activated.</summary>
    public LinkActivation Via { get; }
}

/// <summary>The user clicked an unfilled picture placeholder.</summary>
public sealed class PlaceholderActivatedEventArgs : EditorEventArgs
{
    internal PlaceholderActivatedEventArgs(string key, ElementId? element, EditorState snapshot) : base(snapshot) => (Key, Element) = (key, element);

    /// <summary>The placeholder key.</summary>
    public string Key { get; }

    /// <summary>The placeholder's element, when it is one.</summary>
    public ElementId? Element { get; }
}

/// <summary>How a context menu was opened.</summary>
public enum MenuInvocation
{
    /// <summary>A right-click.</summary>
    Pointer,
    /// <summary>The context-menu key or Shift+F10.</summary>
    Keyboard,
}

/// <summary>The user asked for a context menu. Show your own menu at <see cref="Location"/>.</summary>
public sealed class ContextMenuRequestedEventArgs : EditorEventArgs
{
    internal ContextMenuRequestedEventArgs(ContextMenu menu, PointF location, MenuInvocation via, EditorState snapshot) : base(snapshot) =>
        (Menu, Location, Via) = (menu, location, via);

    /// <summary>The menu model.</summary>
    public ContextMenu Menu { get; }

    /// <summary>Where to show it, in control client pixels.</summary>
    public PointF Location { get; }

    /// <summary>How it was opened.</summary>
    public MenuInvocation Via { get; }
}

/// <summary>A dialog.</summary>
public enum DialogKind
{
    /// <summary>Font.</summary>
    Font,
    /// <summary>Paragraph.</summary>
    Paragraph,
    /// <summary>Page setup.</summary>
    PageSetup,
    /// <summary>Insert table.</summary>
    InsertTable,
    /// <summary>Table properties.</summary>
    TableProperties,
    /// <summary>Picture size.</summary>
    PictureSize,
    /// <summary>Alternative text.</summary>
    AltText,
    /// <summary>Hyperlink.</summary>
    Hyperlink,
    /// <summary>Tabs.</summary>
    Tabs,
    /// <summary>The accessibility check.</summary>
    Accessibility,
}

/// <summary>The user asked for a dialog (Ctrl+D, a double-click on the ruler). Open it through <see cref="EditorSession.Dialogs"/>.</summary>
public sealed class DialogRequestedEventArgs : EditorEventArgs
{
    internal DialogRequestedEventArgs(DialogKind kind, EditorState snapshot) : base(snapshot) => Kind = kind;

    /// <summary>The dialog.</summary>
    public DialogKind Kind { get; }
}

/// <summary>A key the application owns.</summary>
public enum HostCommand
{
    /// <summary>Save (Ctrl+S).</summary>
    Save,
    /// <summary>Save as (F12).</summary>
    SaveAs,
    /// <summary>Print (Ctrl+P).</summary>
    Print,
    /// <summary>Open (Ctrl+O).</summary>
    Open,
    /// <summary>New (Ctrl+N).</summary>
    New,
    /// <summary>Find (Ctrl+F).</summary>
    Find,
    /// <summary>Replace (Ctrl+H).</summary>
    Replace,
    /// <summary>Go to (Ctrl+G).</summary>
    GoTo,
    /// <summary>Insert hyperlink (Ctrl+K).</summary>
    InsertHyperlink,
    /// <summary>Next misspelling (F7).</summary>
    NextMisspelling,
    /// <summary>Focus the font box (Ctrl+Shift+F).</summary>
    FocusFont,
    /// <summary>Focus the font size box (Ctrl+Shift+P).</summary>
    FocusFontSize,
    /// <summary>Help (F1).</summary>
    Help,
}

/// <summary>The user pressed a key the application owns.</summary>
public sealed class HostCommandEventArgs : EditorEventArgs
{
    internal HostCommandEventArgs(HostCommand command, EditorState snapshot) : base(snapshot) => Command = command;

    /// <summary>The command.</summary>
    public HostCommand Command { get; }
}

/// <summary>The user pressed an application shortcut from <see cref="EditorShortcuts.Register"/>.</summary>
public sealed class ShortcutEventArgs : EditorEventArgs
{
    internal ShortcutEventArgs(string id, EditorState snapshot) : base(snapshot) => Id = id;

    /// <summary>The shortcut's id.</summary>
    public string Id { get; }
}

/// <summary>A gesture bound to <see cref="GestureAction.Emit"/> happened.</summary>
public sealed class GestureEventArgs : EditorEventArgs
{
    internal GestureEventArgs(GestureId gesture, PointF location, int page, GestureTarget target, EditorState snapshot) : base(snapshot) =>
        (Gesture, Location, Page, Target) = (gesture, location, page, target);

    /// <summary>The gesture.</summary>
    public GestureId Gesture { get; }

    /// <summary>Where, in control client pixels.</summary>
    public PointF Location { get; }

    /// <summary>The page.</summary>
    public int Page { get; }

    /// <summary>What is under the point.</summary>
    public GestureTarget Target { get; }
}

/// <summary>The user added a word to the dictionary: persist it.</summary>
public sealed class WordAddedEventArgs : EditorEventArgs
{
    internal WordAddedEventArgs(string word, string language, EditorState snapshot) : base(snapshot) => (Word, Language) = (word, language);

    /// <summary>The word.</summary>
    public string Word { get; }

    /// <summary>Its language.</summary>
    public string Language { get; }
}

/// <summary>Fonts arrived or were found missing.</summary>
public sealed class FontsChangedEventArgs : EditorEventArgs
{
    internal FontsChangedEventArgs(IReadOnlyList<string> pending, IReadOnlyList<string> unresolved, EditorState snapshot) : base(snapshot) =>
        (Pending, Unresolved) = (pending, unresolved);

    /// <summary>Families being resolved.</summary>
    public IReadOnlyList<string> Pending { get; }

    /// <summary>Families that couldn't be found.</summary>
    public IReadOnlyList<string> Unresolved { get; }
}

/// <summary>In design mode, the user activated an element (Enter or a double-click).</summary>
public sealed class ElementActivatedEventArgs : EditorEventArgs
{
    internal ElementActivatedEventArgs(ElementId id, EditorState snapshot) : base(snapshot) => Id = id;

    /// <summary>The element.</summary>
    public ElementId Id { get; }
}

/// <summary>Elements were added, removed or changed.</summary>
public sealed class ElementsChangedEventArgs : EditorEventArgs
{
    internal ElementsChangedEventArgs(IReadOnlyList<ElementInfo> added, IReadOnlyList<ElementId> removed, IReadOnlyList<ElementInfo> changed, EditorState snapshot) : base(snapshot) =>
        (Added, Removed, Changed) = (added, removed, changed);

    /// <summary>Added elements.</summary>
    public IReadOnlyList<ElementInfo> Added { get; }

    /// <summary>Removed elements.</summary>
    public IReadOnlyList<ElementId> Removed { get; }

    /// <summary>Changed elements.</summary>
    public IReadOnlyList<ElementInfo> Changed { get; }
}

/// <summary>A field the application computes, to update.</summary>
/// <param name="Id">The field.</param>
/// <param name="Code">Its code.</param>
public sealed record FieldUpdateRequest(ElementId Id, string Code);

/// <summary>Fields need results the application computes. Answer with <see cref="EditorCommands.SetFieldResultsAsync"/> and the token.</summary>
public sealed class FieldsUpdateRequestedEventArgs : EditorEventArgs
{
    internal FieldsUpdateRequestedEventArgs(long token, IReadOnlyList<FieldUpdateRequest> fields, EditorState snapshot) : base(snapshot) =>
        (Token, Fields) = (token, fields);

    /// <summary>The token that joins the results to the update step.</summary>
    public long Token { get; }

    /// <summary>The fields.</summary>
    public IReadOnlyList<FieldUpdateRequest> Fields { get; }
}

/// <summary>The visible comment anchors moved (layout, zoom or scroll). Attached only.</summary>
public sealed class CommentAnchorsChangedEventArgs : EditorEventArgs
{
    internal CommentAnchorsChangedEventArgs(IReadOnlyList<CommentAnchor> anchors, EditorState snapshot) : base(snapshot) => Anchors = anchors;

    /// <summary>The anchors, in control client pixels.</summary>
    public IReadOnlyList<CommentAnchor> Anchors { get; }
}

/// <summary>The ruler view changed. Attached only.</summary>
public sealed class RulerChangedEventArgs : EditorEventArgs
{
    internal RulerChangedEventArgs(RulerView view, EditorState snapshot) : base(snapshot) => View = view;

    /// <summary>The ruler model with its viewport mapping.</summary>
    public RulerView View { get; }
}

/// <summary>A tooltip should show or hide. Attached only.</summary>
public sealed class TooltipEventArgs : EditorEventArgs
{
    internal TooltipEventArgs(TooltipInfo? tooltip, EditorState snapshot) : base(snapshot) => Tooltip = tooltip;

    /// <summary>The tooltip, or null to hide it.</summary>
    public TooltipInfo? Tooltip { get; }
}

/// <summary>The editor said something to screen readers.</summary>
public sealed class AnnouncementEventArgs : EditorEventArgs
{
    internal AnnouncementEventArgs(string text, EditorState snapshot) : base(snapshot) => Text = text;

    /// <summary>What was said.</summary>
    public string Text { get; }
}

/// <summary>An import, export or font approximation.</summary>
public sealed class WarningEventArgs : EditorEventArgs
{
    internal WarningEventArgs(Warning warning, EditorState snapshot) : base(snapshot) => Warning = warning;

    /// <summary>The warning.</summary>
    public Warning Warning { get; }
}

/// <summary>A fault: a fatal engine failure, or an exception from an event handler.</summary>
public sealed class FaultedEventArgs : EditorEventArgs
{
    internal FaultedEventArgs(Exception exception, bool isFatal, EditorState snapshot) : base(snapshot) =>
        (Exception, IsFatal) = (exception, isFatal);

    /// <summary>The exception.</summary>
    public Exception Exception { get; }

    /// <summary>Whether the session is unusable: pending and new work fails, and it can only be disposed.</summary>
    public bool IsFatal { get; }
}
