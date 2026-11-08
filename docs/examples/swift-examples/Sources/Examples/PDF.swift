import Foundation
import Twip

enum PDFExample {
    static func run(_ files: [URL]) async throws {
        // #region convert
        // No session: share one font set, and convert as many documents at once as you like.
        let fonts = try PDFFontSet(.init(includeSystemFonts: true))
        try await withThrowingTaskGroup(of: Void.self) { group in
            for file in files {
                group.addTask {
                    try await PDFConverter.convert(contentsOf: file, to: file.appendingPathExtension("pdf"), fonts: fonts,
                                                   options: PDFOptions(keywords: ["report"]))
                }
            }
            try await group.waitForAll()
        }
        // #endregion
    }
}
