using System;
using System.Drawing;
using Twip;
using Twip.WinForms;

namespace Examples;

static class Rendering
{
    public static DocumentEditor Editor()
    {
        #region rendering
        var editor = new DocumentEditor
        {
            // On by default: Graphite on Dawn (D3D12, D3D11, then Vulkan), else the CPU.
            UseGpu = true,
            // Until set, Windows' caret blink rate; null is a steady caret.
            CaretBlink = TimeSpan.FromMilliseconds(600),
            // Until set, the engine's colours, or the system colours under high contrast.
            OverlayColors = OverlayColors.Default with { Selection = Color.FromArgb(0xFF, 0xD5, 0x4F) },
        };
        editor.HandleCreated += (_, _) => Console.WriteLine($"drawing with {editor.GpuBackend ?? "the CPU"}");
        #endregion
        return editor;
    }
}
