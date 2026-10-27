// Drives the native engine through the generated bindings; fails loudly if the binding layer is broken.
using System.IO.Compression;
using Twip;

using var fonts = Fonts.System();
using var editor = Editor.Create(fonts);
editor.Open(File.ReadAllBytes(Path.Combine(AppContext.BaseDirectory, "hello.docx")));
if (editor.PageCount != 1) throw new Exception($"page count {editor.PageCount}");

// Typing, a command on the selection, and an inserted table all reach the saved file.
editor.Key("End", new Modifiers { Ctrl = true });
editor.Text(" Typed through the SDK.");
editor.SelectAll();
editor.Bold();
editor.Key("End", new Modifiers { Ctrl = true });
editor.InsertTable(2, 3);
using var saved = editor.Save();
var docx = saved.ToArray();
using (var zip = new ZipArchive(new MemoryStream(docx)))
{
    var xml = new StreamReader(zip.GetEntry("word/document.xml")!.Open()).ReadToEnd();
    foreach (var want in new[] { "Typed through the SDK.", "<w:b/>", "<w:tbl>" })
        if (!xml.Contains(want)) throw new Exception($"saved document lacks {want}");
}

// Requests are polled; the caret has a height; a frame paints something other than the background.
using var requests = editor.TakeRequests();
var caret = editor.Caret();
if (caret.Height <= 0) throw new Exception("no caret");
editor.SetViewport(0, 0, 400, 300, 1);
var pixels = new byte[400 * 300 * 4];
if (!editor.Paint(pixels, 400, 300, 0, 0, 1)) throw new Exception("paint failed");
if (pixels.Distinct().Count() < 3) throw new Exception("painted a blank frame");

// Queries: status, a selection round trip through select_range, the reading tree, elements.
var status = editor.Status();
if (status.Page != 1 || status.Pages != 1 || status.Words < 3) throw new Exception($"status {status.Page}/{status.Pages}, {status.Words} words");
editor.SelectAll();
var sel = editor.Selection();
if (sel.Kind != SelectionKind.Text) throw new Exception($"selection {sel.Kind}");
editor.SelectRange(sel.AnchorParagraph, 0, 5);
sel = editor.Selection();
if (sel.AnchorOffset != 0 || sel.FocusOffset != 5) throw new Exception($"select_range gave {sel.AnchorOffset}..{sel.FocusOffset}");
using var tree = editor.AccessibleTree();
if (tree.Role() != "document" || tree.ChildCount == 0) throw new Exception("empty reading tree");
using var elements = editor.Elements("");
if (elements.Len != 0) throw new Exception($"{elements.Len} elements in a plain document");

using var reopened = Editor.Create(fonts);
reopened.Open(docx);
using var pdf = reopened.ExportPdf("Smoke");
var head = System.Text.Encoding.ASCII.GetString(pdf.ToArray(), 0, 5);
if (head != "%PDF-") throw new Exception($"pdf starts {head}");

try { editor.Open([]); throw new Exception("empty input opened"); }
catch (OpenException e) when (e.Inner == OpenError.Empty) { }
try { editor.Open([1, 2, 3]); throw new Exception("junk opened"); }
catch (OpenException e) when (e.Inner == OpenError.Invalid) { }

Console.WriteLine($"{editor.Describe()}, saved {docx.Length} bytes, {requests.Len} request(s), PDF {pdf.Len} bytes");
Console.WriteLine("ok");
