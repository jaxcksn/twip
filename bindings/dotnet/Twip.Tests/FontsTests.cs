using System.Collections.Concurrent;
using Xunit;

namespace Twip.Tests;

/// <summary>Font libraries: the system fonts, added fonts, the resolver, its answers and settings, and missing families.</summary>
public sealed class FontsTests
{
    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    private static string Font(string file) => Native.Fixture(Path.Combine("fonts", file));

    /// <summary>An empty library with the stand-in serif, so results don't depend on the machine's fonts.</summary>
    private static async Task<FontLibrary> StandInAsync()
    {
        var library = FontLibrary.CreateEmpty();
        await library.AddAsync(Font("stand-in-serif.ttf"), Ct);
        return library;
    }

    [Fact]
    public void TheAssemblyContactsNoFontService()
    {
        var bytes = File.ReadAllBytes(typeof(FontLibrary).Assembly.Location);
        foreach (var host in new[] { "fonts.googleapis.com", "fonts.gstatic.com" })
        {
            var utf16 = System.Text.Encoding.Unicode.GetBytes(host);
            var utf8 = System.Text.Encoding.UTF8.GetBytes(host);
            Assert.False(bytes.AsSpan().IndexOf(utf16) >= 0 || bytes.AsSpan().IndexOf(utf8) >= 0, host);
        }
    }

    [Fact]
    public async Task AResolversFailureReachesTheReport()
    {
        Native.Require();
        var library = await StandInAsync();
        library.SetResolver((request, _) => request.Family == "Zzyzx Unknown" ? throw new InvalidOperationException("offline") : new ValueTask<FontSource?>((FontSource?)null));
        await using var session = new EditorSession(new EditorSessionOptions(Fonts: library));
        var changed = new TaskCompletionSource<FontsChangedEventArgs>(TaskCreationOptions.RunContinuationsAsynchronously);
        session.FontsChanged += (_, e) =>
        {
            if (e.Missing.Any(m => m.Error is not null)) changed.TrySetResult(e);
        };
        await session.OpenAsync(Native.Fixture("FX-font-families.docx"), Ct);
        var e = await changed.Task.WaitAsync(TimeSpan.FromSeconds(10), Ct);
        var zzyzx = Assert.Single(e.Missing, m => m.Family == "Zzyzx Unknown");
        Assert.Equal("offline", zzyzx.Error);
    }

    [Fact]
    public async Task TheSystemLibraryHasTheMachinesFonts()
    {
        Native.Require();
        Assert.NotEmpty(await FontLibrary.System.GetFamiliesAsync(Ct));
    }

    [Fact]
    public async Task AddingFontsReachesTheLibrary()
    {
        Native.Require();
        var library = FontLibrary.CreateEmpty();
        Assert.Empty(await library.GetFamiliesAsync(Ct));

        var faces = await library.AddAsync(await File.ReadAllBytesAsync(Font("stand-in-serif.ttf"), Ct), Ct);
        Assert.Contains(faces, f => f.Family == "Stand-in Serif");
        Assert.Contains("Stand-in Serif", await library.GetFamiliesAsync(Ct));

        var error = await Assert.ThrowsAsync<EditorException>(() => library.AddAsync(new byte[] { 1, 2, 3, 4 }, Ct));
        Assert.Equal(EditorErrorCode.InvalidArgument, error.Code);
    }

    [Fact]
    public async Task TheResolverAnswersOncePerRequest()
    {
        Native.Require();
        var library = await StandInAsync();
        var asked = new ConcurrentDictionary<FontRequest, int>();
        var unknown = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        library.SetResolver(async (request, ct) =>
        {
            asked.AddOrUpdate(request, 1, (_, n) => n + 1);
            if (request.Family != "Zzyzx Unknown") return null;
            unknown.TrySetResult();
            return FontSource.FromBytes(await File.ReadAllBytesAsync(Font("zzyzx-unknown.ttf"), ct));
        });

        await using var first = new EditorSession(new EditorSessionOptions(Fonts: library));
        await first.OpenAsync(Native.Fixture("FX-font-families.docx"), Ct);
        await unknown.Task.WaitAsync(TimeSpan.FromSeconds(5), Ct);
        await first.WhenIdleAsync();
        await using var second = new EditorSession(new EditorSessionOptions(Fonts: library));
        await second.OpenAsync(Native.Fixture("FX-font-families.docx"), Ct);
        await second.WhenIdleAsync();
        Assert.All(asked, a => Assert.Equal(1, a.Value));
        Assert.Contains("Zzyzx Unknown", await library.GetFamiliesAsync(Ct));
    }

    [Fact]
    public async Task ReplacementsDrawAndOnlyMissingFamiliesAreUnresolved()
    {
        Native.Require();
        var library = await StandInAsync();
        library.SetReplacements(new Dictionary<string, string> { ["Times New Roman"] = "Stand-in Serif", ["Roboto Slab"] = "Stand-in Serif" });
        await using var session = new EditorSession(new EditorSessionOptions(Fonts: library));
        var changes = new List<FontsChangedEventArgs>();
        session.FontsChanged += (_, e) => changes.Add(e);

        var opened = await session.OpenAsync(Native.Fixture("FX-font-families.docx"), Ct);
        await session.WhenIdleAsync();
        Assert.Contains(opened.Warnings, w => w.Code == WarningCode.FontUnresolved);
        Assert.Equal(["Zzyzx Unknown"], session.State.Fonts.Missing.Select(m => m.Family));
        var missing = Assert.Single(Assert.Single(changes).Missing);
        Assert.Equal(("Zzyzx Unknown", StandIn.LastResort, "Stand-in Serif"), (missing.Family, missing.By, missing.DrawnWith));

        await library.AddAsync(Font("zzyzx-unknown.ttf"), Ct);
        await session.WhenIdleAsync();
        Assert.Empty(session.State.Fonts.Missing);
    }
}
