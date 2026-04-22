import AppKit
import Foundation
import SwiftUI

@_silgen_name("llmime_config_load_json")
private func llmime_config_load_json(_ buf: UnsafeMutablePointer<CChar>?, _ bufLen: UInt32) -> Int32

@_silgen_name("llmime_config_save_settings")
private func llmime_config_save_settings(
    _ mode: UnsafePointer<CChar>?,
    _ modelPath: UnsafePointer<CChar>?,
    _ ollamaEndpoint: UnsafePointer<CChar>?
) -> Int32

@_silgen_name("llmime_config_scan_models_json")
private func llmime_config_scan_models_json(_ buf: UnsafeMutablePointer<CChar>?, _ bufLen: UInt32) -> Int32

@_silgen_name("llmime_config_download_default_model")
private func llmime_config_download_default_model() -> Int32

@_silgen_name("llmime_config_poll_download_progress")
private func llmime_config_poll_download_progress(
    _ downloadedBytes: UnsafeMutablePointer<UInt64>?,
    _ totalBytes: UnsafeMutablePointer<UInt64>?,
    _ status: UnsafeMutablePointer<Int32>?
) -> Int32

private enum LocalLlmMode: String, CaseIterable {
    case ngram = "n_gram"
    case hybrid = "hybrid"
    case localLlm = "local_llm"

    var title: String {
        switch self {
        case .ngram:
            return "N-gram"
        case .hybrid:
            return "ハイブリッド"
        case .localLlm:
            return "ローカルLLM"
        }
    }
}

private struct ScannedModel: Decodable, Identifiable {
    let path: String
    let source: String
    let estimated_ram_gb: Double

    var id: String { path }
}

private struct SettingsPayload: Decodable {
    let mode: String
    let model_path: String?
    let ollama_endpoint: String
}

@MainActor
private final class SettingsViewModel: ObservableObject {
    @Published var mode: LocalLlmMode = .ngram
    @Published var modelPath: String = ""
    @Published var ollamaEndpoint: String = "http://localhost:11434"
    @Published var scannedModels: [ScannedModel] = []
    @Published var downloadProgress: Double = 0.0
    @Published var statusMessage: String = ""

    private var pollTimer: Timer?

    func onAppear() {
        loadSettings()
    }

    func loadSettings() {
        guard let raw = callJsonFunction(llmime_config_load_json) else {
            statusMessage = "設定読み込みに失敗"
            return
        }

        guard let data = raw.data(using: .utf8),
              let payload = try? JSONDecoder().decode(SettingsPayload.self, from: data) else {
            statusMessage = "設定JSONの解析に失敗"
            return
        }

        mode = LocalLlmMode(rawValue: payload.mode) ?? .ngram
        modelPath = payload.model_path ?? ""
        ollamaEndpoint = payload.ollama_endpoint
        statusMessage = "設定を読み込みました"
    }

    func saveSettings() {
        let modeCString = strdup(mode.rawValue)
        let modelCString = modelPath.isEmpty ? nil : strdup(modelPath)
        let endpointCString = strdup(ollamaEndpoint)
        defer {
            if let modeCString { free(modeCString) }
            if let modelCString { free(modelCString) }
            if let endpointCString { free(endpointCString) }
        }

        let ok = llmime_config_save_settings(modeCString, modelCString, endpointCString) == 1
        statusMessage = ok ? "設定を保存しました" : "設定保存に失敗"
    }

    func autoDetectModels() {
        guard let raw = callJsonFunction(llmime_config_scan_models_json),
              let data = raw.data(using: .utf8),
              let models = try? JSONDecoder().decode([ScannedModel].self, from: data) else {
            statusMessage = "モデル自動検出に失敗"
            return
        }
        scannedModels = models
        statusMessage = "\(models.count)件検出"
    }

    func registerCustomModel() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.allowedContentTypes = [.data]

        if panel.runModal() == .OK, let url = panel.url, url.path.lowercased().hasSuffix(".gguf") {
            modelPath = url.path
            saveSettings()
        }
    }

    func startDefaultDownload() {
        guard llmime_config_download_default_model() == 1 else {
            statusMessage = "ダウンロード開始に失敗"
            return
        }
        statusMessage = "ダウンロード開始"
        startPolling()
    }

    func applyScannedModel(_ model: ScannedModel) {
        modelPath = model.path
        saveSettings()
    }

    private func startPolling() {
        pollTimer?.invalidate()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.pollProgress()
            }
        }
    }

    private func pollProgress() {
        var downloaded: UInt64 = 0
        var total: UInt64 = 0
        var status: Int32 = 0
        let ok = llmime_config_poll_download_progress(&downloaded, &total, &status) == 1
        guard ok else {
            return
        }

        if total > 0 {
            downloadProgress = Double(downloaded) / Double(total)
        }

        if status == 2 {
            statusMessage = "ダウンロード完了"
            pollTimer?.invalidate()
        } else if status == -1 {
            statusMessage = "ダウンロード失敗"
            pollTimer?.invalidate()
        }
    }

    private func callJsonFunction(_ fn: (UnsafeMutablePointer<CChar>?, UInt32) -> Int32) -> String? {
        let size = 1024 * 512
        let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: size)
        defer { buffer.deallocate() }
        buffer.initialize(repeating: 0, count: size)
        guard fn(buffer, UInt32(size)) == 1 else {
            return nil
        }
        return String(cString: buffer)
    }
}

struct SettingsView: View {
    @StateObject private var vm = SettingsViewModel()

    var body: some View {
        TabView {
            localLlmTab
                .tabItem { Text("ローカルLLM") }
        }
        .padding(20)
        .frame(minWidth: 760, minHeight: 560)
        .onAppear { vm.onAppear() }
    }

    private var localLlmTab: some View {
        VStack(alignment: .leading, spacing: 16) {
            Picker("モード", selection: $vm.mode) {
                ForEach(LocalLlmMode.allCases, id: \.rawValue) { mode in
                    Text(mode.title).tag(mode)
                }
            }
            .pickerStyle(.segmented)

            HStack {
                Text("モデルパス")
                Text(vm.modelPath.isEmpty ? "未設定" : vm.modelPath)
                    .font(.footnote)
                    .lineLimit(1)
                Spacer()
                Button("自動検出") {
                    vm.autoDetectModels()
                }
                Button(".ggufを選択") {
                    vm.registerCustomModel()
                }
            }

            if !vm.scannedModels.isEmpty {
                List(vm.scannedModels) { model in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(model.path)
                                .font(.caption)
                            Text("\(model.source) / 推定RAM: \(String(format: "%.2f", model.estimated_ram_gb)) GB")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("選択") {
                            vm.applyScannedModel(model)
                        }
                    }
                }
                .frame(height: 180)
            }

            VStack(alignment: .leading) {
                Button("Qwen2.5-1.5B をダウンロード") {
                    vm.startDefaultDownload()
                }
                ProgressView(value: vm.downloadProgress)
            }

            HStack {
                Text("Ollama エンドポイント")
                TextField("http://localhost:11434", text: $vm.ollamaEndpoint)
                    .textFieldStyle(.roundedBorder)
            }

            HStack {
                Spacer()
                Button("保存") {
                    vm.saveSettings()
                }
                .buttonStyle(.borderedProminent)
            }

            Text(vm.statusMessage)
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}
