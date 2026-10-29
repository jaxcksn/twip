using System.Drawing;
using System.Text.Json.Serialization;

namespace Twip
{
    /// <summary>One axis of a ruler's viewport mapping.</summary>
    /// <param name="Origin">The client coordinate of the page's edge, in pixels.</param>
    /// <param name="Scale">Pixels per point.</param>
    public sealed record RulerAxis(double Origin, double Scale);

    /// <summary>The ruler model with its mapping to the viewport, for an application drawing its own ruler.</summary>
    /// <param name="Model">The model, in points.</param>
    /// <param name="Page">The page the rulers follow (the caret's).</param>
    /// <param name="Horizontal">The horizontal mapping.</param>
    /// <param name="Vertical">The vertical mapping.</param>
    /// <param name="Unit">The unit.</param>
    public sealed record RulerView(RulerModel Model, int Page, RulerAxis Horizontal, RulerAxis Vertical, RulerUnit Unit);

    /// <summary>What a tooltip is for.</summary>
    public enum TooltipKind
    {
        /// <summary>A hyperlink.</summary>
        Link,
        /// <summary>An element mark.</summary>
        Mark,
        /// <summary>A revision.</summary>
        Revision,
        /// <summary>A comment.</summary>
        Comment,
    }

    /// <summary>A tooltip to show.</summary>
    /// <param name="Kind">What it is for.</param>
    /// <param name="Text">Its text.</param>
    /// <param name="Location">Where to show it, in control client pixels.</param>
    /// <param name="Bounds">The hovered thing's bounds, in control client pixels.</param>
    public sealed record TooltipInfo(TooltipKind Kind, string Text, PointF Location, RectangleF Bounds);

    /// <summary>What is under a point.</summary>
    /// <param name="Element">The element, if any.</param>
    /// <param name="Editable">Whether the point is in editable content.</param>
    public sealed record HitResult(ElementId? Element, bool Editable);

    /// <summary>A visible page.</summary>
    /// <param name="Page">The page number.</param>
    /// <param name="Bounds">Its bounds, in control client pixels.</param>
    public sealed record PageRect(int Page, [property: JsonPropertyName("rect")] RectangleF Bounds);
}

namespace Twip.Hosting
{
    /// <summary>A UI thread's dispatcher, supplied by a host adapter.</summary>
    public interface IEditorDispatcher
    {
        /// <summary>Runs an action on the UI thread, later and in order.</summary>
        /// <param name="action">The action.</param>
        void Post(Action action);

        /// <summary>Whether the calling thread is the UI thread.</summary>
        /// <returns>True on the UI thread.</returns>
        bool CheckAccess();
    }

    /// <summary>A CPU frame to present: 32-bit pixels, top-down rows.</summary>
    public readonly ref struct FrameView
    {
        internal FrameView(ReadOnlySpan<byte> pixels, int width, int height, int stride)
        {
            Pixels = pixels;
            Width = width;
            Height = height;
            Stride = stride;
        }

        /// <summary>The pixels: RGBA, 8 bits per channel, premultiplied alpha.</summary>
        public ReadOnlySpan<byte> Pixels { get; }

        /// <summary>The width in pixels.</summary>
        public int Width { get; }

        /// <summary>The height in pixels.</summary>
        public int Height { get; }

        /// <summary>The bytes per row.</summary>
        public int Stride { get; }
    }

    /// <summary>A mouse cursor.</summary>
    public enum EditorCursor
    {
        /// <summary>The default arrow.</summary>
        Default,
        /// <summary>A text I-beam.</summary>
        Text,
        /// <summary>A hand, over links.</summary>
        Pointer,
        /// <summary>A move cross.</summary>
        Move,
        /// <summary>Resizing left and right.</summary>
        ResizeHorizontal,
        /// <summary>Resizing up and down.</summary>
        ResizeVertical,
        /// <summary>Resizing from the top-left or bottom-right corner.</summary>
        ResizeNorthWestSouthEast,
        /// <summary>Resizing from the top-right or bottom-left corner.</summary>
        ResizeNorthEastSouthWest,
        /// <summary>A crosshair.</summary>
        Crosshair,
        /// <summary>Not allowed.</summary>
        NotAllowed,
        /// <summary>Copying a dragged selection.</summary>
        Copy,
    }

    /// <summary>The caret, for the IME window and the system caret.</summary>
    /// <param name="Bounds">Its bounds, in control client pixels.</param>
    /// <param name="Visible">Whether it shows.</param>
    public sealed record CaretInfo(RectangleF Bounds, bool Visible);

