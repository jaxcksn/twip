using System.ComponentModel;
using System.Windows.Forms;

namespace Twip.WinForms;

/// <summary>
/// A bindable projection of <see cref="EditorState"/> for toolbars and status bars. It updates on the control's UI
/// thread, raising <see cref="PropertyChanged"/> for each property that changed. The <c>CheckState</c> and <c>Can</c>
/// properties bind straight to a toolbar button's <c>CheckState</c> and <c>Enabled</c>.
/// </summary>
public sealed class DocumentEditorState : INotifyPropertyChanged
{
    private static readonly (string Name, Func<EditorState, object?> Get)[] Projections =
    [
        (nameof(Bold), s => s.Formatting.Bold),
        (nameof(Italic), s => s.Formatting.Italic),
        (nameof(Underline), s => s.Formatting.Underline),
        (nameof(BoldCheckState), s => Check(s.Formatting.Bold)),
        (nameof(ItalicCheckState), s => Check(s.Formatting.Italic)),
        (nameof(UnderlineCheckState), s => Check(s.Formatting.Underline)),
        (nameof(CanCut), s => Enabled(s, CommandId.Cut)),
        (nameof(CanCopy), s => Enabled(s, CommandId.Copy)),
        (nameof(CanPaste), s => Enabled(s, CommandId.Paste)),
        (nameof(CanSetBold), s => Enabled(s, CommandId.SetBold)),
        (nameof(CanSetItalic), s => Enabled(s, CommandId.SetItalic)),
        (nameof(CanSetUnderline), s => Enabled(s, CommandId.SetUnderline)),
        (nameof(CanSetFont), s => Enabled(s, CommandId.SetFont)),
        (nameof(CanSetFontSize), s => Enabled(s, CommandId.SetFontSize)),
        (nameof(CanSetAlignment), s => Enabled(s, CommandId.SetAlignment)),
        (nameof(CanSetTrackChanges), s => Enabled(s, CommandId.SetTrackChanges)),
        (nameof(CanAddComment), s => Enabled(s, CommandId.AddComment)),
        (nameof(FontName), s => s.Formatting.Font),
        (nameof(FontSize), s => s.Formatting.Size),
        (nameof(Alignment), s => s.Paragraph.Alignment),
        (nameof(Style), s => s.Paragraph.Style),
        (nameof(CanUndo), s => s.History.CanUndo),
        (nameof(CanRedo), s => s.History.CanRedo),
        (nameof(UndoLabel), s => s.History.UndoLabel),
        (nameof(RedoLabel), s => s.History.RedoLabel),
        (nameof(PageCount), s => s.Pages.Count),
        (nameof(CurrentPage), s => s.Pages.Current),
        (nameof(Zoom), s => s.View.Zoom),
        (nameof(Modified), s => s.Modified),
        (nameof(ReadOnly), s => s.ReadOnly),
        (nameof(TrackChanges), s => s.Review.Tracking),
        (nameof(MarkupView), s => s.Review.MarkupView),
    ];

    internal DocumentEditorState(EditorState snapshot) => Snapshot = snapshot;

    /// <summary>A property changed.</summary>
    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>The full snapshot these properties come from.</summary>
    public EditorState Snapshot { get; private set; }

    /// <summary>Bold at the selection.</summary>
    public FormatValue<bool> Bold => Snapshot.Formatting.Bold;

    /// <summary>Italic at the selection.</summary>
    public FormatValue<bool> Italic => Snapshot.Formatting.Italic;

    /// <summary>The underline style at the selection.</summary>
    public FormatValue<UnderlineStyle> Underline => Snapshot.Formatting.Underline;

    /// <summary>Bold at the selection, for a toggle button: <see cref="CheckState.Indeterminate"/> where it is mixed.</summary>
    public CheckState BoldCheckState => Check(Bold);

    /// <summary>Italic at the selection, for a toggle button: <see cref="CheckState.Indeterminate"/> where it is mixed.</summary>
    public CheckState ItalicCheckState => Check(Italic);

    /// <summary>
    /// Whether the selection is underlined, for a toggle button: <see cref="CheckState.Checked"/> for any style but
    /// none, <see cref="CheckState.Indeterminate"/> where it is mixed.
    /// </summary>
    public CheckState UnderlineCheckState => Check(Underline);

    /// <summary>The font family at the selection.</summary>
    public FormatValue<string> FontName => Snapshot.Formatting.Font;

    /// <summary>The font size at the selection.</summary>
    public FormatValue<Length> FontSize => Snapshot.Formatting.Size;

