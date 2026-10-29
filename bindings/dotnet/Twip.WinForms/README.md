# Twip.WinForms

A WinForms `DocumentEditor` control over `Twip.Core`: it draws the pages and rulers, and handles the keyboard, mouse,
IME, clipboard, scrolling and accessibility. Windows only.

> **Alpha.** The API may change between minor versions before 1.0.

```sh
dotnet add package Twip.WinForms
```

```csharp
using Twip;
using Twip.WinForms;

var editor = new DocumentEditor { Dock = DockStyle.Fill };
var bold = new ToolStripButton("Bold");
bold.DataBindings.Add(nameof(bold.CheckState), editor.State, nameof(editor.State.BoldCheckState));
bold.DataBindings.Add(nameof(bold.Enabled), editor.State, nameof(editor.State.CanSetBold));
bold.Click += async (_, _) => await editor.Commands.SetBoldAsync(!editor.State.Bold.IsOn);

await editor.OpenAsync("report.docx");
```

`State` is a `DocumentEditorState`: bindable properties for a toolbar and status bar, raised on the UI thread. The
control creates its own `EditorSession`, or takes yours; disposing it then only detaches, and the session keeps its
document, selection and history.

The control draws on the GPU (Graphite on Dawn: D3D12, then D3D11, then Vulkan) into a child window, and with the
CPU when there is no usable GPU (a remote desktop, a VM), after a device loss, with `UseGpu = false`, or while a
`PaintMarkupPane` handler draws over the frame. `GpuBackend` says which it uses.

On .NET Framework 4.8, build the app for x64, and follow `State.PropertyChanged` for `ToolStripItem`s, which have no
`DataBindings` there (controls bind as above):

```csharp
editor.State.PropertyChanged += (_, _) => (bold.CheckState, bold.Enabled) = (editor.State.BoldCheckState, editor.State.CanSetBold);
```
