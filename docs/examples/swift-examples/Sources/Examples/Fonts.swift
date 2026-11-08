import Foundation
import Twip

enum FontsExample {
    static func run() async throws {
        // #region fonts
        // The system's fonts, shared by every session; or FontLibrary.empty() to supply every font yourself.
        let fonts = FontLibrary.system
        try await fonts.add(contentsOf: URL(filePath: "Fonts/Carlito-Regular.ttf"))
        fonts.setAliases(["Calibri": "Carlito", "Cambria": "Caladea"])

        // Asked once per family and style the library doesn't have.
        fonts.setResolver { request in
            request.family == "Brand Sans" ? .url(URL(string: "https://example.com/fonts/brand-sans.woff2")!) : nil
        }

        let session = EditorSession(fonts: fonts)
        // #endregion
        _ = session
    }
}
