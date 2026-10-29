// Drives the engine through Twip.Core as a Native AOT app; fails loudly if the binding layer is broken.
using System.IO.Compression;
using Twip;

await using var session = new EditorSession();
var changes = 0;
session.DocumentChanged += (_, e) =>
{
    if (e.Snapshot.DocumentRevision < e.Record.Revision) throw new Exception("an event's snapshot is older than its commit");
    Interlocked.Increment(ref changes);
};

await using (var file = File.OpenRead(Path.Combine(AppContext.BaseDirectory, "hello.docx"))) await session.OpenAsync(file);

// Typing, a command on the selection, and undo and redo all reach the state and the saved file.
await session.Commands.InsertTextAsync("Typed through the SDK. ");
await session.Commands.SelectAllAsync();
await session.Commands.SetBoldAsync(true);
if (!session.State.Formatting.Bold.IsOn) throw new Exception($"bold is {session.State.Formatting.Bold} after SetBold");
await session.Commands.UndoAsync();
if (session.State.Formatting.Bold.IsOn) throw new Exception("bold survived undo");
if (!session.State.History.CanRedo) throw new Exception("nothing to redo after undo");
await session.Commands.RedoAsync();
if (!session.State.Formatting.Bold.IsOn) throw new Exception("redo lost bold");

using var saved = new MemoryStream();
var result = await session.SaveAsync(saved);
using (var zip = new ZipArchive(new MemoryStream(saved.ToArray())))
{
    var xml = new StreamReader(zip.GetEntry("word/document.xml")!.Open()).ReadToEnd();
    foreach (var want in new[] { "Typed through the SDK.", "<w:b/>" })
    {
        if (!xml.Contains(want)) throw new Exception($"saved document lacks {want}");
    }
}

// Notifications arrive on the session's executor, after the operations that caused them.
for (var i = 0; i < 100 && Volatile.Read(ref changes) < 4; i++) await Task.Delay(20);
if (changes < 4) throw new Exception($"{changes} DocumentChanged events for 4 commits");

// The catalog and a service round trip through the source-generated JSON; a service the engine hasn't landed fails
// with a stable code.
if (!session.Catalog.Any(c => c.Id == CommandId.SetBold)) throw new Exception("the catalog has no setBold");
try
{
    await session.Find.NextAsync(new FindQuery("SDK"));
}
catch (EditorException e) when (e.Code == EditorErrorCode.Unsupported) { }

Console.WriteLine($"revision {result.Revision}, saved {saved.Length} bytes, {changes} changes");
Console.WriteLine("ok");
