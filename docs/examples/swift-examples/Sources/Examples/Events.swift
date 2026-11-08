import Foundation
import Twip

enum EventsExample {
    static func run(_ session: EditorSession) async {
        // #region events
        // Each event comes with the state at its change, in commit order.
        for await (event, state) in session.events {
            switch event {
            case .documentChanged(let record):
                print("revision \(record.revision) (\(record.origin)), \(state.pages.count) pages")
            case .wordAdded(let word, let language):
                print("save \(word) to the \(language) dictionary")
            case .warning(let warning):
                print(warning.message)
            default:
                break
            }
        }
        // #endregion
    }

    static func states(_ session: EditorSession) async {
        // #region states
        // The latest state after each change; a slow reader skips to the newest.
        for await state in session.states {
            print("can undo: \(state.history.canUndo)")
        }
        // #endregion
    }
}
