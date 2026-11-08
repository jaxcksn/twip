import Foundation
import Testing
@testable import Twip

@Suite struct SessionTests {
    static let fixtures = Repo.path("crates", "twip", "tests", "fixtures")

    @Test func editsSavesAndReopens() async throws {
        let session = EditorSession(options: SessionOptions(author: Author(name: "Swift tests")))
        try await session.open(contentsOf: Self.fixtures.appendingPathComponent("editor/FX-EMPTY.docx"))
        try await session.run(.insertText("Hello from Swift"))
        try await session.batch([.selectAll, .setBold(true)], label: "Bold all")
        #expect(session.state.formatting.bold.isOn)
        #expect(session.state.history.canUndo)

        let url = FileManager.default.temporaryDirectory.appendingPathComponent("twip-swift-\(UUID()).docx")
        defer { try? FileManager.default.removeItem(at: url) }
        let saved = try await session.save(to: url)
        #expect(saved.revision == session.state.revision)

        let again = EditorSession()
        try await again.open(contentsOf: url)
        #expect(try await again.find.next(FindQuery(text: "Hello from Swift")).found)
        #expect(again.state.formatting.bold.isOn)
        await again.close()
        await session.close()
    }

    @Test func eventsCarryTheStateAtTheirRevision() async throws {
        let session = EditorSession()
        let events = session.events
        try await session.run(.insertText("a"))
        try await session.run(.insertText("b"))
        await session.close()
        var revisions: [Int] = []
        for await (event, state) in events {
            if case .documentChanged(let record) = event {
                #expect(state.revision == record.revision)
                revisions.append(record.revision)
            }
        }
        #expect(revisions.count == 2 && revisions[1] > revisions[0])
    }

    @Test func closedSessionsThrowDestroyed() async throws {
        let session = EditorSession()
        await session.close()
        await session.close()
        await #expect(throws: TwipError.self) { try await session.run(.undo) }
    }

    @Test func convertsToPDFWithoutASession() async throws {
        let docx = try Data(contentsOf: Self.fixtures.appendingPathComponent("editor/FX-services.docx"))
        let result = try await PDFConverter.convert(docx)
        #expect(result.pdf.starts(with: Data("%PDF".utf8)))
    }

    /// Each case in balloons.json: its anchors, card heights and gap give its tops.
    @Test func stacksBalloons() throws {
        struct Case: Decodable { let name: String; let anchors: [CommentAnchor]; let heights: [Double]; let gap: Double; let tops: [Double] }
        let data = try Data(contentsOf: Repo.path("crates", "twip", "tests", "wire", "balloons.json"))
        for c in try JSONDecoder().decode([Case].self, from: data) {
            #expect(stackBalloons(c.anchors, heights: c.heights, gap: c.gap) == c.tops, "\(c.name)")
        }
    }

    /// The contract runner fails a script whose expectation doesn't hold, so its passes mean something.
    @Test func contractRunnerCatchesMismatches() async throws {
        let script: [String: Any] = ["name": "self-check", "steps": [
            ["op": "command", "command": ["type": "insertText", "value": "x"]],
            ["op": "expectState", "match": ["revision": 999]],
        ]]
        let run = Script(runner: "swift-unattached", name: "self-check", script: script, attached: false)
        await #expect(throws: ScriptFailure.self) { try await run.run() }
    }
}
