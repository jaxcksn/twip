#if !NETFRAMEWORK
using System.Diagnostics;
using Xunit;

namespace Twip.Tests;

public class NativeLoadTests
{
    /// <summary>
    /// D3D12 compiles shaders with DXC, which the engine loads by name. The test host runs from the SDK's folder, not
    /// the tests' (as a portable app's native files are under <c>runtimes/</c>), so only the resolver's load from beside
    /// the library puts DXC in the process.
    /// </summary>
    [Fact]
    public void DxcBesideTheLibraryIsLoadedWithIt()
    {
        Native.Require();
        if (!OperatingSystem.IsWindows() || !File.Exists(Path.Combine(AppContext.BaseDirectory, "dxcompiler.dll")))
        {
            Assert.Skip("Windows with dxcompiler.dll beside the library (`moon run twip-capi:build` fetches it)");
        }
        Assert.StartsWith("%PDF", System.Text.Encoding.Latin1.GetString(PdfConverter.Convert(File.ReadAllBytes(Native.Fixture("FX-TEXT.docx"))).Bytes[..4]));
        var modules = Process.GetCurrentProcess().Modules.Cast<ProcessModule>().Select(m => m.ModuleName);
        Assert.Contains(modules, m => string.Equals(m, "dxcompiler.dll", StringComparison.OrdinalIgnoreCase));
    }
}
#endif
