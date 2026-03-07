import SwiftUI

/// Settings view for the Karukan input method.
/// Accessible from the input method menu bar icon.
struct SettingsView: View {
    @AppStorage("strategy") private var strategy: String = "adaptive"
    @AppStorage("numCandidates") private var numCandidates: Int = 9
    @AppStorage("learningEnabled") private var learningEnabled: Bool = true

    var body: some View {
        Form {
            Section("Conversion") {
                Picker("Strategy", selection: $strategy) {
                    Text("Adaptive").tag("adaptive")
                    Text("Light (fast)").tag("light")
                    Text("Main (accurate)").tag("main")
                }
                Stepper("Candidates: \(numCandidates)", value: $numCandidates, in: 1...9)
            }

            Section("Learning") {
                Toggle("Enable learning", isOn: $learningEnabled)
            }

            Section {
                Text("Settings are saved to ~/.config/karukan-im/config.toml")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 400, height: 300)
        .padding()
    }
}