    /// <summary>Clipboard content in the formats the engine reads and writes.</summary>
    /// <param name="Text">Plain text.</param>
    /// <param name="Html">HTML.</param>
    /// <param name="Native">A token naming the engine's own copy, for pastes between editors that keep everything.</param>
    public sealed record ClipboardContent(
        string? Text = null,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Html = null,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull), JsonPropertyName("internal")] string? Native = null);

    /// <summary>Modifier keys.</summary>
    [Flags]
    public enum KeyModifiers
    {
        /// <summary>No modifiers.</summary>
        None = 0,
        /// <summary>Control.</summary>
        Control = 1,
        /// <summary>Shift.</summary>
        Shift = 2,
        /// <summary>Alt (Option on macOS).</summary>
        Alt = 4,
        /// <summary>The Windows or Command key.</summary>
        Meta = 8,
    }

    /// <summary>A phase of IME composition.</summary>
    public enum CompositionPhase
    {
        /// <summary>Composition started.</summary>
        Start,
        /// <summary>The composition text changed.</summary>
        Update,
        /// <summary>The composition was committed.</summary>
        [JsonStringEnumMemberName("end")]
        Commit,
        /// <summary>The composition was cancelled.</summary>
        Cancel,
    }

    /// <summary>Raw pointer input. The engine recognises every gesture from it.</summary>
    public enum PointerKind
    {
        /// <summary>A button went down.</summary>
        Down,
        /// <summary>The pointer moved.</summary>
        Move,
        /// <summary>A button went up.</summary>
        Up,
        /// <summary>The platform cancelled the pointer (capture lost).</summary>
        Cancel,
        /// <summary>The pointer left the control.</summary>
        Leave,
    }

    /// <summary>The button a pointer event pressed or released.</summary>
    public enum PointerButton
    {
        /// <summary>The primary (usually left) button.</summary>
        Primary,
        /// <summary>The secondary (usually right) button: it opens the context menu.</summary>
        Secondary,
        /// <summary>The middle button or wheel.</summary>
        Middle,
    }

    /// <summary>
    /// A physical key, named as <c>KeyboardEvent.code</c> names it. Adapters translate platform key codes to these.
    /// </summary>
    public enum EditorKey
    {
        /// <summary>No key.</summary>
        None = 0,
        /// <summary>A.</summary>
        KeyA = 1,
        /// <summary>B.</summary>
        KeyB = 2,
        /// <summary>C.</summary>
        KeyC = 3,
        /// <summary>D.</summary>
        KeyD = 4,
        /// <summary>E.</summary>
        KeyE = 5,
        /// <summary>F.</summary>
        KeyF = 6,
        /// <summary>G.</summary>
        KeyG = 7,
        /// <summary>H.</summary>
        KeyH = 8,
        /// <summary>I.</summary>
        KeyI = 9,
        /// <summary>J.</summary>
        KeyJ = 10,
        /// <summary>K.</summary>
        KeyK = 11,
        /// <summary>L.</summary>
        KeyL = 12,
        /// <summary>M.</summary>
        KeyM = 13,
        /// <summary>N.</summary>
        KeyN = 14,
        /// <summary>O.</summary>
        KeyO = 15,
        /// <summary>P.</summary>
        KeyP = 16,
        /// <summary>Q.</summary>
        KeyQ = 17,
        /// <summary>R.</summary>
        KeyR = 18,
        /// <summary>S.</summary>
        KeyS = 19,
        /// <summary>T.</summary>
        KeyT = 20,
        /// <summary>U.</summary>
        KeyU = 21,
        /// <summary>V.</summary>
        KeyV = 22,
        /// <summary>W.</summary>
        KeyW = 23,
        /// <summary>X.</summary>
        KeyX = 24,
        /// <summary>Y.</summary>
        KeyY = 25,
        /// <summary>Z.</summary>
        KeyZ = 26,
        /// <summary>0.</summary>
        Digit0 = 27,
        /// <summary>1.</summary>
        Digit1 = 28,
        /// <summary>2.</summary>
        Digit2 = 29,
        /// <summary>3.</summary>
        Digit3 = 30,
        /// <summary>4.</summary>
        Digit4 = 31,
        /// <summary>5.</summary>
        Digit5 = 32,
        /// <summary>6.</summary>
        Digit6 = 33,
        /// <summary>7.</summary>
        Digit7 = 34,
        /// <summary>8.</summary>
        Digit8 = 35,
        /// <summary>9.</summary>
        Digit9 = 36,
        /// <summary>Enter.</summary>
        Enter = 61,
        /// <summary>Escape.</summary>
        Escape = 65,
        /// <summary>Backspace.</summary>
        Backspace = 63,
        /// <summary>Tab.</summary>
        Tab = 62,
        /// <summary>Space.</summary>
        Space = 66,
        /// <summary>Minus.</summary>
        Minus = 77,
        /// <summary>Equal.</summary>
        Equal = 78,
        /// <summary>Left bracket.</summary>
        BracketLeft = 79,
        /// <summary>Right bracket.</summary>
        BracketRight = 80,
        /// <summary>Backslash.</summary>
        Backslash = 81,
        /// <summary>Semicolon.</summary>
        Semicolon = 82,
        /// <summary>Quote.</summary>
        Quote = 83,
        /// <summary>Backquote.</summary>
        Backquote = 84,
        /// <summary>Comma.</summary>
        Comma = 85,
        /// <summary>Period.</summary>
        Period = 86,
        /// <summary>Slash.</summary>
        Slash = 87,
        /// <summary>F1.</summary>
        F1 = 37,
        /// <summary>F2.</summary>
        F2 = 38,
        /// <summary>F3.</summary>
        F3 = 39,
        /// <summary>F4.</summary>
        F4 = 40,
        /// <summary>F5.</summary>
        F5 = 41,
        /// <summary>F6.</summary>
        F6 = 42,
        /// <summary>F7.</summary>
        F7 = 43,
        /// <summary>F8.</summary>
        F8 = 44,
        /// <summary>F9.</summary>
        F9 = 45,
        /// <summary>F10.</summary>
        F10 = 46,
        /// <summary>F11.</summary>
        F11 = 47,
        /// <summary>F12.</summary>
        F12 = 48,
        /// <summary>Insert.</summary>
        Insert = 67,
        /// <summary>Delete.</summary>
        Delete = 64,
        /// <summary>Home.</summary>
        Home = 68,
        /// <summary>End.</summary>
        End = 69,
        /// <summary>Page Up.</summary>
        PageUp = 70,
        /// <summary>Page Down.</summary>
        PageDown = 71,
        /// <summary>Left arrow.</summary>
        ArrowLeft = 72,
        /// <summary>Right arrow.</summary>
        ArrowRight = 73,
        /// <summary>Up arrow.</summary>
        ArrowUp = 74,
        /// <summary>Down arrow.</summary>
        ArrowDown = 75,
        /// <summary>The context-menu key.</summary>
        ContextMenu = 76,
        /// <summary>Keypad 0.</summary>
        Numpad0 = 95,
        /// <summary>Keypad 1.</summary>
        Numpad1 = 96,
        /// <summary>Keypad 2.</summary>
        Numpad2 = 97,
        /// <summary>Keypad 3.</summary>
        Numpad3 = 98,
        /// <summary>Keypad 4.</summary>
        Numpad4 = 99,
        /// <summary>Keypad 5.</summary>
        Numpad5 = 100,
        /// <summary>Keypad 6.</summary>
        Numpad6 = 101,
        /// <summary>Keypad 7.</summary>
        Numpad7 = 102,
        /// <summary>Keypad 8.</summary>
        Numpad8 = 103,
        /// <summary>Keypad 9.</summary>
        Numpad9 = 104,
        /// <summary>Keypad plus.</summary>
        NumpadAdd = 90,
        /// <summary>Keypad minus.</summary>
        NumpadSubtract = 91,
        /// <summary>Keypad multiply.</summary>
        NumpadMultiply = 92,
        /// <summary>Keypad divide.</summary>
        NumpadDivide = 93,
        /// <summary>Keypad decimal.</summary>
        NumpadDecimal = 94,
        /// <summary>Keypad Enter.</summary>
        NumpadEnter = 89,
        /// <summary>The key between left Shift and Z on ISO keyboards.</summary>
        IntlBackslash = 88,
    }

    /// <summary>
    /// What a platform control implements to host an <see cref="EditorSession"/>: its UI thread, the presentation
    /// surface, and the OS services. It translates platform behaviour and never reimplements editing or layout.
    /// </summary>
    public interface IEditorHostAdapter
    {
        /// <summary>The UI thread.</summary>
        IEditorDispatcher Dispatcher { get; }

        /// <summary>Schedule a paint, which calls <see cref="EditorHost.Render"/>. Called on the UI thread.</summary>
        void RequestRender();

        /// <summary>Present the CPU frame for this paint, rulers included. Called inside <see cref="EditorHost.Render"/>.</summary>
        /// <param name="frame">The frame; valid only during the call.</param>
        void Present(in FrameView frame);

        /// <summary>Show a mouse cursor.</summary>
        /// <param name="cursor">The cursor.</param>
        void SetCursor(EditorCursor cursor);

        /// <summary>Capture the pointer, for a drag.</summary>
        void CapturePointer();

        /// <summary>Release the pointer capture.</summary>
        void ReleasePointer();

        /// <summary>Put content on the OS clipboard.</summary>
        /// <param name="content">The content.</param>
        void SetClipboard(ClipboardContent content);

        /// <summary>Read the OS clipboard.</summary>
        /// <returns>The content, or null when it holds nothing the engine reads.</returns>
        ValueTask<ClipboardContent?> GetClipboardAsync();

        /// <summary>Move the IME window and the system caret.</summary>
        /// <param name="caret">The caret.</param>
        void SetCaret(CaretInfo caret);

        /// <summary>Scroll the view so a content point is at the top left.</summary>
        /// <param name="x">The x, in pixels.</param>
        /// <param name="y">The y, in pixels.</param>
        void ScrollTo(double x, double y);

        /// <summary>Show or hide a tooltip.</summary>
        /// <param name="tooltip">The tooltip, or null to hide it.</param>
        void ShowTooltip(TooltipInfo? tooltip);

        /// <summary>Update the accessibility tree the platform exposes.</summary>
        /// <param name="tree">The tree.</param>
        void UpdateAccessibility(AccessibleNode tree);

        /// <summary>Say something to screen readers.</summary>
        /// <param name="text">The text.</param>
        void Announce(string text);
    }
}