    /// <summary>The paragraph alignment at the selection.</summary>
    public FormatValue<ParagraphAlignment> Alignment => Snapshot.Paragraph.Alignment;

    /// <summary>The paragraph style at the selection.</summary>
    public FormatValue<StyleId> Style => Snapshot.Paragraph.Style;

    /// <summary>Whether there is something to undo.</summary>
    public bool CanUndo => Snapshot.History.CanUndo;

    /// <summary>Whether there is something to redo.</summary>
    public bool CanRedo => Snapshot.History.CanRedo;

    /// <summary>The label of the step undo would undo.</summary>
    public string? UndoLabel => Snapshot.History.UndoLabel;

    /// <summary>The label of the step redo would redo.</summary>
    public string? RedoLabel => Snapshot.History.RedoLabel;

    /// <summary>The number of pages.</summary>
    public int PageCount => Snapshot.Pages.Count;

    /// <summary>The caret's page.</summary>
    public int CurrentPage => Snapshot.Pages.Current;

    /// <summary>The zoom percentage.</summary>
    public double Zoom => Snapshot.View.Zoom;

    /// <summary>Whether the document has unsaved changes.</summary>
    public bool Modified => Snapshot.Modified;

    /// <summary>Whether the editor is read only.</summary>
    public bool ReadOnly => Snapshot.ReadOnly;

    /// <summary>Whether changes are tracked.</summary>
    public bool TrackChanges => Snapshot.Review.Tracking;

    /// <summary>How markup shows.</summary>
    public MarkupView MarkupView => Snapshot.Review.MarkupView;

    /// <summary>Whether <see cref="CommandId.Cut"/> can run now.</summary>
    public bool CanCut => IsEnabled(CommandId.Cut);

    /// <summary>Whether <see cref="CommandId.Copy"/> can run now.</summary>
    public bool CanCopy => IsEnabled(CommandId.Copy);

    /// <summary>Whether <see cref="CommandId.Paste"/> can run now.</summary>
    public bool CanPaste => IsEnabled(CommandId.Paste);

    /// <summary>Whether <see cref="CommandId.SetBold"/> can run now.</summary>
    public bool CanSetBold => IsEnabled(CommandId.SetBold);

    /// <summary>Whether <see cref="CommandId.SetItalic"/> can run now.</summary>
    public bool CanSetItalic => IsEnabled(CommandId.SetItalic);

    /// <summary>Whether <see cref="CommandId.SetUnderline"/> can run now.</summary>
    public bool CanSetUnderline => IsEnabled(CommandId.SetUnderline);

    /// <summary>Whether <see cref="CommandId.SetFont"/> can run now.</summary>
    public bool CanSetFont => IsEnabled(CommandId.SetFont);

    /// <summary>Whether <see cref="CommandId.SetFontSize"/> can run now.</summary>
    public bool CanSetFontSize => IsEnabled(CommandId.SetFontSize);

    /// <summary>Whether <see cref="CommandId.SetAlignment"/> can run now.</summary>
    public bool CanSetAlignment => IsEnabled(CommandId.SetAlignment);

    /// <summary>Whether <see cref="CommandId.SetTrackChanges"/> can run now.</summary>
    public bool CanSetTrackChanges => IsEnabled(CommandId.SetTrackChanges);

    /// <summary>Whether <see cref="CommandId.AddComment"/> can run now.</summary>
    public bool CanAddComment => IsEnabled(CommandId.AddComment);

    /// <summary>Whether a command can run now.</summary>
    /// <param name="command">The command.</param>
    /// <returns>True when it is enabled.</returns>
    public bool IsEnabled(CommandId command) => Enabled(Snapshot, command);

    private static bool Enabled(EditorState s, CommandId command) => s.Commands.TryGetValue(command, out var state) && state.Enabled;

    private static CheckState Check(FormatValue<bool> value) =>
        value.IsMixed ? CheckState.Indeterminate : value.IsOn ? CheckState.Checked : CheckState.Unchecked;

    private static CheckState Check(FormatValue<UnderlineStyle> value) =>
        value.IsMixed ? CheckState.Indeterminate
        : value.IsUniform && value.Value != UnderlineStyle.None ? CheckState.Checked
        : CheckState.Unchecked;

    internal void Update(EditorState next)
    {
        var old = Snapshot;
        if (ReferenceEquals(old, next)) return;
        Snapshot = next;
        foreach (var (name, get) in Projections)
        {
            if (!Equals(get(old), get(next))) PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
        }
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Snapshot)));
    }
}
