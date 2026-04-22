import SwiftUI

@main
struct LlmimeSettingsApp: App {
    var body: some Scene {
        WindowGroup {
            SettingsView()
        }
        .windowResizability(.contentSize)
    }
}
