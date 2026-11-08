import AppKit
import CoreGraphics
import Foundation
import Metal
import QuartzCore
import Testing
import CTwip
@testable import Twip
@testable import TwipUI

@Suite struct HostingTests {
    /// A frame drawn on the CPU has the page on it: not one colour. `TWIP_FRAME_PNG` names a file to write it to.
    @Test func drawsPages() async throws {
        let session = EditorSession()
        try await session.open(contentsOf: SessionTests.fixtures.appendingPathComponent("editor/FX-services.docx"))
        let frames = Frames()
        let host = try EditorHost(session: session, delegate: frames)
        host.resize(width: 640, height: 480, scale: 2)
        host.redraw()
        await host.flush()
        await host.flush()
        let image = try #require(frames.last)
        #expect(image.width == 1280 && image.height == 960)
        #expect(Self.colours(image) > 2)
        if let path = ProcessInfo.processInfo.environment["TWIP_FRAME_PNG"] {
            let rep = NSBitmapImageRep(cgImage: image)
            try rep.representation(using: .png, properties: [:])?.write(to: URL(filePath: path))
        }
        host.detach()
        await session.close()
    }

    /// With a GPU, frames draw with Metal into the layer; after a device loss they draw on the CPU.
    @Test func drawsWithMetal() async throws {
        guard MTLCreateSystemDefaultDevice() != nil else { return }
        let session = EditorSession()
        try await session.open(contentsOf: SessionTests.fixtures.appendingPathComponent("editor/FX-services.docx"))
        let frames = Frames()
        let host = try EditorHost(session: session, delegate: frames)
        let layer = CAMetalLayer()
        #expect(host.attach(layer) == 4)
        host.resize(width: 640, height: 480, scale: 2)
        await host.flush()
        await host.flush()
        #expect(frames.count > 0 && frames.last == nil, "drawn on the GPU")
        #expect(layer.drawableSize == CGSize(width: 1280, height: 960))

        EngineThread.shared.post { if let (id, raw) = session.native { twip_test_lose_gpu(raw, id) } }
        host.redraw()
        await host.flush()
        await host.flush()
        #expect(frames.last != nil, "drawn on the CPU")
        host.detach()
        await session.close()
    }

    /// One view presents a session at a time.
    @Test @MainActor func onePresentationAtATime() throws {
        let session = EditorSession()
        let view = try DocumentEditorView(session: session)
        #expect(throws: TwipError.self) { try DocumentEditorView(session: session) }
        _ = view
    }

    /// Typing reaches the document through the host.
    @Test func typesThroughTheHost() async throws {
        let session = EditorSession()
        let host = try EditorHost(session: session, delegate: Frames())
        host.resize(width: 400, height: 300, scale: 1)
        host.focus(true)
        host.text("typed")
        await host.flush()
        #expect(try await session.find.next(FindQuery(text: "typed")).found)
        host.detach()
        await session.close()
    }

    private static func colours(_ image: CGImage) -> Int {
        let data = image.dataProvider!.data! as Data
        var seen = Set<UInt32>()
        data.withUnsafeBytes { raw in
            let px = raw.bindMemory(to: UInt32.self)
            for i in stride(from: 0, to: px.count, by: 97) { seen.insert(px[i]) }
        }
        return seen.count
    }
}

private final class Frames: EditorHostDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var image: CGImage?
    private var drawn = 0
    var last: CGImage? { lock.withLock { image } }
    var count: Int { lock.withLock { drawn } }

    func host(_ host: EditorHost, didRequest request: HostRequest) {}
    func host(_ host: EditorHost, didDraw frame: FrameInfo, image: CGImage?) { lock.withLock { self.image = image; drawn += 1 } }
    func host(_ host: EditorHost, didReceive event: EditorEvent) {}
}
