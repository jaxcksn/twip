// Schema check: every .docx in the directory validates against the OOXML schemas, Word's extensions included.
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;

var dir = args.Length == 1 ? args[0] : throw new ArgumentException("usage: validate <directory>");
var files = Directory.GetFiles(dir, "*.docx").Order().ToArray();
if (files.Length == 0) throw new InvalidOperationException($"no packages in {dir}");
var validator = new OpenXmlValidator(FileFormatVersions.Microsoft365);
var failed = 0;
foreach (var file in files)
{
    using var doc = WordprocessingDocument.Open(file, false);
    var errors = validator.Validate(doc).ToList();
    if (errors.Count == 0) continue;
    failed++;
    Console.Error.WriteLine($"{Path.GetFileName(file)}:");
    foreach (var e in errors.Take(5)) Console.Error.WriteLine($"  {e.Part?.Uri} {e.Path?.XPath}: {e.Description}");
}
Console.WriteLine($"{files.Length - failed} of {files.Length} packages valid");
return failed == 0 ? 0 : 1;
